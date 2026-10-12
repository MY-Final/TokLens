use super::*;
use crate::collectors::{
    SyncOptions,
    test_support::{TempDir, assert_private},
};
use crate::usage::store::{TotalsFilter, UsageStore};
use rusqlite::Connection;
use serde_json::json;
use std::path::PathBuf;

fn sdk(dir: &TempDir) -> PathBuf {
    let events = [
        json!({"type":"session.start","timestamp":"2026-10-11T23:59:00Z","data":{"sessionId":"sdk","selectedModel":"synthetic","context":{"cwd":"/synthetic"}}}),
        json!({"type":"user.message","id":"u1","timestamp":"2026-10-11T23:59:01Z","data":{"content":"PRIVATE_SENTINEL"}}),
        json!({"type":"assistant.message","timestamp":"2026-10-11T23:59:02Z","data":{"apiCallId":"a1","model":"synthetic","content":"PRIVATE_SENTINEL"}}),
        json!({"type":"assistant.message","timestamp":"2026-10-11T23:59:03Z","data":{"apiCallId":"a2","model":"synthetic","content":"PRIVATE_SENTINEL"}}),
        json!({"type":"user.message","id":"u2","timestamp":"2026-10-12T00:00:01Z","data":{"content":"PRIVATE_SENTINEL"}}),
        json!({"type":"assistant.message","timestamp":"2026-10-12T00:00:02Z","data":{"apiCallId":"a3","model":"synthetic","content":"PRIVATE_SENTINEL"}}),
        // A persisted duplicate of the same API call is not another request.
        json!({"type":"assistant.message","timestamp":"2026-10-12T00:00:02Z","data":{"apiCallId":"a3","model":"synthetic"}}),
    ];
    dir.write(
        "session-state/sdk/events.jsonl",
        &(events
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"),
    )
}
fn usage(api: &str, input: u64, last: u64) -> String {
    json!({"inputTokens":last,"outputTokens":1,"cacheReadTokens":0,
        "_meta":{"vscode.modelCall":{"schemaVersion":1,"sdkSessionId":"sdk","apiCallId":api},
        "turnTokenTotals":[{"model":"synthetic","inputTokens":input,"outputTokens":20,"cachedTokens":0}],
        "directTurnTokenTotals":[{"inputTokens":99999}],"cost":99999},
        "content":"PRIVATE_SENTINEL"}).to_string()
}
fn host(dir: &TempDir, name: &str, primary: bool) -> (PathBuf, Connection) {
    let path = dir
        .0
        .join(format!("Code/agentSessionData/{name}/session.db"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let conn = Connection::open(&path).expect("synthetic db");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
        CREATE TABLE session_metadata(key TEXT PRIMARY KEY,value TEXT);
        CREATE TABLE turns(id TEXT PRIMARY KEY,event_id TEXT);
        CREATE TABLE turn_usage(turn_id TEXT PRIMARY KEY,usage TEXT);
        CREATE TABLE file_edits(before_content TEXT);
        INSERT INTO file_edits VALUES('PRIVATE_SENTINEL');
        INSERT INTO turns VALUES('outer-turn-1','u1'),('outer-turn-2','u2');",
    )
    .expect("schema");
    conn.execute(
        "INSERT INTO session_metadata VALUES(?1,?2)",
        [
            if primary {
                "defaultChatProviderData"
            } else {
                "agentHost.chatProviderData"
            },
            r#"{"sdkSessionId":"sdk","title":"PRIVATE_SENTINEL"}"#,
        ],
    )
    .expect("link");
    conn.execute(
        "INSERT INTO turn_usage VALUES('outer-turn-1',?1)",
        [usage("a2", 200, 11)],
    )
    .expect("usage");
    conn.execute(
        "INSERT INTO turn_usage VALUES('outer-turn-2',?1)",
        [usage("a3", 300, 22)],
    )
    .expect("usage");
    (path, conn)
}
fn collector(dir: &TempDir) -> CopilotCollector {
    CopilotCollector::new(dir.0.join("session-store.db"))
        .with_agent_host(Some(dir.0.join("Code/agentSessionData")))
}

#[test]
fn cli_sdk_and_host_choose_one_session_and_one_request_ledger() {
    let dir = TempDir::new();
    sdk(&dir);
    let (_path, _host_conn) = host(&dir, "outer", true);
    let path = dir.0.join("session-store.db");
    let conn = Connection::open(&path).expect("synthetic CLI");
    conn.execute_batch("CREATE TABLE sessions(id TEXT,cwd TEXT,created_at TEXT,updated_at TEXT);
        CREATE TABLE assistant_usage_events(id INTEGER,session_id TEXT,agent_id TEXT,parent_tool_call_id TEXT,model TEXT,input_tokens INTEGER,output_tokens INTEGER,cache_read_tokens INTEGER,cache_write_tokens INTEGER,reasoning_tokens INTEGER,created_at TEXT);
        INSERT INTO sessions VALUES('sdk',NULL,NULL,NULL);
        INSERT INTO assistant_usage_events VALUES(1,'sdk',NULL,NULL,'synthetic',100,20,0,0,0,NULL);").expect("CLI fixture");
    let store = UsageStore::open_in_memory().expect("store");
    collector(&dir)
        .sync(&store, &SyncOptions::default())
        .expect("combined sources");
    assert_eq!(store.session_count().expect("sessions"), 1);
    assert_eq!(store.message_count().expect("API calls"), 3);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        540
    );
    // A CLI-only configuration uses the native ledger, never SDK placeholders too.
    CopilotCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("CLI configuration");
    assert_eq!(store.message_count().expect("requests"), 1);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        120
    );
}

#[test]
fn links_sdk_and_outer_ids_counts_turn_totals_once_and_keeps_calls_partial() {
    let dir = TempDir::new();
    sdk(&dir);
    let (path, conn) = host(&dir, "outer", true);
    let (_mirror, _mirror_conn) = host(&dir, "mirror", false);
    let before = std::fs::read(&path).expect("source bytes");
    let wal = std::fs::read(path.with_file_name("session.db-wal")).expect("WAL bytes");
    let shm = std::fs::read(path.with_file_name("session.db-shm")).expect("shm bytes");
    let store = UsageStore::open_in_memory().expect("store");
    for _ in 0..2 {
        collector(&dir)
            .sync(&store, &SyncOptions::default())
            .expect("sync without CLI db");
    }
    assert_eq!(store.session_count().expect("sessions"), 1);
    assert_eq!(store.message_count().expect("requests"), 3);
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(
        (
            totals.input_tokens,
            totals.output_tokens,
            totals.total_tokens
        ),
        (500, 40, 540)
    );
    let db = store.connection_for_tests();
    let detail:(i64,i64,String,String)=db.query_row("SELECT SUM(total_tokens),COUNT(total_tokens),(SELECT message_coverage FROM sessions),(SELECT total_basis FROM sessions) FROM messages",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).expect("detail");
    assert_eq!(detail, (35, 2, "partial".into(), "client_reported".into()));
    assert_private(&store);
    assert_eq!(std::fs::read(&path).expect("read"), before);
    assert_eq!(
        std::fs::read(path.with_file_name("session.db-wal")).expect("read"),
        wal
    );
    assert_eq!(
        std::fs::read(path.with_file_name("session.db-shm")).expect("read"),
        shm
    );
    CopilotCollector::new(dir.0.join("session-store.db"))
        .with_agent_host(Some(dir.0.join("unavailable")))
        .sync(&store, &SyncOptions::default())
        .expect("missing host preserves history");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("preserved totals")
            .total_tokens,
        540
    );
    assert_eq!(store.message_count().expect("preserved details"), 3);
    conn.execute(
        "UPDATE turn_usage SET usage=?1 WHERE turn_id='outer-turn-1'",
        [usage("a2", 400, 13)],
    )
    .expect("WAL correction");
    collector(&dir)
        .sync(&store, &SyncOptions::default())
        .expect("corrected primary overrides stale mirror");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        740
    );
}

#[test]
fn failed_tail_schema_and_identity_links_preserve_successful_history() {
    let dir = TempDir::new();
    let events = sdk(&dir);
    let (_path, conn) = host(&dir, "outer", true);
    let store = UsageStore::open_in_memory().expect("store");
    collector(&dir)
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let old = std::fs::read(&events).expect("events");
    let mut tail = old.clone();
    tail.extend_from_slice(b"{\"type\":");
    std::fs::write(&events, tail).expect("synthetic tail");
    assert!(
        collector(&dir)
            .sync(&store, &SyncOptions::default())
            .is_err()
    );
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        540
    );
    std::fs::write(&events, old).expect("restore fixture");
    conn.execute(
        "UPDATE turns SET event_id='wrong-id' WHERE id='outer-turn-1'",
        [],
    )
    .expect("bad link");
    assert!(
        collector(&dir)
            .sync(&store, &SyncOptions::default())
            .is_err()
    );
    conn.execute("UPDATE turns SET event_id='u1' WHERE id='outer-turn-1'", [])
        .expect("repair");
    conn.execute_batch("ALTER TABLE turn_usage RENAME COLUMN usage TO unsupported")
        .expect("new format");
    assert!(
        collector(&dir)
            .sync(&store, &SyncOptions::default())
            .is_err()
    );
    assert_eq!(store.message_count().expect("history"), 3);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        540
    );
}

#[test]
fn unknown_cache_missing_turns_zero_and_invalid_counts_never_fabricate_tokens() {
    let dir = TempDir::new();
    sdk(&dir);
    let (_path, conn) = host(&dir, "outer", true);
    let store = UsageStore::open_in_memory().expect("store");
    let mut value: serde_json::Value =
        serde_json::from_str(&usage("a2", 200, 11)).expect("fixture json");
    value["_meta"]["turnTokenTotals"][0]["cachedTokens"] = json!(30);
    conn.execute(
        "UPDATE turn_usage SET usage=?1 WHERE turn_id='outer-turn-1'",
        [value.to_string()],
    )
    .expect("cache");
    collector(&dir)
        .sync(&store, &SyncOptions::default())
        .expect("unknown relation");
    let db = store.connection_for_tests();
    assert_eq!(
        db.query_row("SELECT total_tokens FROM sessions", [], |r| r
            .get::<_, Option<i64>>(0))
            .expect("unknown"),
        None
    );
    value["_meta"]["turnTokenTotals"][0]["cachedTokens"] = json!(0);
    value["_meta"]["turnTokenTotals"][0]["inputTokens"] = json!(-1);
    conn.execute(
        "UPDATE turn_usage SET usage=?1 WHERE turn_id='outer-turn-1'",
        [value.to_string()],
    )
    .expect("invalid");
    assert!(
        collector(&dir)
            .sync(&store, &SyncOptions::default())
            .is_err()
    );
    conn.execute_batch("DELETE FROM turn_usage WHERE turn_id='outer-turn-1'")
        .expect("missing turn");
    collector(&dir)
        .sync(&store, &SyncOptions::default())
        .expect("missing stays unknown");
    assert_eq!(
        db.query_row("SELECT total_tokens FROM sessions", [], |r| r
            .get::<_, Option<i64>>(0))
            .expect("unknown"),
        None
    );
    assert_private(&store);
}
