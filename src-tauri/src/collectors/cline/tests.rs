use super::*;
use crate::collectors::test_support::{TempDir, assert_private};
use crate::collectors::{SyncError, SyncOptions};
use crate::usage::analytics::AnalyticsFilter;
use crate::usage::client::Client;
use crate::usage::store::UsageStore;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::PathBuf;

const MANIFEST: &str = include_str!("../../../tests/fixtures/cline/manifest.json");
const MESSAGES: &str = include_str!("../../../tests/fixtures/cline/messages.json");
const SESSION: &str = "synthetic-session-1";

fn setup() -> (TempDir, UsageStore, ClineCollector) {
    let dir = TempDir::new();
    dir.write(&format!("{SESSION}/{SESSION}.json"), MANIFEST);
    dir.write(&format!("{SESSION}/{SESSION}.messages.json"), MESSAGES);
    let store = UsageStore::open_in_memory().expect("synthetic store");
    let collector = ClineCollector::new(&dir.0);
    (dir, store, collector)
}

fn totals(store: &UsageStore) -> crate::usage::analytics::TokenSums {
    store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals")
        .sums
}

fn session_row(store: &UsageStore, column: &str) -> Value {
    let conn = store.connection_for_tests();
    conn.query_row(
        &format!("SELECT {column} FROM sessions WHERE client = ?1"),
        [Client::Cline.as_str()],
        |row| match column {
            "message_coverage" | "total_basis" | "source_kind" | "unknown_relations" => {
                Ok(Value::String(row.get(0)?))
            }
            _ => Ok(Value::from(row.get::<_, i64>(0)?)),
        },
    )
    .expect("session row")
}

#[test]
fn session_totals_subtract_caches_from_input_and_keep_every_bucket_once() {
    let (_dir, store, collector) = setup();
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_seen, 2);
    assert_eq!(report.files_parsed, 2);
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(report.files_failed, 0);
    assert_eq!(store.message_count().expect("count"), 3);
    let sums = totals(&store);
    // The client reports input 350 (including 190 cache read and 5 cache write) and output 35.
    assert_eq!(sums.input_tokens, 155);
    assert_eq!(sums.output_tokens, 35);
    assert_eq!(sums.cache_read_tokens, 190);
    assert_eq!(sums.cache_write_tokens, 5);
    assert_eq!(sums.counted_cache_write_tokens, 5);
    // No bucket is counted twice: uncached input + both caches + output equals client input + output.
    assert_eq!(sums.total_tokens, 385);
    assert_eq!(
        session_row(&store, "message_coverage"),
        Value::String("complete".into())
    );
    assert_eq!(
        session_row(&store, "total_basis"),
        Value::String("client_cumulative".into())
    );
    assert_eq!(
        session_row(&store, "source_kind"),
        Value::String("derived".into())
    );
    assert_private(&store);
}

#[test]
fn repeated_scans_do_not_duplicate_usage() {
    let (_dir, store, collector) = setup();
    collector
        .sync(&store, &SyncOptions::default())
        .expect("first sync");
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("second sync");
    assert_eq!(report.files_skipped_unchanged, 2);
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(totals(&store).total_tokens, 385);
}

#[test]
fn rewritten_manifest_replaces_the_previous_totals() {
    let (dir, store, collector) = setup();
    collector
        .sync(&store, &SyncOptions::default())
        .expect("first sync");
    let mut manifest: Value = serde_json::from_str(MANIFEST).expect("manifest");
    manifest["metadata"]["usage"]["inputTokens"] = json!(400);
    manifest["metadata"]["usage"]["cacheReadTokens"] = json!(200);
    dir.write(
        &format!("{SESSION}/{SESSION}.json"),
        &serde_json::to_string_pretty(&manifest).expect("json"),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("second sync");
    let sums = totals(&store);
    assert_eq!(sums.input_tokens, 195);
    assert_eq!(sums.cache_read_tokens, 200);
    assert_eq!(sums.total_tokens, 435);
    assert_eq!(store.message_count().expect("count"), 3);
}

#[test]
fn detail_that_disagrees_with_the_cumulative_total_marks_the_session_partial() {
    let (dir, store, collector) = setup();
    let mut manifest: Value = serde_json::from_str(MANIFEST).expect("manifest");
    // The client cumulative value no longer matches the sum of the stored turns.
    manifest["metadata"]["usage"]["outputTokens"] = json!(99);
    dir.write(
        &format!("{SESSION}/{SESSION}.json"),
        &serde_json::to_string_pretty(&manifest).expect("json"),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(
        session_row(&store, "message_coverage"),
        Value::String("partial".into())
    );
    let relations = session_row(&store, "unknown_relations");
    let relations = relations.as_str().expect("relations");
    assert!(relations.contains("cline.detail_mismatch"), "{relations}");
    // The cumulative value stays authoritative.
    assert_eq!(totals(&store).total_tokens, 449);
}

#[test]
fn a_session_without_stored_turns_keeps_the_cumulative_total() {
    let (dir, store, collector) = setup();
    std::fs::remove_file(dir.0.join(SESSION).join(format!("{SESSION}.messages.json")))
        .expect("remove detail");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.message_count().expect("count"), 0);
    assert_eq!(totals(&store).total_tokens, 385);
    assert_eq!(
        session_row(&store, "message_coverage"),
        Value::String("none".into())
    );
}

#[test]
fn a_subagent_session_is_counted_on_its_own_and_never_twice() {
    let (dir, store, collector) = setup();
    // The parent manifest also carries an aggregate total that includes the subagent.
    let mut manifest: Value = serde_json::from_str(MANIFEST).expect("manifest");
    manifest["metadata"]["aggregateUsage"] = json!({
        "inputTokens": 450,
        "outputTokens": 60,
        "cacheReadTokens": 190,
        "cacheWriteTokens": 5,
        "totalCost": 0
    });
    dir.write(
        &format!("{SESSION}/{SESSION}.json"),
        &serde_json::to_string_pretty(&manifest).expect("json"),
    );
    let subagent = "synthetic-session-1-sub";
    let mut sub_manifest: Value = serde_json::from_str(MANIFEST).expect("manifest");
    sub_manifest["session_id"] = json!(subagent);
    sub_manifest["metadata"]["usage"] = json!({
        "inputTokens": 100,
        "outputTokens": 25,
        "cacheReadTokens": 0,
        "cacheWriteTokens": 0,
        "totalCost": 0
    });
    dir.write(
        &format!("{subagent}/{subagent}.json"),
        &serde_json::to_string_pretty(&sub_manifest).expect("json"),
    );
    let mut sub_messages: Value = serde_json::from_str(MESSAGES).expect("messages");
    sub_messages["sessionId"] = json!(subagent);
    sub_messages["agent"] = json!("subagent");
    sub_messages["origin"]["parentThreadId"] = json!(SESSION);
    sub_messages["messages"] = json!([{
        "id": "s1",
        "role": "assistant",
        "ts": 1767322804000i64,
        "modelInfo": {"id": "synthetic/model-a", "provider": "synthetic"},
        "metrics": {
            "inputTokens": 100,
            "outputTokens": 25,
            "cacheReadTokens": 0,
            "cacheWriteTokens": 0,
            "cost": 0
        }
    }]);
    dir.write(
        &format!("{subagent}/{subagent}.messages.json"),
        &serde_json::to_string_pretty(&sub_messages).expect("json"),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.message_count().expect("count"), 4);
    // The parent keeps its own cumulative value; the aggregate total is never added.
    assert_eq!(totals(&store).total_tokens, 385 + 125);
    let conn = store.connection_for_tests();
    let parent: String = conn
        .query_row(
            "SELECT parent_client_session_id FROM sessions WHERE client_session_id = ?1",
            [subagent],
            |row| row.get(0),
        )
        .expect("parent link");
    assert_eq!(parent, SESSION);
}

#[test]
fn an_unreadable_detail_keeps_the_previous_import() {
    let (dir, store, collector) = setup();
    collector
        .sync(&store, &SyncOptions::default())
        .expect("first sync");
    dir.write(
        &format!("{SESSION}/{SESSION}.messages.json"),
        "not-json PRIVATE_SENTINEL",
    );
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("second sync");
    assert_eq!(report.files_failed, 1);
    // The failed file keeps its watermark and the known data stays visible.
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(totals(&store).total_tokens, 385);
}

#[test]
fn an_unknown_message_format_version_is_not_parsed() {
    let (dir, store, collector) = setup();
    let mut messages: Value = serde_json::from_str(MESSAGES).expect("messages");
    messages["version"] = json!(2);
    dir.write(
        &format!("{SESSION}/{SESSION}.messages.json"),
        &serde_json::to_string_pretty(&messages).expect("json"),
    );
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_failed, 1);
    assert_eq!(report.sessions_imported, 0);
    assert_eq!(store.message_count().expect("count"), 0);
}

#[test]
fn a_missing_root_is_reported_as_no_data() {
    let dir = TempDir::new();
    let store = UsageStore::open_in_memory().expect("store");
    let collector = ClineCollector::new(dir.0.join("missing"));
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert!(report.root_missing);
    assert_eq!(store.message_count().expect("count"), 0);
}

#[test]
fn the_session_root_follows_the_client_directory_precedence() {
    let home = PathBuf::from("/synthetic/home");
    assert_eq!(
        discovery::resolve_root(None, None, None, Some(home.clone())),
        Some(home.join(".cline").join("data").join("sessions"))
    );
    assert_eq!(
        discovery::resolve_root(
            None,
            Some(OsString::from("/synthetic/data")),
            None,
            Some(home.clone())
        ),
        Some(PathBuf::from("/synthetic/data").join("sessions"))
    );
    assert_eq!(
        discovery::resolve_root(
            None,
            None,
            Some(OsString::from("/synthetic/cline")),
            Some(home.clone()),
        ),
        Some(
            PathBuf::from("/synthetic/cline")
                .join("data")
                .join("sessions")
        )
    );
    assert_eq!(
        discovery::resolve_root(
            Some(OsString::from("/synthetic/sessions")),
            Some(OsString::from("/synthetic/data")),
            None,
            None
        ),
        Some(PathBuf::from("/synthetic/sessions"))
    );
    assert_eq!(
        discovery::resolve_root(None, Some(OsString::from("  ")), None, Some(home)),
        Some(PathBuf::from("/synthetic/home/.cline/data/sessions"))
    );
}

#[test]
fn a_source_change_during_the_scan_is_reported_as_changed() {
    let dir = TempDir::new();
    dir.write(&format!("{SESSION}/{SESSION}.json"), MANIFEST);
    dir.write(&format!("{SESSION}/{SESSION}.messages.json"), MESSAGES);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = ClineCollector::new(&dir.0);
    let error = collector
        .sync_with_mutation(&store, || {
            std::fs::write(
                dir.0.join(SESSION).join(format!("{SESSION}.messages.json")),
                format!("{MESSAGES}\n"),
            )
            .expect("mutate source");
        })
        .expect_err("changed during scan");
    assert!(matches!(error, SyncError::ChangedDuringScan));
}
