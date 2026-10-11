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
const HEADER: &str =
    r#"{"type":"session","schema":1,"sessionId":"s1","createdAt":"2026-10-11T10:00:00Z"}"#;
const OP: &str = r#"{"operationId":"op1","aggregation":"operation","usageOrigin":"pi","inputTokens":10,"outputTokens":5,"reasoningTokens":2,"cacheReadTokens":100,"cacheWriteTokens":7,"totalTokens":122,"costStatus":"estimated","cost":{"total":99}}"#;
fn message(id: &str, usage: &str) -> String {
    format!(
        r#"{{"type":"message","id":"{id}","role":"assistant","createdAt":"2026-10-11T10:01:00Z","blocks":[{{"text":"PRIVATE_SENTINEL"}}],"meta":{{"modelId":"synthetic","usage":{usage}}}}}"#
    )
}
fn log(rows: &[String]) -> String {
    HEADER.to_string() + "\n" + &rows.join("\n") + "\n"
}
fn total(store: &UsageStore) -> i64 {
    store
        .totals(&TotalsFilter::default())
        .expect("totals")
        .total_tokens
}
#[test]
fn operation_ledger_replay_and_aggregates_count_physical_calls_once() {
    let dir = TempDir::new();
    let op2 = OP.replace("op1", "op2");
    let aggregate = format!(
        r#"{{"aggregation":"aggregate","inputTokens":20,"outputTokens":10,"reasoningTokens":4,"cacheReadTokens":200,"cacheWriteTokens":14,"totalTokens":244,"operations":[{OP},{op2}]}}"#
    );
    dir.write(
        "one.jsonl",
        &log(&[
            message("m1", OP),
            message("replay", OP),
            message("parent", &aggregate),
        ]),
    );
    let store = UsageStore::open_in_memory().expect("store");
    let collector = PiDesktopCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(store.message_count().expect("count"), 2);
    assert_eq!(total(&store), 244);
    let t = store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals");
    assert_eq!(t.sums.output_tokens, 6);
    assert_eq!(t.sums.reasoning_tokens, 4);
    assert_eq!(t.sums.counted_cache_write_tokens, 14);
    assert_private(&store);
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
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("skip")
            .files_skipped_unchanged,
        1
    );
    collector
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("forced");
    assert_eq!(total(&store), 244);
    // A smaller replay cannot replace the complete physical operation, even across sessions.
    dir.write(
        "two.jsonl",
        &log(&[message("partial", &OP.replace("122", "1"))])
            .replace("\"sessionId\":\"s1\"", "\"sessionId\":\"s2\""),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("cross-session dedup");
    assert_eq!(total(&store), 244);
}
#[test]
fn optional_caches_legacy_ids_residuals_and_unknown_usage() {
    let dir = TempDir::new();
    let plain = r#"{"inputTokens":10,"outputTokens":5,"reasoningTokens":2,"totalTokens":15}"#;
    dir.write(
        "one.jsonl",
        &log(&[message("legacy1", plain), message("legacy2", plain)]),
    );
    let store = UsageStore::open_in_memory().expect("store");
    let collector = PiDesktopCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("legacy");
    assert_eq!(total(&store), 30);
    assert_eq!(store.message_count().expect("count"), 2);
    let aggregate = format!(
        r#"{{"aggregation":"aggregate","inputTokens":20,"outputTokens":10,"reasoningTokens":4,"cacheReadTokens":200,"cacheWriteTokens":14,"totalTokens":244,"operations":[{OP}]}}"#
    );
    dir.write("one.jsonl", &log(&[message("parent", &aggregate)]));
    collector
        .sync(&store, &SyncOptions::default())
        .expect("residual");
    assert_eq!(total(&store), 244);
    assert_eq!(store.message_count().expect("count"), 2);
    dir.write("one.jsonl", &log(&[message("missing", "{}")]));
    collector
        .sync(&store, &SyncOptions::default())
        .expect("unknown");
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("unknown")
            .sessions_without_usage,
        1
    );
    dir.write(
        "one.jsonl",
        &log(&[message(
            "zero",
            r#"{"inputTokens":0,"outputTokens":0,"totalTokens":0}"#,
        )]),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("zero");
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("known zero")
            .sessions_without_usage,
        0
    );
}
#[test]
fn incomplete_tail_unknown_schema_and_source_failure_preserve_watermarks() {
    let dir = TempDir::new();
    let path = dir.write("one.jsonl", &log(&[message("m1", OP)]));
    let store = UsageStore::open_in_memory().expect("store");
    let collector = PiDesktopCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let before = store
        .source_file(Client::PiDesktop, &path.to_string_lossy())
        .expect("source");
    for broken in [
        log(&[message("m1", OP)]) + "{",
        log(&[]).replace("\"schema\":1", "\"schema\":99"),
        String::new(),
    ] {
        std::fs::write(&path, broken).expect("broken fixture");
        assert!(collector.sync(&store, &SyncOptions::default()).is_err());
        assert_eq!(total(&store), 122);
        assert_eq!(
            store
                .source_file(Client::PiDesktop, &path.to_string_lossy())
                .expect("source"),
            before
        );
    }
    assert!(
        PiDesktopCollector::new(dir.0.join("missing"))
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
    assert_eq!(total(&store), 122);
}
#[test]
fn invalid_counts_origins_null_cache_and_unknown_records_are_visible() {
    let dir = TempDir::new();
    let mut rows = vec![message("m1", OP)];
    for usage in [
        OP.replace("\"inputTokens\":10", "\"inputTokens\":-1"),
        OP.replace("\"inputTokens\":10", "\"inputTokens\":9223372036854775808"),
        OP.replace("\"cacheReadTokens\":100", "\"cacheReadTokens\":null"),
        OP.replace("\"usageOrigin\":\"pi\"", "\"usageOrigin\":\"estimated\""),
    ] {
        rows.push(message("bad", &usage));
    }
    rows.push(r#"{"type":"future"}"#.to_string());
    rows.push("invalid row".to_string());
    dir.write("one.jsonl", &log(&rows));
    let store = UsageStore::open_in_memory().expect("store");
    let report = PiDesktopCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 6);
    assert_eq!(total(&store), 122);
    assert_eq!(
        store
            .range_totals(&AnalyticsFilter::default())
            .expect("partial")
            .sessions_partial,
        1
    );
}
#[test]
fn snapshots_append_shrink_delete_and_reopen() {
    let dir = TempDir::new();
    dir.write("one.jsonl", &log(&[message("m1", OP)]));
    let db = dir.0.join("usage.db");
    let collector = PiDesktopCollector::new(&dir.0);
    {
        let store = UsageStore::open(&db).expect("store");
        collector
            .sync(&store, &SyncOptions::default())
            .expect("sync");
    }
    let store = UsageStore::open(&db).expect("reopen");
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("unchanged")
            .files_skipped_unchanged,
        1
    );
    dir.write(
        "one.jsonl",
        &log(&[message("m1", OP), message("m2", &OP.replace("op1", "op2"))]),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("append");
    assert_eq!(total(&store), 244);
    let path = dir.write("one.jsonl", &log(&[message("m1", OP)]));
    collector
        .sync(&store, &SyncOptions::default())
        .expect("shrink");
    assert_eq!(total(&store), 122);
    std::fs::remove_file(path).expect("delete synthetic source");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("delete");
    assert_eq!(store.session_count().expect("empty"), 0);
}
