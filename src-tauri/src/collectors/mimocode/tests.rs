use super::*;
use crate::collectors::{
    SyncOptions,
    test_support::{TempDir, assert_private},
};
use crate::usage::{
    analytics::AnalyticsFilter,
    client::Client,
    store::{TotalsFilter, UsageStore},
};
use rusqlite::Connection;
const USAGE: &str = r#"{"role":"assistant","modelID":"synthetic","providerID":"provider","tokens":{"input":10,"output":2,"reasoning":3,"cache":{"read":100,"write":7},"total":122},"content":"PRIVATE_SENTINEL","cost":999}"#;
fn fixture(dir: &TempDir) -> (std::path::PathBuf, Connection) {
    let path = dir.0.join("mimocode.db");
    let conn = Connection::open(&path).expect("source");
    conn.execute_batch("PRAGMA journal_mode=WAL;
    CREATE TABLE session(id TEXT PRIMARY KEY,parent_id TEXT,directory TEXT,time_created INTEGER,time_updated INTEGER,title TEXT);
    CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,agent_id TEXT,time_created INTEGER,data TEXT);
    CREATE TABLE claude_import(session_id TEXT,message_ids TEXT);CREATE TABLE external_import(session_id TEXT,message_ids TEXT);
    CREATE TABLE part(data TEXT);CREATE TABLE account(access_token TEXT);
    INSERT INTO session VALUES('s1',NULL,'/fixture',1,10,'PRIVATE_SENTINEL');
    INSERT INTO session VALUES('s2',NULL,NULL,1,10,NULL);INSERT INTO session VALUES('s3',NULL,NULL,1,10,NULL);
    INSERT INTO session VALUES('s4',NULL,NULL,1,10,NULL);
    INSERT INTO claude_import VALUES('s2','[\"m3\"]');INSERT INTO external_import VALUES('s3','[\"m4\"]');").expect("schema");
    for (id, sid, agent) in [
        ("m1", "s1", "main"),
        ("m2", "s1", "explore"),
        ("m3", "s2", "main"),
        ("m4", "s3", "main"),
    ] {
        conn.execute(
            "INSERT INTO message VALUES(?1,?2,?3,2,?4)",
            rusqlite::params![id, sid, agent, USAGE],
        )
        .expect("message");
    }
    (path, conn)
}
#[test]
fn counts_native_requests_excludes_imports_and_reads_wal() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MiMoCodeCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.session_count().expect("sessions"), 2);
    assert_eq!(store.message_count().expect("messages"), 2);
    let totals = store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals");
    assert_eq!(totals.sums.total_tokens, 244);
    assert_eq!(totals.sums.reasoning_tokens, 6);
    assert_eq!(totals.sums.counted_cache_write_tokens, 14);
    assert_eq!(totals.sessions_without_usage, 1);
    let sidechains: i64 = store
        .connection_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE is_sidechain=1",
            [],
            |r| r.get(0),
        )
        .expect("sidechain");
    assert_eq!(sidechains, 1);
    assert_private(&store);
    let size = std::fs::metadata(&path).expect("metadata").len();
    conn.execute_batch("UPDATE message SET data=json_set(data,'$.tokens.input',20,'$.tokens.total',132) WHERE id='m1'").expect("update WAL");
    assert_eq!(std::fs::metadata(&path).expect("metadata").len(), size);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("wal");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        254
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("repeat");
    assert_eq!(store.message_count().expect("messages"), 2);
    conn.execute_batch("DELETE FROM message WHERE id='m1'")
        .expect("delete synthetic row");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("correction");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("total")
            .total_tokens,
        122
    );
    assert_eq!(
        conn.query_row("SELECT data FROM message WHERE id='m2'", [], |r| r
            .get::<_, String>(0))
            .expect("readonly"),
        USAGE
    );
    assert_eq!(
        store
            .connection_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE client_cost_usd IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .expect("cost"),
        0
    );
}
#[test]
fn absent_import_tables_old_totals_and_missing_usage() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    conn.execute_batch("DELETE FROM session WHERE id!='s1';DELETE FROM message WHERE id!='m1';DROP TABLE claude_import;DROP TABLE external_import;
    UPDATE message SET data=json_remove(data,'$.tokens.total');").expect("old schema");
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MiMoCodeCollector::new(path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("old total");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("derived")
            .total_tokens,
        122
    );
    conn.execute_batch("UPDATE message SET data=json_remove(data,'$.tokens.output')")
        .expect("missing required");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("missing");
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("unknown")
            .sessions_without_usage,
        1
    );
    conn.execute("UPDATE message SET data=?1",[r#"{"role":"assistant","tokens":{"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0},"total":0}}"#]).expect("zero");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("zero");
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("zero known")
            .sessions_without_usage,
        0
    );
}
#[test]
fn malformed_unknown_overflow_schema_failure_and_missing_root() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MiMoCodeCollector::new(&path);
    for (id, data) in [
        ("invalid", "{"),
        ("negative", r#"{"role":"assistant","tokens":{"input":-1}}"#),
        (
            "overflow",
            r#"{"role":"assistant","tokens":{"input":9223372036854775808}}"#,
        ),
        ("future", r#"{"role":"future"}"#),
    ] {
        conn.execute(
            "INSERT INTO message VALUES(?1,'s1','main',2,?2)",
            rusqlite::params![id, data],
        )
        .expect("invalid fixture");
    }
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 4);
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("partial")
            .sessions_partial,
        1
    );
    let before = store
        .source_file(Client::MimoCode, &path.to_string_lossy())
        .expect("source");
    conn.execute_batch("DROP TABLE message")
        .expect("unsupported schema");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(
        store
            .source_file(Client::MimoCode, &path.to_string_lossy())
            .expect("preserved"),
        before
    );
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("total")
            .total_tokens,
        244
    );
    assert!(
        MiMoCodeCollector::new(dir.0.join("missing.db"))
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
}

#[test]
fn imported_session_continuations_keep_native_requests_only() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    conn.execute(
        "INSERT INTO message VALUES('native','s2','main',3,?1)",
        [USAGE],
    )
    .expect("native continuation");
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MiMoCodeCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("continued session");
    assert_eq!(store.session_count().expect("count"), 3);
    assert_eq!(store.message_count().expect("requests"), 3);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        366
    );
    conn.execute_batch("UPDATE claude_import SET message_ids='invalid'")
        .expect("invalid provenance");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("preserved")
            .total_tokens,
        366
    );
}

#[test]
fn sync_does_not_create_a_sqlite_shared_memory_sidecar() {
    let dir = TempDir::new();
    let (path, conn) = fixture(&dir);
    // Release the writer so any checkpointed WAL/shm can be removed first.
    drop(conn);
    let shm = std::path::PathBuf::from(format!("{}-shm", path.to_string_lossy()));
    let _ = std::fs::remove_file(&shm);
    let store = UsageStore::open_in_memory().expect("store");
    MiMoCodeCollector::new(&path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert!(
        !shm.exists(),
        "collector must not create a client -shm sidecar"
    );
}

#[test]
fn database_resolution_prefers_an_absolute_override_without_a_data_root() {
    use std::path::PathBuf;
    let absolute = std::env::temp_dir().join("mimocode-override.db");
    assert_eq!(
        super::sync::resolve_database(Some(absolute.clone().into_os_string()), None, None),
        Some(absolute)
    );
    assert_eq!(
        super::sync::resolve_database(Some("relative.db".into()), None, None),
        None
    );
    let home = PathBuf::from("home");
    assert_eq!(
        super::sync::resolve_database(
            Some("relative.db".into()),
            Some("xdg".into()),
            Some(home.clone())
        ),
        Some(PathBuf::from("xdg").join("mimocode").join("relative.db"))
    );
    assert_eq!(
        super::sync::resolve_database(None, Some("xdg".into()), Some(home.clone())),
        Some(PathBuf::from("xdg").join("mimocode").join("mimocode.db"))
    );
    assert_eq!(
        super::sync::resolve_database(None, None, Some(home.clone())),
        Some(
            home.join(".local")
                .join("share")
                .join("mimocode")
                .join("mimocode.db")
        )
    );
    assert_eq!(super::sync::resolve_database(None, None, None), None);
    assert_eq!(
        super::sync::resolve_database(Some("".into()), None, Some(home.clone())),
        Some(
            home.join(".local")
                .join("share")
                .join("mimocode")
                .join("mimocode.db")
        )
    );
}
