use super::*;
use crate::collectors::SyncOptions;
use crate::collectors::test_support::{TempDir, assert_private};
use crate::usage::client::Client;
use crate::usage::store::{TotalsFilter, UsageStore};

fn assistant(id: &str, output: u64, sidechain: bool) -> String {
    format!(
        "{{\"type\":\"assistant\",\"sessionId\":\"s1\",\"timestamp\":\"2026-01-02T00:00:00Z\",\"cwd\":\"/fixture\",\"isSidechain\":{sidechain},\"message\":{{\"id\":\"{id}\",\"model\":\"claude-test\",\"content\":\"PRIVATE_SENTINEL\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{output},\"cache_read_input_tokens\":100,\"cache_creation_input_tokens\":5}}}}}}\n"
    )
}

#[test]
fn cross_file_duplicates_subagents_incremental_and_truncation() {
    let dir = TempDir::new();
    let first = assistant("m1", 2, false);
    let main = dir.write("project/main.jsonl", &format!("{first}{first}"));
    let child = dir.write(
        "project/subagents/agent-1.jsonl",
        &format!("{first}{}", assistant("m2", 3, true)),
    );
    let store = UsageStore::open_in_memory().expect("store");
    let collector = ClaudeCodeCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.messages_inserted, 2);
    let child_source: String = store.connection_for_tests().query_row(
        "SELECT f.path FROM messages m JOIN source_files f ON f.id=m.source_file_id WHERE m.dedup_key='m2'",
        [], |row| row.get(0)).expect("provenance");
    assert_eq!(
        std::fs::canonicalize(child_source).expect("source path"),
        std::fs::canonicalize(&child).expect("fixture path")
    );
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        235
    );
    assert_eq!(
        store
            .connection_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE is_sidechain=1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("sidechains"),
        1
    );
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("unchanged")
            .files_skipped_unchanged,
        2
    );
    std::fs::remove_file(child).expect("delete synthetic child");
    std::fs::write(&main, assistant("m1", 1, false)).expect("truncate fixture");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("rescan");
    assert_eq!(store.message_count().expect("count"), 1);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("totals")
            .total_tokens,
        116
    );
    assert_private(&store);
    std::fs::write(main, "").expect("empty fixture");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("empty snapshot");
    assert_eq!(store.session_count().expect("count"), 0);
}

#[test]
fn malformed_unknown_missing_and_partial_tail() {
    let dir = TempDir::new();
    let path = dir.write("p/s.jsonl", &format!("{}not json\n{{\"type\":\"future\"}}\n{{\"type\":\"assistant\",\"sessionId\":\"s1\",\"message\":{{\"id\":\"missing\"}}}}\n", assistant("m1", 2, false)));
    let store = UsageStore::open_in_memory().expect("store");
    let collector = ClaudeCodeCollector::new(&dir.0);
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("sync")
            .unparsed_lines,
        2
    );
    assert_eq!(store.message_count().expect("count"), 2);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT total_tokens FROM sessions", [], |row| row
                .get::<_, Option<i64>>(0))
            .expect("unknown total"),
        None
    );
    let before = store
        .source_file(Client::ClaudeCode, &path.to_string_lossy())
        .expect("watermark");
    std::fs::write(&path, format!("{}{{\"type\":", assistant("m1", 10, false)))
        .expect("partial tail");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(
        store
            .source_file(Client::ClaudeCode, &path.to_string_lossy())
            .expect("watermark"),
        before
    );
    assert_eq!(store.message_count().expect("count"), 2);
}

#[test]
fn missing_root_keeps_existing_data_and_reopened_watermarks() {
    let dir = TempDir::new();
    dir.write("p/s.jsonl", &assistant("m1", 2, false));
    let database = dir.0.join("toklens.sqlite");
    let collector = ClaudeCodeCollector::new(&dir.0);
    {
        let store = UsageStore::open(&database).expect("store");
        collector
            .sync(&store, &SyncOptions::default())
            .expect("sync");
    }
    let store = UsageStore::open(&database).expect("reopen");
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("skip")
            .files_skipped_unchanged,
        1
    );
    assert!(
        ClaudeCodeCollector::new(dir.0.join("absent"))
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
    assert_eq!(store.session_count().expect("count"), 1);
}

#[test]
fn mixed_timestamp_precision_uses_time_order_for_session_bounds() {
    let dir = TempDir::new();
    let first = assistant("m1", 2, false);
    let last =
        assistant("m2", 3, false).replace("2026-01-02T00:00:00Z", "2026-01-02T00:00:00.001Z");
    dir.write("session.jsonl", &format!("{first}{last}"));
    let store = UsageStore::open_in_memory().expect("store");
    ClaudeCodeCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let bounds: (i64, i64) = store
        .connection_for_tests()
        .query_row(
            "SELECT started_at_ms, ended_at_ms FROM sessions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("bounds");
    assert_eq!(bounds.1 - bounds.0, 1);
}
