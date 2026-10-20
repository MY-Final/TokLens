use super::*;
use crate::collectors::SyncOptions;
use crate::collectors::test_support::{TempDir, assert_private};
use crate::usage::store::{TotalsFilter, UsageStore};
use rusqlite::Connection;

/// 客户端库结构：会话表（新旧两代）与旧消息表。
fn schema(conn: &Connection) {
    conn.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE session(id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, model TEXT, time_created INTEGER, time_updated INTEGER,
            cost REAL, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER);
        CREATE TABLE session_v2 AS SELECT * FROM session;
        CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);")
        .expect("schema");
}

/// 新消息表结构：`type` 列记录消息种类，`data` 里是嵌套模型与用量。
fn session_message_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE session_message(id TEXT PRIMARY KEY, session_id TEXT, type TEXT, seq INTEGER,
            time_created INTEGER, time_updated INTEGER, data TEXT);",
    )
    .expect("session_message schema");
}

fn source(dir: &TempDir) -> (std::path::PathBuf, Connection) {
    let path = dir.0.join("opencode.db");
    let conn = Connection::open(&path).expect("source");
    schema(&conn);
    conn.execute_batch("INSERT INTO session VALUES('s1',NULL,'/fixture','{\"id\":\"model\",\"providerID\":\"provider\"}',1,10,0.5,10,2,3,100,0);
        INSERT INTO session_v2 SELECT * FROM session;
        UPDATE session_v2 SET tokens_input=999, time_updated=9;").expect("schema");
    conn.execute("INSERT INTO message VALUES('m1','s1',1,10,?1)", [r#"{"role":"assistant","content":"PRIVATE_SENTINEL","modelID":"model","providerID":"provider","time":{"created":1,"completed":10},"tokens":{"input":10,"output":2,"reasoning":3,"cache":{"read":100,"write":0},"total":115},"cost":0.5}"#]).expect("message");
    (path, conn)
}

/// 新表来源：会话累计与三条带用量的消息，外加各类非请求消息。
fn session_message_source(dir: &TempDir) -> (std::path::PathBuf, Connection) {
    let path = dir.0.join("opencode.db");
    let conn = Connection::open(&path).expect("source");
    schema(&conn);
    session_message_schema(&conn);
    conn.execute_batch("INSERT INTO session VALUES('s1',NULL,'/fixture','{\"id\":\"model\",\"providerID\":\"provider\"}',1,10,0.5,31,9,3,303,0);
        INSERT INTO session_v2 SELECT * FROM session;").expect("schema");
    // 旧表历史消息：与新表的 m1 同一个 id。
    conn.execute("INSERT INTO message VALUES('m1','s1',1,10,?1)", [r#"{"role":"assistant","content":"PRIVATE_SENTINEL","modelID":"model","providerID":"provider","time":{"created":1,"completed":10},"tokens":{"input":10,"output":2,"reasoning":3,"cache":{"read":100,"write":0},"total":115},"cost":0.5}"#]).expect("message");
    // 新表：模型嵌套在 data.model 里，用量里没有 total 字段。
    let rows = [
        (
            "m1",
            "assistant",
            r#"{"content":"PRIVATE_SENTINEL","model":{"id":"model","providerID":"provider"},"time":{"created":1,"completed":10},"tokens":{"input":10,"output":2,"reasoning":3,"cache":{"read":100,"write":0}},"cost":0.5}"#,
        ),
        (
            "m2",
            "assistant",
            r#"{"model":{"id":"model","providerID":"provider"},"time":{"created":2,"completed":20},"tokens":{"input":20,"output":5,"reasoning":0,"cache":{"read":200,"write":0}},"cost":0.25}"#,
        ),
        (
            "c1",
            "compaction",
            r#"{"model":{"id":"model","providerID":"provider"},"time":{"created":3,"completed":30},"tokens":{"input":1,"output":2,"reasoning":0,"cache":{"read":3,"write":0}},"cost":0.125}"#,
        ),
        (
            "u1",
            "user",
            r#"{"text":"PRIVATE_SENTINEL","time":{"created":4}}"#,
        ),
        (
            "sys1",
            "system",
            r#"{"text":"PRIVATE_SENTINEL","time":{"created":5}}"#,
        ),
        (
            "sy1",
            "synthetic",
            r#"{"text":"PRIVATE_SENTINEL","time":{"created":6}}"#,
        ),
        ("id1", "idle", r#"{"outcome":"idle","time":{"created":7}}"#),
        (
            "ag1",
            "agent-switched",
            r#"{"agent":"plan","previous":"build","time":{"created":8}}"#,
        ),
        (
            "mo1",
            "model-switched",
            r#"{"model":{"id":"model"},"previous":{"id":"old"},"time":{"created":9}}"#,
        ),
        (
            "c2",
            "compaction",
            r#"{"reason":"PRIVATE_SENTINEL","time":{"created":11}}"#,
        ),
        ("x1", "future-kind", r#"{"time":{"created":12}}"#),
    ];
    for (id, kind, data) in rows {
        conn.execute(
            "INSERT INTO session_message VALUES(?1,'s1',?2,1,1,10,?3)",
            [id, kind, data],
        )
        .expect("session_message row");
    }
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
fn session_message_rows_are_imported_and_merged_with_the_legacy_table() {
    let dir = TempDir::new();
    let (path, _source) = session_message_source(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let report = OpenCodeCollector::new(&path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    // 新表的 m1 与旧表同一个 id，只保留一条明细；会话控制类消息不计入。
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(report.unparsed_lines, 1, "未知消息种类只计数不导入");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        346,
        "会话累计与三条明细求和一致"
    );
    let model: String = store
        .connection_for_tests()
        .query_row("SELECT model FROM messages LIMIT 1", [], |row| row.get(0))
        .expect("model");
    assert_eq!(model, "provider/model", "嵌套模型字段被正确读取");
    assert_private(&store);
}

#[test]
fn session_message_only_database_is_supported() {
    let dir = TempDir::new();
    let path = dir.0.join("opencode.db");
    let source = Connection::open(&path).expect("source");
    schema(&source);
    session_message_schema(&source);
    source
        .execute_batch("INSERT INTO session VALUES('s1',NULL,'/fixture','{\"id\":\"model\"}',1,10,0.5,10,2,3,100,0);")
        .expect("session");
    source
        .execute("INSERT INTO session_message VALUES('m1','s1','assistant',1,1,10,?1)", [r#"{"model":{"id":"model","providerID":"provider"},"time":{"created":1,"completed":10},"tokens":{"input":10,"output":2,"reasoning":3,"cache":{"read":100,"write":0}}}"#])
        .expect("row");
    let store = UsageStore::open_in_memory().expect("store");
    OpenCodeCollector::new(&path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.message_count().expect("count"), 1);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        115,
        "没有 tokens.total 字段时按分桶求和"
    );
    assert_private(&store);
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

/// 客户端库主文件与旁车文件的内容指纹。
fn source_bytes(path: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    ["", "-wal", "-shm"]
        .into_iter()
        .filter_map(|suffix| {
            let name = format!("{}{suffix}", path.to_string_lossy());
            std::fs::read(&name).ok().map(|bytes| (name, bytes))
        })
        .collect()
}

#[test]
fn a_running_client_is_read_in_place_without_modifying_its_database() {
    let dir = TempDir::new();
    let (path, source) = source(&dir);
    let before = source_bytes(&path);
    let store = UsageStore::open_in_memory().expect("store");
    OpenCodeCollector::new(&path)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    // 客户端正在持有这个库：就地只读打开，不新建任何文件。
    // 客户端数据（主库与 WAL）逐字节不变；共享内存索引里只会多一条读标记，
    // 任何 SQLite 读者都会写它，与客户端自身的行为一致。
    let after = source_bytes(&path);
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>(),
        "no sidecar file may appear beside the client database"
    );
    for suffix in ["", "-wal"] {
        let name = format!("{}{suffix}", path.to_string_lossy());
        assert_eq!(
            before.get(&name),
            after.get(&name),
            "the client database and its WAL must stay untouched"
        );
    }
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        115
    );
    drop(source);
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
