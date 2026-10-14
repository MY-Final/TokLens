use super::*;
use crate::collectors::SyncOptions;
use crate::collectors::test_support::{TempDir, assert_private};
use crate::usage::store::{TotalsFilter, UsageStore};
use rusqlite::Connection;

fn source(dir: &TempDir) -> (std::path::PathBuf, Connection) {
    let path = dir.0.join("opencode.db");
    let conn = Connection::open(&path).expect("source");
    conn.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE session(id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, model TEXT, time_created INTEGER, time_updated INTEGER,
            cost REAL, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER);
        CREATE TABLE session_v2 AS SELECT * FROM session;
        CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
        INSERT INTO session VALUES('s1',NULL,'/fixture','{\"id\":\"model\",\"providerID\":\"provider\"}',1,10,0.5,10,2,3,100,0);
        INSERT INTO session_v2 SELECT * FROM session;
        UPDATE session_v2 SET tokens_input=999, time_updated=9;").expect("schema");
    conn.execute("INSERT INTO message VALUES('m1','s1',1,10,?1)", [r#"{"role":"assistant","content":"PRIVATE_SENTINEL","modelID":"model","providerID":"provider","time":{"created":1,"completed":10},"tokens":{"input":10,"output":2,"reasoning":3,"cache":{"read":100,"write":0},"total":115},"cost":0.5}"#]).expect("message");
    (path, conn)
}

#[test]
fn merges_tables_without_double_counting_and_reads_live_wal() {
    let dir = TempDir::new();
    let (path, source) = source(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = OpenCodeCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.session_count().expect("count"), 1);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        115
    );
    assert_private(&store);
    let size = std::fs::metadata(&path).expect("metadata").len();
    source.execute_batch("UPDATE session_v2 SET tokens_input=20, time_updated=10; UPDATE message SET data=json_set(data,'$.tokens.input',20,'$.tokens.total',125)").expect("WAL update");
    assert_eq!(std::fs::metadata(&path).expect("metadata").len(), size);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("WAL sync");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        125
    );
    assert_eq!(store.message_count().expect("count"), 1);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("repeat");
    assert_eq!(store.message_count().expect("count"), 1);
    source
        .execute_batch("DELETE FROM session; DELETE FROM session_v2; DELETE FROM message")
        .expect("delete fixture sessions");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("deleted source rows");
    assert_eq!(store.session_count().expect("count"), 0);
}

#[test]
fn schema_failure_keeps_existing_snapshot_and_source_is_read_only() {
    let dir = TempDir::new();
    let (path, source) = source(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = OpenCodeCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    source
        .execute_batch("DROP TABLE session; DROP TABLE session_v2")
        .expect("unsupported fixture");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(store.session_count().expect("count"), 1);
    let readonly = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("readonly");
    assert!(readonly.execute("DELETE FROM message", []).is_err());
}

#[test]
fn missing_aggregate_fields_unknown_json_and_nested_models() {
    let dir = TempDir::new();
    let (path, source) = source(&dir);
    source.execute_batch("DROP TABLE session_v2;
        UPDATE session SET tokens_output=NULL;
        UPDATE message SET data=json_set(data,'$.model',json('{\"modelID\":\"nested\",\"providerID\":\"new\"}'));
        INSERT INTO message VALUES('bad','s1',1,1,'{');
        INSERT INTO message VALUES('orphan','missing',1,1,'{\"role\":\"assistant\"}');").expect("fixtures");
    let store = UsageStore::open_in_memory().expect("store");
    let report = OpenCodeCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 2);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT total_tokens FROM sessions", [], |row| row
                .get::<_, Option<i64>>(0))
            .expect("unknown"),
        None
    );
    assert_eq!(store.message_count().expect("count"), 1);
}

#[test]
fn sync_does_not_create_a_sqlite_shared_memory_sidecar() {
    let dir = TempDir::new();
    let (path, source) = source(&dir);
    // Release the writer so any checkpointed WAL/shm can be removed first.
    drop(source);
    let shm = std::path::PathBuf::from(format!("{}-shm", path.to_string_lossy()));
    let _ = std::fs::remove_file(&shm);
    let store = UsageStore::open_in_memory().expect("store");
    OpenCodeCollector::new(&path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert!(
        !shm.exists(),
        "collector must not create a client -shm sidecar"
    );
}
