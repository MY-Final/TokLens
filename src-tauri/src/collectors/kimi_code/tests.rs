use super::*;
use crate::collectors::{
    SyncOptions,
    test_support::{TempDir, assert_private},
};
use crate::usage::{
    client::Client,
    store::{TotalsFilter, UsageStore},
};

const STATE: &str = r#"{"id":"session_fixture","version":2,"cwd":"/fixture","createdAt":1000,"updatedAt":2000,"title":"PRIVATE_SENTINEL","lastPrompt":"PRIVATE_SENTINEL"}"#;
const USAGE: &str = r#"{"type":"usage.record","model":"synthetic","usage":{"inputOther":10,"output":5,"inputCacheRead":100,"inputCacheCreation":7},"usageScope":"turn","time":2000}"#;
fn wire(records: &[&str]) -> String {
    String::from("{\"type\":\"metadata\",\"protocol_version\":\"1.5\"}\n")
        + &records.join("\n")
        + "\n"
}
fn fixture(dir: &TempDir, records: &[&str]) {
    dir.write("workspace/session_fixture/state.json", STATE);
    dir.write(
        "workspace/session_fixture/agents/main/wire.jsonl",
        &wire(records),
    );
}
fn total(store: &UsageStore) -> i64 {
    store
        .totals(&TotalsFilter::default())
        .expect("totals")
        .total_tokens
}

#[test]
fn identical_requests_agents_privacy_and_provenance() {
    let dir = TempDir::new();
    fixture(
        &dir,
        &[
            USAGE,
            USAGE,
            r#"{"type":"token_counting.measured","tokens":9999}"#,
            r#"{"type":"turn.prompt","text":"PRIVATE_SENTINEL"}"#,
        ],
    );
    dir.write(
        "workspace/session_fixture/agents/worker/wire.jsonl",
        &wire(&[USAGE]),
    );
    dir.write(
        "workspace/session_fixture/plugins/example/state.json",
        "PRIVATE_SENTINEL",
    );
    dir.write("workspace/session_fixture/server/events.jsonl", USAGE);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = KimiCodeCollector::new(&dir.0);
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_seen, 3);
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(store.message_count().expect("count"), 3);
    assert_eq!(total(&store), 366);
    let conn = store.connection_for_tests();
    let workers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE is_sidechain = 1",
            [],
            |row| row.get(0),
        )
        .expect("sidechains");
    assert_eq!(workers, 1);
    let wire_sources: i64 = conn.query_row("SELECT COUNT(*) FROM messages m JOIN source_files f ON m.source_file_id = f.id WHERE f.path LIKE '%wire.jsonl'", [], |row| row.get(0)).expect("provenance");
    assert_eq!(wire_sources, 3);
    let counted = store
        .range_totals(&crate::usage::analytics::AnalyticsFilter::default())
        .expect("analytics");
    assert_eq!(counted.sums.counted_cache_write_tokens, 21);
    assert_private(&store);
    assert_eq!(
        collector
            .sync(&store, &SyncOptions::default())
            .expect("unchanged")
            .files_skipped_unchanged,
        3
    );
    collector
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("forced");
    assert_eq!(total(&store), 366);
}

#[test]
fn snapshots_handle_append_shrink_delete_and_restart() {
    let dir = TempDir::new();
    fixture(&dir, &[USAGE]);
    let db = dir.0.join("usage.db");
    let collector = KimiCodeCollector::new(&dir.0);
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
            .expect("watermarks")
            .files_skipped_unchanged,
        2
    );
    dir.write(
        "workspace/session_fixture/agents/main/wire.jsonl",
        &wire(&[USAGE, USAGE]),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("append");
    assert_eq!(total(&store), 244);
    dir.write(
        "workspace/session_fixture/agents/main/wire.jsonl",
        &wire(&[USAGE]),
    );
    collector
        .sync(&store, &SyncOptions::default())
        .expect("shrink");
    assert_eq!(total(&store), 122);
    std::fs::remove_dir_all(dir.0.join("workspace")).expect("remove synthetic session");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("delete");
    assert_eq!(store.message_count().expect("count"), 0);
    assert_eq!(
        store.source_paths(Client::KimiCode).expect("sources").len(),
        0
    );
}

#[test]
fn legacy_protocol_empty_missing_and_zero_usage() {
    let dir = TempDir::new();
    fixture(&dir, &[]);
    dir.write("workspace/session_fixture/state.json", r#"{"workDir":"/fixture","createdAt":"2026-07-21T14:08:44.991Z","updatedAt":"2026-07-21T14:08:44.991Z"}"#);
    let path = dir.write(
        "workspace/session_fixture/agents/main/wire.jsonl",
        &wire(&[]).replace("1.5", "1.4"),
    );
    let meta = metadata::read(
        &dir.0.join("workspace/session_fixture"),
        Some(&dir.0.join("workspace/session_fixture/state.json")),
        &workdirs::WorkDirs::new(),
    )
    .expect("legacy");
    assert_eq!(meta.id, "session_fixture");
    assert_eq!(meta.start_ms, Some(1784642924991));
    let store = UsageStore::open_in_memory().expect("store");
    let collector = KimiCodeCollector::new(&dir.0);
    for record in [
        None,
        Some(r#"{"type":"usage.record","usage":{"inputOther":0,"output":0,"inputCacheRead":0}}"#),
        Some(r#"{"type":"usage.record"}"#),
    ] {
        std::fs::write(&path, wire(&record.into_iter().collect::<Vec<_>>())).expect("fixture");
        collector
            .sync(&store, &SyncOptions { force_rescan: true })
            .expect("sync");
        assert_eq!(
            store
                .range_totals(&crate::usage::analytics::AnalyticsFilter::default())
                .expect("totals")
                .sessions_without_usage,
            1
        );
    }
    std::fs::write(path, wire(&[r#"{"type":"usage.record","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0}}"#])).expect("zero");
    collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(
        store
            .range_totals(&crate::usage::analytics::AnalyticsFilter::default())
            .expect("totals")
            .sessions_without_usage,
        0
    );
}

#[test]
fn failed_scan_preserves_snapshot_and_watermarks() {
    let dir = TempDir::new();
    fixture(&dir, &[USAGE]);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = KimiCodeCollector::new(&dir.0);
    collector
        .sync(&store, &SyncOptions::default())
        .expect("initial");
    let path = dir
        .0
        .join("workspace/session_fixture/agents/main/wire.jsonl");
    let before = store
        .source_file(Client::KimiCode, &path.to_string_lossy())
        .expect("source");
    for broken in [
        wire(&[USAGE]) + "{",
        wire(&[USAGE]).replace("1.5", "9.9"),
        String::new(),
    ] {
        std::fs::write(&path, broken).expect("broken fixture");
        assert!(collector.sync(&store, &SyncOptions::default()).is_err());
        assert_eq!(total(&store), 122);
        assert_eq!(
            store
                .source_file(Client::KimiCode, &path.to_string_lossy())
                .expect("preserved"),
            before
        );
    }
    std::fs::write(&path, wire(&[USAGE])).expect("restore");
    dir.write(
        "workspace/session_fixture/state.json",
        &STATE.replace("\"version\":2", "\"version\":99"),
    );
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    std::fs::remove_file(dir.0.join("workspace/session_fixture/state.json"))
        .expect("remove synthetic state");
    assert!(collector.sync(&store, &SyncOptions::default()).is_err());
    assert!(
        KimiCodeCollector::new(dir.0.join("missing"))
            .sync(&store, &SyncOptions::default())
            .expect("missing root")
            .root_missing
    );
    assert_eq!(total(&store), 122);
}

#[test]
fn invalid_usage_is_counted_and_partial_coverage_is_visible() {
    let dir = TempDir::new();
    fixture(
        &dir,
        &[
            USAGE,
            r#"{"type":"usage.record","usage":{"inputOther":-1}}"#,
            r#"{"type":"usage.record","usage":{"inputOther":9223372036854775808}}"#,
            "bad row",
            r#"{"type":"future.event"}"#,
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = KimiCodeCollector::new(&dir.0)
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.unparsed_lines, 4);
    assert_eq!(store.message_count().expect("count"), 1);
    assert_eq!(
        store
            .range_totals(&crate::usage::analytics::AnalyticsFilter::default())
            .expect("totals")
            .sessions_partial,
        1
    );
}

/// 1.1 协议：每行是 `{"timestamp": 秒, "message": {...}}`，用量在消息负载上。
fn wire_v11(records: &[&str]) -> String {
    String::from("{\"type\":\"metadata\",\"protocol_version\":\"1.1\"}\n")
        + &records.join("\n")
        + "\n"
}

const V11_USAGE: &str = r#"{"timestamp":1769652498.3081927,"message":{"type":"StatusUpdate","payload":{"message_id":"chatcmpl-a","token_usage":{"input_other":10,"output":5,"input_cache_read":100,"input_cache_creation":7}}}}"#;
const V11_TOOL: &str =
    r#"{"timestamp":1769652499.0,"message":{"type":"ToolCall","payload":{"type":"function"}}}"#;

/// 当前版本的布局：`<工作目录摘要>/<会话 id>/wire.jsonl`，没有 state.json。
fn current_fixture(dir: &TempDir, records: &[&str]) -> (String, String) {
    let project = "/synthetic/workspace";
    let digest = workdirs::digest(project);
    dir.write(
        "kimi.json",
        &format!(
            r#"{{"work_dirs":[{{"path":"{project}","kaos":"local","last_session_id":null}}]}}"#
        ),
    );
    dir.write(
        &format!("sessions/{digest}/session_v1/wire.jsonl"),
        &wire_v11(records),
    );
    (digest, project.to_string())
}

#[test]
fn current_protocol_reads_usage_and_maps_the_workspace_digest() {
    let dir = TempDir::new();
    let (digest, project) = current_fixture(&dir, &[V11_USAGE, V11_TOOL]);
    let store = UsageStore::open_in_memory().expect("store");
    let collector = KimiCodeCollector::new(dir.0.join("sessions"));
    let report = collector
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(report.files_seen, 1);
    assert_eq!(report.unparsed_lines, 0);
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(store.message_count().expect("count"), 1);
    // 四个互不重叠的分桶：10 + 5 + 100 + 7。
    assert_eq!(total(&store), 122);
    let conn = store.connection_for_tests();
    let (session, project_path, model): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT client_session_id, project_path, model FROM sessions WHERE client='kimi_code'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("session");
    assert_eq!(session, "session_v1");
    assert_eq!(project_path, Some(project.clone()));
    // 1.1 不落盘模型，缺失保持未知。
    assert_eq!(model, None);
    let (key, start): (String, Option<i64>) = conn
        .query_row(
            "SELECT dedup_key, created_at_ms FROM messages WHERE client='kimi_code'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("message");
    assert_eq!(key, "message:chatcmpl-a");
    assert_eq!(start, Some(1769652498308));
    // 工作目录摘要是路径的 MD5，摘要对不上时项目路径保持未知。
    assert_eq!(workdirs::digest(&project), digest);
    assert_private(&store);
}

#[test]
fn current_protocol_deduplicates_repeated_message_ids() {
    let dir = TempDir::new();
    current_fixture(&dir, &[V11_USAGE, V11_USAGE]);
    let store = UsageStore::open_in_memory().expect("store");
    KimiCodeCollector::new(dir.0.join("sessions"))
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    assert_eq!(store.message_count().expect("count"), 1);
    assert_eq!(total(&store), 122);
}

#[test]
fn current_protocol_reports_unknown_records_and_ignores_the_conversation() {
    let dir = TempDir::new();
    current_fixture(
        &dir,
        &[
            V11_USAGE,
            r#"{"timestamp":1769652500.0,"message":{"type":"FutureEvent","payload":{"n":1}}}"#,
        ],
    );
    dir.write("sessions/unknown/context.jsonl", "PRIVATE_SENTINEL");
    let store = UsageStore::open_in_memory().expect("store");
    let report = KimiCodeCollector::new(dir.0.join("sessions"))
        .sync(&store, &SyncOptions::default())
        .expect("sync");
    // 未知消息类型计入未识别记录；对话文件根本不读。
    assert_eq!(report.unparsed_lines, 1);
    assert_eq!(store.message_count().expect("count"), 1);
    assert_private(&store);
}
