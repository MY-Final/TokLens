use super::*;
use crate::collectors::SyncOptions;
use crate::collectors::test_support::{TempDir, assert_private};
use crate::usage::store::{TotalsFilter, UsageStore};

fn log() -> String {
    [
        r#"{"type":"session","version":0,"id":"dsh-fixture","createdAt":1,"cwd":"/fixture","seedLength":2,"parentSession":"parent","origin":"subagent"}"#,
        r#"{"type":"request/header","seq":0,"time":2,"data":{"header":{"config":{"provider":"deepseek","model":"test"},"system":"PRIVATE_SENTINEL"}}}"#,
        r#"{"type":"assistant/message","seq":1,"time":3,"data":{"turn":1,"step":1,"usage":{"inputTokens":999,"outputTokens":999,"cacheReadTokens":999},"message":{"content":"PRIVATE_SENTINEL"}}}"#,
        r#"{"type":"assistant/chunk","seq":2,"time":4,"data":{"turn":1,"step":1,"chunk":{"type":"usage","usage":{"inputTokens":1,"outputTokens":1}}}}"#,
        r#"{"type":"assistant/message","seq":3,"time":5,"data":{"turn":1,"step":1,"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":100,"reasoningTokens":2},"message":{"content":"PRIVATE_SENTINEL"}}}"#,
        r#"{"type":"assistant/message","seq":3,"time":5,"data":{"turn":1,"step":1,"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":100,"reasoningTokens":2}}}"#,
    ].join("\n") + "\n"
}

#[test]
fn concatenated_frames_skip_inherited_seed_and_chunk_usage() {
    let dir = TempDir::new();
    let text = log();
    let split = text.find('\n').expect("header end") + 1;
    let mut bytes = zstd::stream::encode_all(&text.as_bytes()[..split], 1).expect("header frame");
    bytes.extend(zstd::stream::encode_all(&text.as_bytes()[split..], 1).expect("event frame"));
    let path = dir.0.join("session.jsonl.zstd");
    std::fs::write(&path, &bytes).expect("compressed fixture");
    dir.write("session.v4.jsonl", &log());
    let store = UsageStore::open_in_memory().expect("store");
    let collector = DshCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_seen, 1);
    assert_eq!(store.message_count().expect("count"), 1);
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(totals.total_tokens, 115);
    assert_eq!(totals.output_tokens, 3);
    assert_eq!(totals.reasoning_tokens, 2);
    assert_eq!(totals.input_tokens, 10);
    assert_private(&store);
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("unchanged")
            .files_skipped_unchanged,
        1
    );
    bytes.pop();
    std::fs::write(path, bytes).expect("torn frame");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("preserved")
            .total_tokens,
        115
    );
}

#[test]
fn unsupported_version_retains_data_and_plain_artifacts_work() {
    let dir = TempDir::new();
    let path = dir.write("project/session.jsonl", &log());
    let store = UsageStore::open_in_memory().expect("store");
    let collector = DshCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    std::fs::write(&path, log().replace("\"version\":0", "\"version\":99"))
        .expect("unsupported fixture");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert_eq!(store.message_count().expect("preserved"), 1);
}

#[test]
fn independent_dispatch_continues_after_one_client_fails() {
    let dir = TempDir::new();
    dir.write("session.jsonl", &log());
    let corrupt = dir.write("bad.db", "not SQLite");
    let store = UsageStore::open_in_memory().expect("store");
    let roots = crate::collectors::CollectorRoots {
        dsh: Some(dir.0.clone()),
        opencode: Some(corrupt),
        ..Default::default()
    };
    let result = crate::collectors::sync_clients(
        &store,
        &roots,
        &SyncOptions::default(),
        &crate::usage::client::Client::ALL,
    );
    assert!(result.has_errors());
    assert!(result.clients[2].error.is_some());
    assert_eq!(
        result.clients[3]
            .report
            .as_ref()
            .expect("DSH success")
            .sessions_imported,
        1
    );
    assert_eq!(store.message_count().expect("DSH imported"), 1);
}

#[test]
fn chunk_only_usage_and_final_message_replace_the_same_request() {
    let dir = TempDir::new();
    let text = log()
        + concat!(
            "{\"type\":\"assistant/chunk\",\"seq\":4,\"time\":6,\"data\":{\"turn\":2,\"step\":1,\"chunk\":{\"type\":\"usage\",\"usage\":{\"inputTokens\":1,\"outputTokens\":2}}}}\n",
            "{\"type\":\"assistant/message\",\"seq\":5,\"time\":7,\"data\":{\"turn\":2,\"step\":1}}\n",
            "{\"type\":\"assistant/chunk\",\"seq\":6,\"time\":8,\"data\":{\"turn\":2,\"step\":2,\"chunk\":{\"type\":\"usage\",\"usage\":{\"inputTokens\":10,\"outputTokens\":20,\"cacheWriteTokens\":4}}}}\n"
        );
    dir.write("session.jsonl", &text);
    let store = UsageStore::open_in_memory().expect("store");
    DshCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.message_count().expect("requests"), 3);
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("total")
            .total_tokens,
        152
    );
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("writes")
            .cache_write_tokens,
        4
    );
}

#[test]
fn null_cache_is_rejected_and_missing_required_usage_stays_unknown() {
    let dir = TempDir::new();
    let text = log()
        + concat!(
            "{\"type\":\"assistant/message\",\"seq\":4,\"time\":6,\"data\":{\"turn\":2,\"step\":1,\"usage\":{\"inputTokens\":1,\"outputTokens\":2,\"cacheReadTokens\":null}}}\n",
            "{\"type\":\"assistant/message\",\"seq\":5,\"time\":7,\"data\":{\"turn\":2,\"step\":2,\"usage\":{\"inputTokens\":1}}}\n"
        );
    dir.write("session.jsonl", &text);
    let store = UsageStore::open_in_memory().expect("store");
    let report = DshCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 1);
    assert_eq!(store.message_count().expect("requests"), 2);
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    let unknown: i64 = store
        .connection_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE total_tokens IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("unknown sessions");
    assert_eq!(unknown, 1);
    assert_eq!(totals.total_tokens, 0);
}
