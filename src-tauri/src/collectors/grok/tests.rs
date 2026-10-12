use super::*;
use crate::collectors::test_support::{TempDir, assert_private};
use crate::collectors::{SyncError, SyncOptions};
use crate::usage::analytics::AnalyticsFilter;
use crate::usage::client::Client;
use crate::usage::store::UsageStore;
use serde_json::{Value, json};

const USAGE: &str = include_str!("../../../tests/fixtures/grok/usage.json");
const SUMMARY: &str = include_str!("../../../tests/fixtures/grok/summary.json");

fn setup() -> (TempDir, UsageStore, GrokCollector, std::path::PathBuf) {
    let dir = TempDir::new();
    dir.write("workspace/session/usage.json", USAGE);
    // Build with individual components, matching read_dir paths on Windows.
    let path = dir.0.join("workspace").join("session").join("usage.json");
    dir.write("workspace/session/summary.json", SUMMARY);
    let store = UsageStore::open_in_memory().expect("synthetic store");
    let collector = GrokCollector::new(&dir.0);
    (dir, store, collector, path)
}

fn total(store: &UsageStore) -> i64 {
    store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals")
        .sums
        .total_tokens
}

#[test]
fn authoritative_totals_subsets_turns_and_idempotence() {
    let (dir, store, collector, _) = setup();
    // These files contain no source usage and must never be opened by discovery.
    dir.write(
        "workspace/session/chat_history.jsonl",
        "not-json PRIVATE_SENTINEL",
    );
    dir.write("workspace/session/events.jsonl", "PRIVATE_SENTINEL");
    dir.write("workspace/session/updates.jsonl", "PRIVATE_SENTINEL");
    dir.write("workspace/session/prompt_context.json", "PRIVATE_SENTINEL");
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_seen, 2);
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(store.message_count().expect("count"), 3); // Nine calls must not invent nine records.
    assert_eq!(total(&store), 360);
    let conn = store.connection_for_tests();
    assert_eq!(
        conn.query_row("SELECT total_tokens FROM sessions", [], |r| r
            .get::<_, i64>(0))
            .expect("session total"),
        360
    );
    assert_eq!(
        conn.query_row("SELECT message_coverage FROM sessions", [], |r| r
            .get::<_, String>(0))
            .expect("coverage"),
        "complete"
    );
    assert_eq!(
        conn.query_row("SELECT input_tokens FROM sessions", [], |r| r
            .get::<_, i64>(0))
            .expect("input"),
        120
    );
    assert_eq!(
        conn.query_row("SELECT output_tokens FROM sessions", [], |r| r
            .get::<_, i64>(0))
            .expect("output"),
        45
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE source_ref LIKE 'grok.turn.v1:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .expect("unit"),
        3
    );
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
    assert_eq!(store.message_count().expect("count"), 3);
}

#[test]
fn duplicate_turns_and_session_mirrors_are_not_added() {
    let (dir, store, collector, path) = setup();
    let mut source: Value = serde_json::from_str(USAGE).expect("synthetic");
    let duplicate = source["turns"][0].clone();
    source["turns"]
        .as_array_mut()
        .expect("turns")
        .push(duplicate);
    std::fs::write(&path, source.to_string()).expect("duplicate fixture");
    dir.write("second-workspace/copy/usage.json", &source.to_string());
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(total(&store), 360);
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(store.session_count().expect("count"), 1);
    source["turns"][3]["totalTokens"] = json!(121);
    std::fs::write(path, source.to_string()).expect("conflicting duplicate");
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("failed scan report")
            .files_failed,
        1
    );
    assert_eq!(total(&store), 360);
}

#[test]
fn mirror_timestamp_selects_new_snapshot_and_equal_time_conflict_preserves_history() {
    let (dir, store, collector, _) = setup();
    let mut copy: Value = serde_json::from_str(USAGE).expect("synthetic");
    copy["updatedAt"] = json!("2026-10-12T10:02:00Z");
    copy["session"]["totalTokens"] = json!(361);
    let copy_path = dir.write("another-workspace/mirror/usage.json", &copy.to_string());
    collector
        .sync(&store, &SyncOptions::default())
        .expect("newer mirror");
    assert_eq!(store.session_count().expect("count"), 1);
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT total_tokens FROM sessions", [], |r| r
                .get::<_, i64>(0))
            .expect("newer total"),
        361
    );
    copy["updatedAt"] = json!("2026-10-12T10:01:00.123456789Z");
    std::fs::write(copy_path, copy.to_string()).expect("conflicting mirror");
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("failed scan report")
            .files_failed,
        1
    );
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT total_tokens FROM sessions", [], |r| r
                .get::<_, i64>(0))
            .expect("preserved total"),
        361
    );
}

#[test]
fn usage_without_summary_and_empty_turns_preserve_unknown_metadata() {
    let dir = TempDir::new();
    dir.write(
        "workspace/session/usage.json",
        "{\"sessionId\":\"synthetic\",\"session\":{\"totalTokens\":0}}",
    );
    let store = UsageStore::open_in_memory().expect("store");
    GrokCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("summary optional");
    let row = store.connection_for_tests().query_row("SELECT project_path, started_at_ms, model, message_coverage, total_tokens FROM sessions", [], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<i64>>(4)?))).expect("metadata");
    assert_eq!(row, (None, None, None, "none".into(), Some(0)));
    assert_eq!(store.message_count().expect("count"), 0);
}

#[test]
fn session_and_turn_mismatch_is_partial_without_double_counting() {
    let (_dir, store, collector, path) = setup();
    let mut source: Value = serde_json::from_str(USAGE).expect("synthetic");
    source["session"]["totalTokens"] = json!(999);
    std::fs::write(&path, source.to_string()).expect("synthetic correction");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let conn = store.connection_for_tests();
    assert_eq!(
        conn.query_row("SELECT total_tokens FROM sessions", [], |r| r
            .get::<_, i64>(0))
            .expect("session total"),
        999
    );
    assert_eq!(
        conn.query_row("SELECT SUM(total_tokens) FROM messages", [], |r| r
            .get::<_, i64>(0))
            .expect("turn total"),
        360
    );
    assert_eq!(
        conn.query_row("SELECT message_coverage FROM sessions", [], |r| r
            .get::<_, String>(0))
            .expect("coverage"),
        "partial"
    );
    source["session"]["usageIsIncomplete"] = json!(true);
    std::fs::write(path, source.to_string()).expect("incomplete");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync incomplete");
    assert!(
        store
            .connection_for_tests()
            .query_row("SELECT unknown_relations FROM sessions", [], |r| r
                .get::<_, String>(0))
            .expect("unknown")
            .contains("grok.usage_incomplete")
    );
}

#[test]
fn missing_fields_stay_null_zero_is_measured_and_cost_is_not_imported() {
    let (_dir, store, collector, path) = setup();
    let source = json!({"sessionId":"synthetic-grok-session", "session":{"totalTokens":0},
        "turns":[{"turnNumber":1,"totalTokens":0,"endedAt":"invalid-time","costUsdTicks":100},
                 {"turnNumber":2,"inputTokens":0,"outputTokens":0,"cachedReadTokens":0,"cacheCreationTokens":0,"reasoningTokens":0,"totalTokens":0}]});
    std::fs::write(path, source.to_string()).expect("missing fixture");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    let conn = store.connection_for_tests();
    let row = conn.query_row("SELECT input_tokens, output_tokens, cache_read_tokens, reasoning_tokens, total_tokens, created_at_ms, client_cost_usd FROM messages WHERE dedup_key='turn:1'", [], |r| Ok((r.get::<_, Option<i64>>(0)?,r.get::<_, Option<i64>>(1)?,r.get::<_, Option<i64>>(2)?,r.get::<_, Option<i64>>(3)?,r.get::<_, Option<i64>>(4)?,r.get::<_, Option<i64>>(5)?,r.get::<_, Option<f64>>(6)?))).expect("missing query");
    assert_eq!(row, (None, None, None, None, Some(0), None, None));
    assert_eq!(
        conn.query_row(
            "SELECT input_tokens FROM messages WHERE dedup_key='turn:2'",
            [],
            |r| r.get::<_, Option<i64>>(0)
        )
        .expect("zero"),
        Some(0)
    );
}

#[test]
fn large_overlapping_buckets_are_valid_when_native_total_fits_sqlite() {
    let (_dir, store, collector, path) = setup();
    let max = i64::MAX as u64;
    let source = json!({"sessionId":"synthetic-grok-session", "session":{
        "inputTokens":max - 1, "outputTokens":1, "cachedReadTokens":max - 2,
        "reasoningTokens":1, "cacheCreationTokens":0, "totalTokens":max,
        "modelCalls":1, "turnCount":1}, "turns":[{
        "turnNumber":1, "inputTokens":max - 1, "outputTokens":1,
        "cachedReadTokens":max - 2, "reasoningTokens":1, "cacheCreationTokens":0,
        "totalTokens":max, "modelCalls":1, "turnCount":1}]});
    std::fs::write(path, source.to_string()).expect("large synthetic counts");
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("large sync");
    assert_eq!(report.files_failed, 0);
    assert_eq!(report.sessions_imported, 1);
    let conn = store.connection_for_tests();
    let row = conn
        .query_row(
            "SELECT input_tokens, output_tokens, cache_read_tokens, total_tokens FROM sessions",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .expect("large query");
    assert_eq!(row, (1, 0, i64::MAX - 2, i64::MAX));
}

#[test]
fn empty_bad_numeric_and_unsupported_sources_preserve_history_and_watermark() {
    let (_dir, store, collector, path) = setup();
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let watermark = store
        .source_file(Client::Grok, &path.to_string_lossy())
        .expect("watermark")
        .expect("exists");
    for bad in [
        "",
        "{",
        "{}",
        "{\"sessionId\":\"synthetic-grok-session\",\"session\":{\"totalTokens\":-1}}",
        "{\"sessionId\":\"synthetic-grok-session\",\"session\":{\"totalTokens\":18446744073709551615}}",
    ] {
        std::fs::write(&path, bad).expect("bad fixture");
        assert_eq!(
            collector
                .sync(&store, &SyncOptions::default())
                .expect("failed scan report")
                .files_failed,
            1
        );
        assert_eq!(total(&store), 360);
        assert_eq!(
            store
                .source_file(Client::Grok, &path.to_string_lossy())
                .expect("watermark")
                .expect("exists"),
            watermark
        );
    }
    std::fs::write(&path, USAGE).expect("restore");
    std::fs::write(
        path.with_file_name("summary.json"),
        "{\"info\":{\"id\":\"other-session\"}}",
    )
    .expect("wrong identity");
    assert_eq!(
        collector
            .sync(&store, &SyncOptions { force_rescan: true })
            .expect("failed scan report")
            .files_failed,
        1
    );
    assert_eq!(total(&store), 360);
}

#[test]
fn changes_during_scan_missing_root_and_empty_directory_have_distinct_behavior() {
    let (dir, store, collector, path) = setup();
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let watermark = store
        .source_file(Client::Grok, &path.to_string_lossy())
        .expect("watermark")
        .expect("exists");
    let result = collector.sync_with_mutation(&store, || {
        std::fs::write(&path, USAGE.to_owned() + " ").expect("racing append");
    });
    assert!(matches!(result, Err(SyncError::ChangedDuringScan)));
    assert_eq!(total(&store), 360);
    assert_eq!(
        store
            .source_file(Client::Grok, &path.to_string_lossy())
            .expect("watermark")
            .expect("exists"),
        watermark
    );
    assert!(
        GrokCollector::new(dir.0.join("missing"))
            .sync(&store, &SyncOptions::default())
            .expect("missing")
            .root_missing
    );
    assert_eq!(total(&store), 360);
    std::fs::remove_file(&path).expect("remove synthetic ledger");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("complete empty scan");
    assert_eq!(store.session_count().expect("count"), 0);
}

#[test]
fn summary_changes_are_metadata_only_and_chat_changes_do_not_trigger_rescan() {
    let (dir, store, collector, path) = setup();
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    dir.write("workspace/session/chat_history.jsonl", "PRIVATE_SENTINEL");
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("skip")
            .files_skipped_unchanged,
        2
    );
    let mut summary: Value = serde_json::from_str(SUMMARY).expect("fixture");
    summary["info"]["cwd"] = json!("/other-synthetic-project");
    std::fs::write(path.with_file_name("summary.json"), summary.to_string())
        .expect("summary correction");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("metadata refresh");
    assert_eq!(total(&store), 360);
    assert_eq!(
        store
            .connection_for_tests()
            .query_row("SELECT project_path FROM sessions", [], |r| r
                .get::<_, String>(0))
            .expect("project"),
        "/other-synthetic-project"
    );
    assert_private(&store);
}

#[test]
fn home_override_and_discovery_use_only_grok_statistics_layout() {
    use std::path::PathBuf;
    let home = PathBuf::from("synthetic-home");
    assert_eq!(
        discovery::resolve_root(None, Some(home.clone())),
        Some(home.join(".grok/sessions"))
    );
    assert_eq!(
        discovery::resolve_root(Some("custom".into()), Some(home.clone())),
        Some(PathBuf::from("custom/sessions"))
    );
    assert_eq!(
        discovery::resolve_root(Some(" ".into()), Some(home.clone())),
        Some(home.join(".grok/sessions"))
    );
    assert_eq!(discovery::resolve_root(None, None), None);
    let dir = TempDir::new();
    dir.write("usage.json", "bad");
    dir.write("workspace/usage.json", "bad");
    dir.write("workspace/session/extra/usage.json", "bad");
    dir.write("workspace/session/summary.json", SUMMARY);
    assert!(
        discovery::files(&dir.0, &mut Default::default())
            .expect("discover")
            .is_empty()
    );
}
