use super::*;
use crate::collectors::{
    SyncOptions,
    test_support::{TempDir, assert_private},
};
use crate::usage::{
    analytics::AnalyticsFilter,
    store::{TotalsFilter, UsageStore},
    time::parse_iso8601_utc_ms,
};
use rusqlite::Connection;

fn fixture(dir: &TempDir) -> (std::path::PathBuf, Connection) {
    let path = dir.0.join("session-store.db");
    let conn = Connection::open(&path).expect("synthetic source");
    conn.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE sessions(id TEXT PRIMARY KEY,cwd TEXT,created_at TEXT,updated_at TEXT,summary TEXT);
        CREATE TABLE assistant_usage_events(id INTEGER PRIMARY KEY,session_id TEXT,agent_id TEXT,
          parent_tool_call_id TEXT,model TEXT,input_tokens INTEGER,output_tokens INTEGER,
          cache_read_tokens INTEGER,cache_write_tokens INTEGER,reasoning_tokens INTEGER,created_at TEXT,
          token_details_json TEXT,total_nano_aiu INTEGER);
        CREATE TABLE turns(user_message TEXT,assistant_response TEXT);
        INSERT INTO turns VALUES('PRIVATE_SENTINEL','PRIVATE_SENTINEL');
        INSERT INTO sessions VALUES('s','/synthetic','2026-10-11T23:59:00Z','2026-10-12 00:00:02','PRIVATE_SENTINEL');
        INSERT INTO sessions VALUES('empty',NULL,NULL,NULL,'PRIVATE_SENTINEL');
        INSERT INTO assistant_usage_events VALUES(1,'s',NULL,NULL,'synthetic',100,20,40,10,5,'2026-10-11T23:59:01Z','PRIVATE_SENTINEL',999);
        INSERT INTO assistant_usage_events VALUES(2,'s','agent','tool','synthetic',100,20,40,10,5,'2026-10-12 00:00:01','PRIVATE_SENTINEL',999);")
        .expect("synthetic schema");
    (path, conn)
}

#[test]
fn native_usage_is_idempotent_private_and_reads_wal_corrections() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = CopilotCollector::new(&path);
    for _ in 0..2 {
        collector
            .sync(&store, &SyncOptions::default())
            .expect("sync");
    }
    assert_eq!(store.session_count().expect("sessions"), 2);
    assert_eq!(store.message_count().expect("messages"), 2);
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(totals.total_tokens, 240);
    assert_eq!(totals.input_tokens, 100);
    assert_eq!(totals.output_tokens, 30);
    assert_eq!(totals.cache_read_tokens, 80);
    assert_eq!(totals.cache_write_tokens, 20);
    assert_eq!(totals.reasoning_tokens, 10);
    let db = store.connection_for_tests();
    let metadata: (i64, String, i64, i64) = db.query_row(
        "SELECT created_at_ms,source_kind,total_tokens,is_sidechain FROM messages WHERE dedup_key='usage:2'", [],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).expect("metadata");
    assert_eq!(
        metadata.0,
        parse_iso8601_utc_ms("2026-10-12T00:00:01Z").expect("time")
    );
    assert_eq!(metadata.1, "derived");
    assert_eq!(metadata.2, 120);
    assert_eq!(metadata.3, 1);
    assert_private(&store);
    let ranges = store
        .range_totals(&AnalyticsFilter::default())
        .expect("range");
    assert_eq!(ranges.sessions_without_usage, 1);
    assert_eq!(ranges.sums.counted_cache_write_tokens, 20);
    conn.execute_batch("DELETE FROM assistant_usage_events WHERE id=1")
        .expect("synthetic correction");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("WAL correction");
    assert_eq!(store.message_count().expect("messages"), 1);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        120
    );
    let counted: i64 = db
        .query_row("SELECT SUM(cache_write_tokens) FROM messages", [], |r| {
            r.get(0)
        })
        .expect("cache");
    assert_eq!(counted, 10);
    assert_eq!(
        conn.query_row("SELECT summary FROM sessions WHERE id='s'", [], |r| r
            .get::<_, String>(0))
            .expect("source intact"),
        "PRIVATE_SENTINEL"
    );
}

#[test]
fn missing_and_zero_usage_invalid_counts_and_missing_time_stay_distinct() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    conn.execute_batch("DELETE FROM assistant_usage_events;
        INSERT INTO assistant_usage_events(id,session_id,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,reasoning_tokens)
        VALUES(1,'s','synthetic',0,0,0,0,0),(2,'s','synthetic',NULL,2,NULL,NULL,NULL),(3,'s','synthetic',-1,2,0,0,0);")
        .expect("synthetic records");
    let store = UsageStore::open_in_memory().expect("store");
    let report = CopilotCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 1);
    assert_eq!(store.message_count().expect("requests"), 2);
    let db = store.connection_for_tests();
    let rows: (Option<i64>, Option<i64>, Option<i64>) = db
        .query_row(
            "SELECT (SELECT total_tokens FROM messages WHERE dedup_key='usage:1'),
         (SELECT total_tokens FROM messages WHERE dedup_key='usage:2'),
         (SELECT total_tokens FROM sessions WHERE client_session_id='s')",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("unknown totals");
    assert_eq!(rows, (Some(0), None, None));
    let missing: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE created_at_ms IS NULL",
            [],
            |r| r.get(0),
        )
        .expect("missing times");
    assert_eq!(missing, 2);
}

#[test]
fn unsupported_database_or_missing_source_preserves_history_and_watermark() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = CopilotCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let old = store
        .source_file(
            crate::usage::client::Client::Copilot,
            &path.to_string_lossy(),
        )
        .expect("source")
        .expect("watermark");
    conn.execute_batch(
        "ALTER TABLE assistant_usage_events RENAME COLUMN session_id TO unsupported_session",
    )
    .expect("unsupported synthetic schema");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(store.message_count().expect("history"), 2);
    assert_eq!(
        store
            .source_file(
                crate::usage::client::Client::Copilot,
                &path.to_string_lossy()
            )
            .expect("source")
            .expect("watermark")
            .last_scan_ms,
        old.last_scan_ms
    );
    assert!(
        CopilotCollector::new(dir.0.join("missing.db"))
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
    assert_eq!(store.message_count().expect("history"), 2);
}

#[test]
fn old_database_without_usage_keeps_sessions_unknown_and_respects_client_home() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    conn.execute_batch("DROP TABLE assistant_usage_events")
        .expect("old synthetic schema");
    let store = UsageStore::open_in_memory().expect("store");
    CopilotCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("old source");
    assert_eq!(store.session_count().expect("sessions"), 2);
    assert_eq!(store.message_count().expect("requests"), 0);
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("totals")
            .sessions_without_usage,
        2
    );
    assert_eq!(
        sync::resolve_database(None, Some(dir.0.clone())),
        Some(dir.0.join(".copilot/session-store.db"))
    );
    assert_eq!(
        sync::resolve_database(Some(dir.0.clone().into_os_string()), None),
        Some(dir.0.join("session-store.db"))
    );
    assert_eq!(
        sync::resolve_database(Some("".into()), Some(dir.0.clone())),
        Some(dir.0.join(".copilot/session-store.db"))
    );
}

#[test]
fn overlapping_counters_near_sqlite_limit_are_not_rejected_as_a_sum() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    conn.execute_batch("DELETE FROM assistant_usage_events")
        .expect("reset synthetic usage");
    conn.execute("INSERT INTO assistant_usage_events(id,session_id,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,reasoning_tokens)
        VALUES(1,'s','synthetic',?1,0,?1,0,0)", [i64::MAX]).expect("overlapping counts");
    let store = UsageStore::open_in_memory().expect("store");
    let report = CopilotCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(store.message_count().expect("requests"), 1);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("total")
            .total_tokens,
        i64::MAX
    );
}

#[test]
fn missing_optional_token_columns_are_read_as_unknown() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    conn.execute_batch(
        "ALTER TABLE assistant_usage_events RENAME COLUMN cache_write_tokens TO unsupported_cache;
         ALTER TABLE assistant_usage_events RENAME COLUMN reasoning_tokens TO unsupported_reasoning;",
    )
    .expect("column variant");
    let store = UsageStore::open_in_memory().expect("store");
    let report = CopilotCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("tolerated column variant");
    assert_eq!(report.files_failed, 0);
    assert_eq!(store.message_count().expect("requests"), 2);
    // Absent caches stay unknown; input + output still yields the derived total.
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        240
    );
}
