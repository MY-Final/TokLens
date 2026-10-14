use super::*;
use crate::collectors::{
    SyncOptions,
    test_support::{TempDir, assert_private},
};
use crate::usage::{analytics::AnalyticsFilter, client::Client, store::UsageStore};
use serde_json::{Value, json};

fn item(id: &str, session: &str, kind: &str) -> Value {
    json!({"type":kind,"id":id,"sessionId":session,"timestamp":1791799200000i64,
        "parentId":"same-parent","cwd":"/synthetic-project",
        "content":"PRIVATE_SENTINEL","arguments":"PRIVATE_SENTINEL",
        "providerData":{"messageId":"reused-provider-id","conversationRequestId":"same-turn",
            "traceId":"same-trace","model":"synthetic-model","agent":"craft",
            "usage":{"requests":1,"inputTokens":100,"outputTokens":20,"totalTokens":120,
                "inputTokensDetails":[{"cached_tokens":60}],"outputTokensDetails":[{"reasoning_tokens":5}]},
            "rawUsage":{"prompt_cache_miss_tokens":40,"credit":"PRIVATE_SENTINEL"}},
        "message":{"usage":{"input_tokens":100,"output_tokens":20,"total_tokens":120,
            "cache_read_input_tokens":60},"content":"PRIVATE_SENTINEL"}})
}
fn log(rows: &[Value]) -> String {
    rows.iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
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

#[test]
fn native_items_mirrors_copies_and_shared_provider_ids_count_once() {
    let dir = TempDir::new();
    let a = item("a", "s", "function_call");
    let b = item("b", "s", "message");
    let mut tool = item("tool", "s", "function_call_result");
    tool["providerData"]
        .as_object_mut()
        .expect("provider")
        .remove("usage");
    tool.as_object_mut().expect("record").remove("message");
    tool["output"] = json!("PRIVATE_SENTINEL");
    dir.write("project/one.jsonl", &log(&[a.clone(), b, tool]));
    dir.write(
        "project/copy.jsonl",
        &log(&[a, item("a", "other-session", "function_call")]),
    );
    let store = store();
    let collector = WorkBuddyCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(total(&store), 360);
    let sums = store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals")
        .sums;
    assert_eq!(sums.input_tokens, 120);
    assert_eq!(sums.output_tokens, 45);
    assert_eq!(sums.reasoning_tokens, 15);
    assert_eq!(sums.cache_read_tokens, 180);
    assert_eq!(sums.counted_cache_write_tokens, 0);
    assert_private(&store);
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("skip")
            .files_skipped_unchanged,
        2
    );
    collector
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("rescan");
    assert_eq!(total(&store), 360);
    let detail = store
        .session_detail(
            &AnalyticsFilter {
                client: Some(Client::WorkBuddy),
                ..Default::default()
            },
            "s",
            100,
            0,
        )
        .expect("detail");
    assert_eq!(detail.requests.len(), 2);
    assert_eq!(detail.session.model.as_deref(), Some("synthetic-model"));
}

#[test]
fn missing_zero_and_reported_mismatch_stay_distinct() {
    let dir = TempDir::new();
    let mut zero = item("zero", "zero", "message");
    zero["providerData"]["usage"] = json!({"inputTokens":0,"outputTokens":0,"totalTokens":0});
    zero["message"]["usage"] = json!({});
    let mut unknown = item("unknown", "unknown", "function_call");
    unknown["providerData"]["usage"] = json!({"inputTokens":100});
    unknown["message"]["usage"] = json!({});
    let mut mismatch = item("mismatch", "mismatch", "message");
    mismatch["providerData"]["usage"]["totalTokens"] = json!(999);
    dir.write("a.jsonl", &log(&[zero, unknown, mismatch]));
    let store = store();
    WorkBuddyCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let conn = store.connection_for_tests();
    let totals: Vec<Option<i64>> = conn
        .prepare("SELECT total_tokens FROM sessions ORDER BY client_session_id")
        .expect("query")
        .query_map([], |r| r.get(0))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("values");
    assert_eq!(totals, vec![Some(999), None, Some(0)]);
    assert_eq!(
        conn.query_row(
            "SELECT client_total_mismatch FROM messages WHERE dedup_key='mismatch'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .expect("mismatch"),
        1
    );
}

#[test]
fn normalized_fallback_null_details_and_write_relation_are_visible() {
    let mut fallback = item("old", "s", "message");
    fallback["providerData"]["usage"] = Value::Null;
    fallback["message"]["usage"]["cache_creation_input_tokens"] = json!(10);
    let parsed = parser::read_from(std::io::Cursor::new(log(&[fallback]))).expect("parse");
    let raw = parsed.records[0].counts().expect("counts");
    let usage = crate::usage::tokens::normalize_usage(
        crate::usage::tokens::semantics_for(Client::WorkBuddy),
        &raw,
    );
    assert_eq!(usage.total_tokens, Some(120));
    assert_eq!(usage.buckets.cache_write, Some(10));
    assert!(
        usage
            .unknown_relations
            .contains(&crate::usage::tokens::UNKNOWN_CACHE_WRITE_RELATION)
    );
    assert!(
        usage
            .unknown_relations
            .contains(&crate::usage::tokens::UNKNOWN_REASONING_MISSING)
    );
    let mut one = item("one", "s", "message");
    one["providerData"]["usage"]["inputTokensDetails"] = json!({"cached_tokens":0});
    one["message"]["usage"] = json!({});
    let parsed = parser::read_from(std::io::Cursor::new(log(&[one]))).expect("parse");
    assert_eq!(
        parsed.records[0].counts().expect("counts").cache_read,
        Some(0)
    );
}

#[test]
fn malformed_records_and_aggregate_usage_do_not_invent_requests() {
    let mut aggregate = item("aggregate", "s", "message");
    aggregate["providerData"]["usage"]["requests"] = json!(2);
    let mut overflow = item("overflow", "s", "message");
    overflow["providerData"]["usage"]["inputTokensDetails"] =
        json!([{"cached_tokens":u64::MAX},{"cached_tokens":1}]);
    let mut negative = item("negative", "s", "message");
    negative["providerData"]["usage"]["inputTokens"] = json!(-1);
    let mut missing_id = item("missing", "s", "message");
    missing_id["id"] = Value::Null;
    let text = log(&[
        item("valid", "s", "message"),
        aggregate,
        overflow,
        negative,
        missing_id,
        json!({"type":"future-record","content":"PRIVATE_SENTINEL"}),
    ]) + "not-json\n";
    let dir = TempDir::new();
    dir.write("a.jsonl", &text);
    let store = store();
    let report = WorkBuddyCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 6);
    assert_eq!(store.message_count().expect("count"), 1);
    assert_eq!(total(&store), 120);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT message_coverage FROM sessions", [], |r| r
                .get::<_, String>(0))
            .expect("coverage"),
        "partial"
    );
    assert_private(&store);
}

#[test]
fn unfinished_tail_unknown_format_and_missing_root_preserve_watermark() {
    let dir = TempDir::new();
    let first = log(&[item("a", "s", "message")]);
    let path = dir.write("a.jsonl", &first);
    let store = store();
    let collector = WorkBuddyCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let watermark = store
        .source_file(Client::WorkBuddy, &path.to_string_lossy())
        .expect("watermark")
        .expect("source");
    std::fs::write(&path, first.clone() + "{\"type\":").expect("partial fixture");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(total(&store), 120);
    std::fs::write(&path, "{\"type\":\"message\",\"futureSessionId\":\"s\"}\n")
        .expect("unsupported identity fixture");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(total(&store), 120);
    assert_eq!(
        store
            .source_file(Client::WorkBuddy, &path.to_string_lossy())
            .expect("watermark")
            .expect("source")
            .content_offset,
        watermark.content_offset
    );
    std::fs::write(&path, "{\"type\":\"future-format\"}\n").expect("unknown fixture");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(total(&store), 120);
    std::fs::write(&path, first + &item("b", "s", "function_call").to_string())
        .expect("complete without newline");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("recovery");
    assert_eq!(total(&store), 240);
    assert!(
        WorkBuddyCollector::new(dir.0.join("missing"))
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
    assert_eq!(total(&store), 240);
}

#[test]
fn full_snapshot_tracks_corrections_and_removes_deleted_source_copies() {
    let dir = TempDir::new();
    let a = item("a", "s", "message");
    let b = item("b", "s", "function_call");
    let first = dir.write("a.jsonl", &log(std::slice::from_ref(&a)));
    let second = dir.write("b.jsonl", &log(&[a, b]));
    let store = store();
    let collector = WorkBuddyCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(total(&store), 240);
    std::fs::remove_file(second).expect("remove synthetic source");
    let mut corrected = item("a", "s", "message");
    corrected["timestamp"] = json!(1791799201000i64);
    corrected["providerData"]["usage"]["totalTokens"] = json!(125);
    std::fs::write(first, log(&[corrected])).expect("correction");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("rescan");
    assert_eq!(total(&store), 125);
    assert_eq!(store.message_count().expect("count"), 1);
}

#[test]
fn configured_home_and_empty_override_resolve_only_workbuddy_projects() {
    use std::path::PathBuf;
    let home = PathBuf::from("synthetic-home");
    assert_eq!(
        sync::resolve_root(None, Some(home.clone())),
        Some(home.join(".workbuddy/projects"))
    );
    assert_eq!(
        sync::resolve_root(Some(" ".into()), Some(home.clone())),
        Some(home.join(".workbuddy/projects"))
    );
    assert_eq!(
        sync::resolve_root(Some("custom".into()), Some(home)),
        Some(PathBuf::from("custom/projects"))
    );
    assert_eq!(sync::resolve_root(None, None), None);
}

#[test]
fn read_interruption_never_returns_a_partial_success() {
    struct Interrupted {
        first: std::io::Cursor<Vec<u8>>,
    }
    impl std::io::Read for Interrupted {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("synthetic read failure"))
        }
    }
    impl std::io::BufRead for Interrupted {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            if self.first.position() == self.first.get_ref().len() as u64 {
                Err(std::io::Error::other("synthetic read failure"))
            } else {
                std::io::BufRead::fill_buf(&mut self.first)
            }
        }
        fn consume(&mut self, count: usize) {
            std::io::BufRead::consume(&mut self.first, count);
        }
    }
    let reader = Interrupted {
        first: std::io::Cursor::new(log(&[item("a", "s", "message")]).into_bytes()),
    };
    assert!(parser::read_from(reader).is_err());
}

fn usage_item(id: &str, session: &str, usage: Value) -> Value {
    json!({"type":"message","id":id,"sessionId":session,"timestamp":1791799200000i64,
        "cwd":"/synthetic-project",
        "providerData":{"model":"synthetic-model","agent":"craft","usage":usage},
        "message":{"usage":{}}})
}

#[test]
fn derived_totals_are_derived_and_reported_totals_stay_measured() {
    let dir = TempDir::new();
    // No totalTokens: the total is a deterministic conversion from complete buckets.
    let derived = usage_item(
        "derived",
        "s-derived",
        json!({"inputTokens":10,"outputTokens":20,
            "inputTokensDetails":[{"cached_tokens":0}],
            "outputTokensDetails":[{"reasoning_tokens":0}]}),
    );
    // Client-reported total: keeps the measured level.
    let reported = usage_item(
        "reported",
        "s-reported",
        json!({"inputTokens":10,"outputTokens":20,"totalTokens":30}),
    );
    dir.write("a.jsonl", &log(&[derived, reported]));
    let store = store();
    WorkBuddyCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let conn = store.connection_for_tests();
    let request_kinds: Vec<(String, String)> = conn
        .prepare(
            "SELECT s.client_session_id, m.source_kind FROM messages m
             JOIN sessions s ON s.id = m.session_id ORDER BY s.client_session_id",
        )
        .expect("query")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("values");
    assert_eq!(
        request_kinds,
        vec![
            ("s-derived".into(), "derived".into()),
            ("s-reported".into(), "measured".into()),
        ]
    );
    let session_kinds: Vec<(String, String)> = conn
        .prepare("SELECT client_session_id, source_kind FROM sessions ORDER BY client_session_id")
        .expect("query")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("values");
    assert_eq!(
        session_kinds,
        vec![
            ("s-derived".into(), "derived".into()),
            ("s-reported".into(), "measured".into()),
        ]
    );
}

#[test]
fn one_corrupt_line_only_marks_its_own_file_session_partial() {
    let dir = TempDir::new();
    let clean = usage_item(
        "clean",
        "s-clean",
        json!({"inputTokens":10,"outputTokens":20,"totalTokens":30}),
    );
    let dirty = usage_item(
        "dirty",
        "s-dirty",
        json!({"inputTokens":10,"outputTokens":20,"totalTokens":30}),
    );
    dir.write("clean.jsonl", &log(&[clean]));
    // Non-JSON complete line: counted as unparsed without aborting the file.
    dir.write("dirty.jsonl", &(log(&[dirty]) + "not-json\n"));
    let store = store();
    let report = WorkBuddyCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 1);
    let coverage: Vec<(String, String)> = store
        .connection_for_tests()
        .prepare(
            "SELECT client_session_id, message_coverage FROM sessions ORDER BY client_session_id",
        )
        .expect("query")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("values");
    assert_eq!(
        coverage,
        vec![
            ("s-clean".into(), "complete".into()),
            ("s-dirty".into(), "partial".into()),
        ]
    );
}
