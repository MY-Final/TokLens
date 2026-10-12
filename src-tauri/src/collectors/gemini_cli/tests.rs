use super::*;
use crate::collectors::{
    SyncOptions,
    test_support::{TempDir, assert_private},
};
use crate::usage::{analytics::AnalyticsFilter, client::Client, store::UsageStore};
use serde_json::{Value, json};
use std::io::Cursor;

const JOURNAL: &str = include_str!("../../../tests/fixtures/gemini_cli/journal.jsonl");

fn store() -> UsageStore {
    UsageStore::open_in_memory().expect("synthetic store")
}
fn total(store: &UsageStore) -> i64 {
    store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals")
        .sums
        .total_tokens
}
fn log(rows: &[Value]) -> String {
    rows.iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
fn header(id: &str) -> Value {
    json!({"sessionId":id,"projectHash":"synthetic-hash","startTime":"2026-10-12T10:00:00.000Z","lastUpdated":"2026-10-12T10:00:03.000Z"})
}
fn message(id: &str, total: u64) -> Value {
    json!({"id":id,"type":"gemini","timestamp":"2026-10-12T10:00:01.000Z","model":"synthetic-model","tokens":{"input":100,"output":3,"cached":40,"thoughts":20,"tool":0,"total":total},"content":"PRIVATE_SENTINEL"})
}

#[test]
fn replay_replaces_full_records_updates_metadata_and_ignores_private_fields() {
    let parsed = parser::read_from(Cursor::new(JOURNAL)).expect("parse");
    assert_eq!(parsed.unparsed, 0);
    assert_eq!(parsed.records.len(), 4);
    assert_eq!(parsed.records[1].tokens.expect("usage").total, Some(123));
    assert_eq!(
        parsed.metadata.last_updated.as_deref(),
        Some("2026-10-12T10:00:02.000Z")
    );
    let dir = TempDir::new();
    dir.write("tmp/project/chats/session-one.jsonl", JOURNAL);
    dir.write("history/project/.project_root", "/synthetic-project\n");
    let store = store();
    let collector = GeminiCliCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(report.messages_inserted, 1);
    assert_eq!(total(&store), 123);
    let conn = store.connection_for_tests();
    assert_eq!(
        conn.query_row("SELECT input_tokens FROM messages", [], |row| row
            .get::<_, i64>(0))
            .expect("input"),
        60
    );
    assert_eq!(
        conn.query_row("SELECT project_path FROM sessions", [], |row| row
            .get::<_, String>(0))
            .expect("project"),
        "/synthetic-project"
    );
    assert_private(&store);
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("skip")
            .files_skipped_unchanged,
        1
    );
    collector
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("rescan");
    assert_eq!(store.message_count().expect("count"), 1);
    assert_eq!(total(&store), 123);
}

#[test]
fn set_messages_replaces_snapshot_and_rewind_removes_target_and_later_rows() {
    let parsed = parser::read_from(Cursor::new(log(&[
        header("s"), message("old", 123),
        json!({"$set":{"messages":[message("a", 123), message("b", 123)],"lastUpdated":"2026-10-12T10:00:04.000Z"}}),
        json!({"$rewindTo":"b"}), message("c", 456), message("c", 789),
    ]))).expect("parse");
    assert_eq!(
        parsed
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "c"]
    );
    assert_eq!(parsed.records[1].tokens.expect("usage").total, Some(789));
    let parsed = parser::read_from(Cursor::new(log(&[
        header("s"),
        message("a", 123),
        json!({"$rewindTo":"missing"}),
    ])))
    .expect("clear");
    assert!(parsed.records.is_empty());
    let parsed = parser::read_from(Cursor::new(log(&[
        header("s"),
        message("a", 123),
        json!({"$set":{"messages":[]}}),
    ])))
    .expect("empty replacement");
    assert!(parsed.records.is_empty());
}

#[test]
fn discovery_excludes_antigravity_and_handles_subagents_and_session_copies() {
    let dir = TempDir::new();
    dir.write("tmp/project/chats/session-one.jsonl", JOURNAL);
    dir.write("tmp/project/chats/session-copy.jsonl", JOURNAL);
    dir.write(
        "tmp/project/chats/parent/child.jsonl",
        &log(&[
            json!({"sessionId":"child","projectHash":"synthetic-hash","kind":"subagent"}),
            message("child-response", 123),
        ]),
    );
    dir.write(
        "antigravity/chats/session-private.jsonl",
        "PRIVATE_SENTINEL",
    );
    dir.write("tmp/project/checkpoints/private.jsonl", "PRIVATE_SENTINEL");
    let store = store();
    let report = GeminiCliCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_seen, 3);
    assert_eq!(store.session_count().expect("count"), 2);
    assert_eq!(store.message_count().expect("count"), 2);
    assert_eq!(total(&store), 246);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row(
                "SELECT is_sidechain FROM messages WHERE session_id='gemini_cli:child'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("sidechain"),
        1
    );
    assert_private(&store);
}

#[test]
fn unknown_types_do_not_panic_and_missing_fields_stay_null() {
    let dir = TempDir::new();
    dir.write("tmp/project/chats/session-one.jsonl", &log(&[
        header("s"), json!({"id":"unknown","type":"future-kind","tokens":{"total":999},"content":"PRIVATE_SENTINEL"}),
        json!({"id":"missing","type":"gemini","tokens":{"input":100}}),
        json!({"id":"zero","type":"gemini","tokens":{"input":0,"output":0,"cached":0,"thoughts":0,"tool":0,"total":0}}),
        json!({"$set":{"futureMetadata":"PRIVATE_SENTINEL"}}),
    ]));
    let store = store();
    let report = GeminiCliCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 1);
    assert_eq!(store.message_count().expect("count"), 2);
    let conn = store.connection_for_tests();
    assert_eq!(
        conn.query_row(
            "SELECT total_tokens FROM messages WHERE dedup_key='missing'",
            [],
            |row| row.get::<_, Option<i64>>(0)
        )
        .expect("unknown"),
        None
    );
    assert_eq!(
        conn.query_row(
            "SELECT input_tokens FROM messages WHERE dedup_key='missing'",
            [],
            |row| row.get::<_, Option<i64>>(0)
        )
        .expect("input"),
        None
    );
    assert_eq!(
        conn.query_row(
            "SELECT total_tokens FROM messages WHERE dedup_key='zero'",
            [],
            |row| row.get::<_, Option<i64>>(0)
        )
        .expect("zero"),
        Some(0)
    );
    assert_eq!(
        conn.query_row("SELECT total_tokens FROM sessions", [], |row| row
            .get::<_, Option<i64>>(0))
            .expect("session"),
        None
    );
    assert_private(&store);
}

#[test]
fn empty_new_file_is_safe_and_failed_or_changing_sources_preserve_history() {
    assert!(
        !parser::read_from(Cursor::new("\n"))
            .expect("empty")
            .nonempty
    );
    let dir = TempDir::new();
    let path = "tmp/project/chats/session-one.jsonl";
    dir.write(path, JOURNAL);
    dir.write("tmp/project/chats/session-empty.jsonl", "");
    let store = store();
    let collector = GeminiCliCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("seed");
    let source_path = dir.0.join(path).to_string_lossy().to_string();
    let old = store
        .source_file(Client::GeminiCli, &source_path)
        .expect("source");
    for damage in ["", " \n", "{", "{}\n", "{\"$set\":{\"messages\":[{}]}}\n"] {
        dir.write(path, damage);
        let result = collector.sync(&store, &SyncOptions::default());
        if damage.trim().is_empty() {
            assert!(matches!(
                result,
                Err(crate::collectors::SyncError::ChangedDuringScan)
            ));
        } else {
            assert_eq!(result.expect("visible parse failure").files_failed, 1);
        }
        assert_eq!(total(&store), 123);
        assert_eq!(
            store
                .source_file(Client::GeminiCli, &source_path)
                .expect("source"),
            old
        );
    }
    dir.write(path, JOURNAL);
    let result = collector.sync_before_commit(&store, &SyncOptions { force_rescan: true }, || {
        dir.write(
            path,
            &(JOURNAL.to_string() + "{\"$set\":{\"lastUpdated\":\"2026-10-13T10:00:00.000Z\"}}\n"),
        );
    });
    assert!(matches!(
        result,
        Err(crate::collectors::SyncError::ChangedDuringScan)
    ));
    assert_eq!(total(&store), 123);
}

#[test]
fn successful_corrections_and_removals_replace_instead_of_accumulate() {
    let dir = TempDir::new();
    let path = "tmp/project/chats/session-one.jsonl";
    dir.write(path, JOURNAL);
    let store = store();
    let collector = GeminiCliCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("seed");
    dir.write(
        path,
        &log(&[header("synthetic-session"), message("response-1", 456)]),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("correct");
    assert_eq!(total(&store), 456);
    dir.write(
        path,
        &log(&[header("synthetic-session"), json!({"$set":{"messages":[]}})]),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("rewind");
    assert_eq!(store.message_count().expect("count"), 0);
    std::fs::remove_file(dir.0.join(path)).expect("remove synthetic fixture");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("prune");
    assert_eq!(store.session_count().expect("count"), 0);
}

#[test]
fn default_root_uses_installed_storage_home_contract() {
    assert_eq!(
        discovery::resolve_root(Some("/synthetic-home".into()), Some("/fallback".into())),
        Some(std::path::PathBuf::from("/synthetic-home/.gemini"))
    );
    assert_eq!(
        discovery::resolve_root(Some("".into()), Some("/fallback".into())),
        Some(std::path::PathBuf::from("/fallback/.gemini"))
    );
    assert_eq!(discovery::resolve_root(None, None), None);
}

#[test]
fn missing_roots_and_changed_project_markers_keep_previous_snapshot() {
    let dir = TempDir::new();
    let path = "tmp/project/chats/session-one.jsonl";
    dir.write(path, JOURNAL);
    dir.write("history/project/.project_root", "/synthetic-project");
    let store = store();
    let collector = GeminiCliCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("seed");
    let missing = GeminiCliCollector::new(dir.0.join("absent"));
    assert!(
        missing
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
    assert_eq!(total(&store), 123);
    let result = collector.sync_before_commit(&store, &SyncOptions { force_rescan: true }, || {
        dir.write(
            "history/project/.project_root",
            "/different-synthetic-project",
        );
    });
    assert!(matches!(
        result,
        Err(crate::collectors::SyncError::ChangedDuringScan)
    ));
    assert_eq!(total(&store), 123);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT project_path FROM sessions", [], |row| row
                .get::<_, String>(0))
            .expect("project"),
        "/synthetic-project"
    );
}

#[test]
fn conflicting_mirrors_and_unknown_operations_do_not_overwrite_history() {
    let dir = TempDir::new();
    let path = "tmp/project/chats/session-one.jsonl";
    dir.write(path, &log(&[header("s"), message("a", 123)]));
    let store = store();
    let collector = GeminiCliCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("seed");
    dir.write(
        "tmp/project/chats/session-copy.jsonl",
        &log(&[header("s"), message("a", 456)]),
    );
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("conflict")
            .files_failed,
        1
    );
    assert_eq!(total(&store), 123);
    std::fs::remove_file(dir.0.join("tmp/project/chats/session-copy.jsonl"))
        .expect("remove synthetic mirror");
    dir.write(
        path,
        &log(&[header("s"), json!({"$futureOperation":{"messages":[]}})]),
    );
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("unsupported")
            .files_failed,
        1
    );
    assert_eq!(total(&store), 123);
}
