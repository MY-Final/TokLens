use super::*;
use crate::{
    collectors::{
        SyncError, SyncOptions,
        test_support::{TempDir, assert_private},
    },
    usage::{
        client::Client,
        store::{TotalsFilter, UsageStore},
    },
};
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn fixture(dir: &TempDir) -> (PathBuf, Connection) {
    let path = dir.0.join("runtime-state.sqlite");
    let conn = Connection::open(&path).expect("synthetic SQLite");
    conn.execute_batch(include_str!("../../../tests/fixtures/mcode/schema.sql"))
        .expect("synthetic schema");
    (path, conn)
}
fn source_bytes(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    ["", "-wal", "-shm"]
        .into_iter()
        .filter_map(|suffix| {
            let path = PathBuf::from(format!("{}{suffix}", path.to_string_lossy()));
            std::fs::read(&path).ok().map(|bytes| (path, bytes))
        })
        .collect()
}
fn total(store: &UsageStore) -> i64 {
    store
        .totals(&TotalsFilter::default())
        .expect("totals")
        .total_tokens
}

fn session_total(store: &UsageStore, id: &str) -> Option<i64> {
    store
        .connection_for_tests()
        .query_row(
            "SELECT total_tokens FROM sessions WHERE client='mcode' AND client_session_id=?1",
            [id],
            |row| row.get(0),
        )
        .expect("session total")
}

#[test]
fn public_data_roots_trim_overrides_without_guessing_profile_paths() {
    let home = Some(PathBuf::from("home"));
    let suffix = "v2/sqlite/runtime-state.sqlite";
    assert_eq!(
        discovery::database(None, None, home.clone()),
        Some(PathBuf::from("home/.minimax").join(suffix))
    );
    assert_eq!(
        discovery::database(
            Some(" primary ".into()),
            Some("secondary".into()),
            home.clone()
        ),
        Some(PathBuf::from("primary").join(suffix))
    );
    assert_eq!(
        discovery::database(Some(" ".into()), Some(" secondary ".into()), home),
        Some(PathBuf::from("secondary").join(suffix))
    );
    assert_eq!(discovery::database(None, None, None), None);
}

#[test]
fn committed_wal_full_snapshot_is_private_idempotent_and_keeps_unknown_model() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MCodeCollector::new(&path);
    let before = source_bytes(&path);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync WAL");
    assert_eq!(
        before,
        source_bytes(&path),
        "source main/WAL/shm must not change"
    );
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(report.sessions_imported, 2);
    assert_eq!(
        store.message_count().expect("requests"),
        2,
        "same turn does not dedup distinct requests"
    );
    assert_eq!(total(&store), 342);
    assert_eq!(session_total(&store, "session-empty"), None);
    let db = store.connection_for_tests();
    let (unknown, known, costs): (i64,i64,i64) = db.query_row("SELECT SUM(model IS NULL), SUM(model='synthetic-model'), SUM(client_cost_usd IS NOT NULL) FROM messages",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).expect("metadata");
    assert_eq!((unknown, known, costs), (1, 1, 0));
    assert_private(&store);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("repeat");
    assert_eq!(store.message_count().expect("requests"), 2);
    assert_eq!(total(&store), 342);
    let main_len = std::fs::metadata(&path).expect("metadata").len();
    writer
        .execute_batch("UPDATE local_runtime_token_usage SET input_tokens=30 WHERE id=1")
        .expect("WAL correction");
    assert_eq!(std::fs::metadata(&path).expect("metadata").len(), main_len);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("correction");
    assert_eq!(total(&store), 362);
    writer
        .execute_batch("DELETE FROM local_runtime_token_usage WHERE id=1")
        .expect("synthetic delete");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("snapshot prune");
    assert_eq!(store.message_count().expect("prune"), 1);
    assert_eq!(total(&store), 223);
}

#[test]
fn excludes_uncommitted_wal_pages_and_does_not_create_missing_shm() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    writer.execute_batch("BEGIN IMMEDIATE; INSERT INTO local_runtime_token_usage VALUES(3,'session-a','main','pi-agent','future',NULL,1791849700000,1,1,0,1,0,0,NULL,zeroblob(131072));").expect("uncommitted WAL spill");
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MCodeCollector::new(&path);
    let before = source_bytes(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("committed-only snapshot");
    assert_eq!(source_bytes(&path), before);
    assert_eq!(store.message_count().expect("count"), 2);
    assert_eq!(total(&store), 342);
    writer.execute_batch("COMMIT").expect("commit");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("committed new row");
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(total(&store), 345);
    drop(writer);
    let shm = PathBuf::from(format!("{}-shm", path.to_string_lossy()));
    assert!(!shm.exists());
    let before = source_bytes(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("no shm");
    assert_eq!(source_bytes(&path), before);
    assert!(!shm.exists(), "collector must not create shm");
}

#[test]
fn null_zero_absent_columns_and_no_usage_table_remain_distinct() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MCodeCollector::new(&path);
    writer.execute_batch("UPDATE local_runtime_token_usage SET input_tokens=0,output_tokens=0,reasoning_tokens=0,cache_read_tokens=0,cache_write_tokens=0").expect("zero");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("known zero");
    assert_eq!(session_total(&store, "session-a"), Some(0));
    writer
        .execute_batch("UPDATE local_runtime_token_usage SET output_tokens=NULL WHERE id=1")
        .expect("NULL");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("NULL snapshot");
    assert_eq!(session_total(&store, "session-a"), None);
    let output: Option<i64> = store
        .connection_for_tests()
        .query_row(
            "SELECT output_tokens FROM messages WHERE dedup_key='usage:1'",
            [],
            |row| row.get(0),
        )
        .expect("NULL field");
    assert_eq!(output, None);
    writer
        .execute_batch("ALTER TABLE local_runtime_token_usage DROP COLUMN cache_write_tokens")
        .expect("old schema");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("optional column absent");
    assert_eq!(session_total(&store, "session-a"), None);
    writer
        .execute_batch("DROP TABLE local_runtime_token_usage")
        .expect("metadata only");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("no ledger");
    assert_eq!(store.message_count().expect("no invented requests"), 0);
    assert_eq!(session_total(&store, "session-a"), None);
}

#[test]
fn malformed_negative_overflow_and_unassigned_rows_are_counted() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MCodeCollector::new(path);
    writer.execute_batch("INSERT INTO local_runtime_token_usage(id,session_id,input_tokens,output_tokens,reasoning_tokens,cache_read_tokens,cache_write_tokens) VALUES
      (3,'session-a',-1,1,0,0,0),
      (4,'session-a','not-a-count',1,0,0,0),
      (5,'session-a',9223372036854775807,1,0,0,0),
      (6,'absent-session',1,1,0,0,0);").expect("invalid synthetic counters");
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("count invalid");
    assert_eq!(report.unparsed_lines, 4);
    assert_eq!(store.message_count().expect("valid requests"), 2);
    assert_eq!(session_total(&store, "session-a"), None);
    let coverage: String = store
        .connection_for_tests()
        .query_row(
            "SELECT message_coverage FROM sessions WHERE client_session_id='session-a'",
            [],
            |row| row.get(0),
        )
        .expect("coverage");
    assert_eq!(coverage, "partial");
}

#[test]
fn source_changes_and_schema_failure_preserve_previous_watermark_and_tokens() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MCodeCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let before = store
        .source_file(Client::Mcode, &path.to_string_lossy())
        .expect("watermark");
    let changed = collector.sync_before_commit(&store,|| {
        writer.execute_batch("INSERT INTO local_runtime_token_usage(id,session_id,raw) VALUES(3,'session-a',zeroblob(131072))").expect("change after read");
    });
    assert!(matches!(changed, Err(SyncError::ChangedDuringScan)));
    assert_eq!(
        store
            .source_file(Client::Mcode, &path.to_string_lossy())
            .expect("preserved"),
        before
    );
    assert_eq!(total(&store), 342);
    writer
        .execute_batch("DROP TABLE local_runtime_sessions")
        .expect("unsupported schema");
    let failure = collector
        .sync(&store, &SyncOptions::default())
        .expect("visible schema failure");
    assert_eq!(failure.files_seen, 1);
    assert_eq!(failure.files_failed, 1);
    assert_eq!(failure.files_parsed, 0);
    assert_eq!(
        store
            .source_file(Client::Mcode, &path.to_string_lossy())
            .expect("preserved"),
        before
    );
    assert_eq!(total(&store), 342);
    assert!(
        MCodeCollector::new(dir.0.join("absent.sqlite"))
            .sync(&store, &SyncOptions::default())
            .expect("missing root")
            .root_missing
    );
    assert_eq!(total(&store), 342);
}

#[test]
fn duplicate_physical_identity_rejects_unsupported_ledger_schema() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    writer.execute_batch("CREATE TABLE duplicate_usage AS SELECT * FROM local_runtime_token_usage;INSERT INTO duplicate_usage SELECT * FROM local_runtime_token_usage WHERE id=1;DROP TABLE local_runtime_token_usage;ALTER TABLE duplicate_usage RENAME TO local_runtime_token_usage").expect("invalid identity fixture");
    let store = UsageStore::open_in_memory().expect("store");
    let failure = MCodeCollector::new(path)
        .sync(&store, &SyncOptions::default())
        .expect("visible malformed ledger");
    assert_eq!(failure.files_failed, 1);
    assert_eq!(store.message_count().expect("atomic"), 0);
}

#[test]
fn invalid_database_bytes_report_failed_file_without_replacing_watermark() {
    let dir = TempDir::new();
    let (path, writer) = fixture(&dir);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = MCodeCollector::new(&path);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial snapshot");
    let before = store
        .source_file(Client::Mcode, &path.to_string_lossy())
        .expect("watermark");
    drop(writer);
    std::fs::write(&path, b"synthetic invalid SQLite header").expect("corrupt synthetic source");
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("visible source failure");
    assert_eq!(report.files_seen, 1);
    assert_eq!(report.files_failed, 1);
    assert_eq!(report.files_parsed, 0);
    assert_eq!(
        store
            .source_file(Client::Mcode, &path.to_string_lossy())
            .expect("preserved watermark"),
        before
    );
    assert_eq!(total(&store), 342);
}
