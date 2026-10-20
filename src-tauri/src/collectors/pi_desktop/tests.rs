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

#[test]
fn derived_bucket_totals_are_derived_and_reported_totals_stay_measured() {
    let dir = TempDir::new();
    let header = |id: &str| {
        format!(
            r#"{{"type":"session","schema":1,"sessionId":"{id}","createdAt":"2026-10-11T10:00:00Z"}}"#
        )
    };
    // No totalTokens: the total is a deterministic conversion from complete buckets.
    let derived = r#"{"inputTokens":10,"outputTokens":5,"reasoningTokens":2}"#;
    // Client-reported total: keeps the measured level.
    let reported = r#"{"inputTokens":10,"outputTokens":5,"reasoningTokens":2,"totalTokens":15}"#;
    dir.write(
        "derived.jsonl",
        &(header("s-derived") + "\n" + &message("m-derived", derived) + "\n"),
    );
    dir.write(
        "reported.jsonl",
        &(header("s-reported") + "\n" + &message("m-reported", reported) + "\n"),
    );
    let store = UsageStore::open_in_memory().expect("store");
    PiDesktopCollector::new(&dir.0)
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

/// 版本 3 的会话头：时间用 ISO 字符串，消息行用纪元毫秒。
const V3_HEADER: &str = r#"{"type":"session","version":3,"id":"01a0ad59-59fc-759d-9191-99715872de7f","timestamp":"2026-09-17T03:11:49.244Z","cwd":"D:\\synthetic\\project"}"#;

fn v3_message(id: &str, ts: i64, model: &str, usage: &str) -> String {
    format!(
        r#"{{"type":"message","id":"{id}","parentId":null,"timestamp":{ts},"message":{{"role":"assistant","model":"{model}","usage":{usage},"content":[{{"type":"text","text":"PRIVATE_SENTINEL"}}]}}}}"#
    )
}

const V3_USAGE: &str = r#"{"input":64609,"output":5967,"cacheRead":260224,"cacheWrite":0,"reasoning":2890,"totalTokens":330800,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}"#;

#[test]
fn version_three_sessions_read_usage_models_and_events() {
    let dir = TempDir::new();
    let rows = [
        V3_HEADER.to_string(),
        r#"{"type":"model_change","id":"680190ec","parentId":null,"timestamp":"2026-09-17T03:11:49.302Z","provider":"synthetic","modelId":"synthetic/model"}"#.to_string(),
        r#"{"type":"thinking_level_change","id":"535ef0c1","parentId":"680190ec","timestamp":"2026-09-17T03:11:49.302Z","thinkingLevel":"medium"}"#.to_string(),
        v3_message("m1", 1789614731679, "synthetic/model", V3_USAGE),
        r#"{"type":"message","id":"u1","parentId":null,"timestamp":1789614731000,"message":{"role":"user","content":[{"type":"text","text":"PRIVATE_SENTINEL"}]}}"#.to_string(),
    ];
    dir.write("project/session.jsonl", &(rows.join("\n") + "\n"));
    let store = UsageStore::open_in_memory().expect("store");
    let report = PiDesktopCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    // 模型切换事件不携带用量，不算异常记录。
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(store.message_count().expect("count"), 1);
    // 客户端自报总量：input + output + cacheRead + cacheWrite。
    assert_eq!(total(&store), 330800);
    let t = store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals");
    assert_eq!(t.sums.input_tokens, 64609);
    assert_eq!(t.sums.cache_read_tokens, 260224);
    // 推理是输出的子集，普通输出不重复加推理。
    assert_eq!(t.sums.output_tokens, 5967 - 2890);
    assert_eq!(t.sums.reasoning_tokens, 2890);
    let conn = store.connection_for_tests();
    let (session, project, model): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT client_session_id, project_path, model FROM sessions WHERE client='pi_desktop'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("session");
    assert_eq!(session, "01a0ad59-59fc-759d-9191-99715872de7f");
    assert_eq!(project, Some("D:\\synthetic\\project".to_string()));
    assert_eq!(model, Some("synthetic/model".to_string()));
    let (key, time): (String, Option<i64>) = conn
        .query_row(
            "SELECT dedup_key, created_at_ms FROM messages WHERE client='pi_desktop'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("message");
    assert_eq!(key, "message:m1");
    assert_eq!(time, Some(1789614731679));
    assert_private(&store);
}
