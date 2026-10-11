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
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("isolated failure")
            .files_failed,
        1
    );
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
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("isolated failure")
            .files_failed,
        1
    );
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

fn versioned(version: u64, id: &str) -> String {
    let text = log()
        .replace("dsh-fixture", id)
        .replace("\"version\":0", &format!("\"version\":{version}"));
    if version < 2 {
        text
    } else {
        text.replace("\"seedLength\":2", "\"isSeeded\":true")
            + "{\"type\":\"session/end-seed\",\"seq\":2,\"time\":4,\"data\":{\"inherited\":true}}\n"
    }
}

#[test]
fn twenty_nine_nested_sessions_cover_all_generations_without_duplicate_migrations() {
    let dir = TempDir::new();
    for index in 0..29 {
        let version = index % 5;
        let id = format!("fixture-{index}");
        let directory = format!("project/session-{index}");
        // Every session can retain its original v0 generation after migration.
        let old = versioned(0, &id);
        let old = if version > 0 {
            old.replace("\"inputTokens\":10", "\"inputTokens\":888")
        } else {
            old
        };
        dir.write(&format!("{directory}/session.jsonl"), &old);
        if version > 0 {
            let bytes =
                zstd::stream::encode_all(versioned(version, &id).as_bytes(), 1).expect("compress");
            let path = dir
                .0
                .join(format!("{directory}/session.v{version}.jsonl.zstd"));
            std::fs::write(path, bytes).expect("versioned fixture");
        }
        dir.write(
            &format!("{directory}/session.v4.jsonl.zstd.tmp"),
            "ignored temporary artifact",
        );
    }
    let store = UsageStore::open_in_memory().expect("store");
    let collector = DshCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_failed, 0);
    assert_eq!(report.files_parsed, 29);
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(totals.sessions, 29);
    assert_eq!(totals.messages, 29);
    assert_eq!(totals.total_tokens, 29 * 115);
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("unchanged")
            .files_skipped_unchanged,
        29
    );
    collector
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("force");
    assert_eq!(
        store.totals(&TotalsFilter::default()).expect("idempotent"),
        totals
    );
    assert_private(&store);
}

#[test]
fn v4_stream_usage_final_override_and_retried_attempts_are_separate() {
    let dir = TempDir::new();
    let text = concat!(
        "{\"type\":\"session\",\"version\":4,\"id\":\"stream-fixture\",\"createdAt\":1,\"isSeeded\":false}\n",
        "{\"type\":\"assistant/attempt\",\"seq\":0,\"time\":2,\"data\":{\"turn\":1,\"step\":1,\"stream\":[{\"type\":\"text-chunks\",\"text\":\"PRIVATE_SENTINEL\"},{\"type\":\"chunk\",\"chunk\":{\"type\":\"usage\",\"usage\":{\"inputTokens\":100,\"outputTokens\":200}}},{\"type\":\"chunk\",\"chunk\":{\"type\":\"usage\",\"usage\":{\"inputTokens\":1,\"outputTokens\":2}}}]}}\n",
        "{\"type\":\"llm/retry-started\",\"seq\":1,\"time\":3,\"data\":{\"turn\":1,\"step\":1}}\n",
        "{\"type\":\"llm/retry-started\",\"seq\":1,\"time\":3,\"data\":{\"turn\":1,\"step\":1}}\n",
        "{\"type\":\"assistant/message\",\"seq\":2,\"time\":4,\"data\":{\"turn\":1,\"step\":1,\"usage\":{\"inputTokens\":10,\"outputTokens\":20,\"cacheWriteTokens\":4},\"stream\":[{\"type\":\"chunk\",\"chunk\":{\"type\":\"usage\",\"usage\":{\"inputTokens\":100,\"outputTokens\":200}}}]}}\n",
        "{\"type\":\"assistant/message\",\"seq\":3,\"time\":5,\"data\":{\"turn\":2,\"step\":1,\"stream\":[{\"type\":\"chunk\",\"chunk\":{\"type\":\"usage\",\"usage\":{\"inputTokens\":5,\"outputTokens\":6}}}]}}\n"
    );
    let path = dir.write("session.v4.jsonl", text);
    let parsed = super::parser::parse_file(&path).expect("parse");
    assert_eq!(parsed.unparsed, 0);
    assert_eq!(parsed.import.messages.len(), 3);
    assert_eq!(parsed.import.messages[1].dedup_key, "step:1:1:attempt:1");
    let store = UsageStore::open_in_memory().expect("store");
    DshCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("total")
            .total_tokens,
        48
    );
    assert_private(&store);
}

#[test]
fn seeded_modern_header_requires_a_matching_inherited_boundary() {
    let dir = TempDir::new();
    for version in 2..=4 {
        let path = dir.write(
            &format!("session.v{version}.jsonl"),
            &versioned(version, "seed"),
        );
        assert_eq!(
            super::parser::parse_file(&path)
                .expect("valid seed")
                .import
                .messages
                .len(),
            1
        );
        let incomplete = versioned(version, "seed")
            .lines()
            .filter(|line| !line.contains("session/end-seed"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&path, incomplete).expect("missing boundary");
        assert!(super::parser::parse_file(&path).is_err());
        std::fs::write(
            &path,
            versioned(version, "seed").replace("\"isSeeded\":true", "\"isSeeded\":false"),
        )
        .expect("invalid marker");
        assert!(super::parser::parse_file(&path).is_err());
    }
}

#[test]
fn partial_scan_imports_healthy_neighbor_preserves_failed_watermark_and_retries() {
    let dir = TempDir::new();
    let first = dir.write("project/broken/session.jsonl", &versioned(0, "broken"));
    let store = UsageStore::open(&dir.0.join("usage.db")).expect("store");
    let collector = DshCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let path = first.to_string_lossy().to_string();
    let before = store
        .source_file(crate::usage::client::Client::Dsh, &path)
        .expect("watermark");
    // Unsupported latest generation must not fall back to an older valid log.
    dir.write("project/broken/session.v99.jsonl", &versioned(99, "broken"));
    let healthy = dir.write("project/healthy/session.v4.jsonl", &versioned(4, "healthy"));
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("partial");
    assert_eq!(report.files_failed, 1);
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(store.session_count().expect("preserved plus healthy"), 2);
    assert_eq!(
        store
            .source_file(crate::usage::client::Client::Dsh, &path)
            .expect("preserved watermark"),
        before
    );
    // Corrected totals and an earlier end time must replace, rather than stale-upsert skip.
    std::fs::write(
        &healthy,
        versioned(4, "healthy")
            .replace("\"inputTokens\":10", "\"inputTokens\":2")
            .replace("\"time\":5", "\"time\":4"),
    )
    .expect("correction");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("partial correction");
    assert_eq!(
        store
            .totals(&TotalsFilter::default())
            .expect("corrected")
            .total_tokens,
        222
    );
    drop(store);
    let store = UsageStore::open(&dir.0.join("usage.db")).expect("reopen");
    let repeat = collector
        .sync(&store, &SyncOptions::default())
        .expect("retry unsupported");
    assert_eq!(repeat.files_failed, 1);
    assert_eq!(store.message_count().expect("idempotent"), 2);
    std::fs::remove_dir_all(dir.0.join("project/broken")).expect("remove fixture");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("complete deletion cleanup");
    assert_eq!(store.session_count().expect("pruned failed history"), 1);
    assert_eq!(
        store
            .source_paths(crate::usage::client::Client::Dsh)
            .expect("paths")
            .len(),
        1
    );
}

#[test]
fn empty_retry_does_not_inherit_previous_attempt_usage_and_unknown_events_are_counted() {
    let dir = TempDir::new();
    let mut text = versioned(4, "retry-fixture");
    for (seq, kind, data) in [
        (
            4,
            "llm/retry-started",
            serde_json::json!({"turn":1,"step":1}),
        ),
        (
            5,
            "assistant/attempt",
            serde_json::json!({"turn":1,"step":1,"stream":[]}),
        ),
        (
            6,
            "future/event",
            serde_json::json!({"content":"PRIVATE_SENTINEL"}),
        ),
        (
            7,
            "system/message",
            serde_json::json!({"content":"PRIVATE_SENTINEL"}),
        ),
        (
            8,
            "compaction/prune",
            serde_json::json!({"shadowedTokenCount":999999}),
        ),
    ] {
        text += &serde_json::json!({"seq":seq,"time":8,"type":kind,"data":data}).to_string();
        text.push('\n');
    }
    let path = dir.write("session.v4.jsonl", &text);
    let parsed = super::parser::parse_file(&path).expect("parse");
    assert_eq!(parsed.unparsed, 1);
    assert_eq!(parsed.import.messages.len(), 2);
    assert_eq!(parsed.import.messages[0].usage.total_tokens, Some(115));
    assert_eq!(parsed.import.messages[1].usage.total_tokens, None);
    assert_eq!(parsed.import.session.usage.total_tokens, None);
}
