//! 合成数据回归：目录↔分发器一致性、逐类型 round-trip、边界拒绝、持久化与存活语义。

use super::catalog::{self, CATALOG, ParamSpec};
use super::query::{self, PanelDataDto, PanelQueryArgs};
use super::spec::{self, PanelSpec, PanelType, TimeBasis};
use super::store as panel_store;
use crate::settings::{AppSettings, SettingsPatch, SettingsStore};
use crate::usage::client::Client;
use crate::usage::model::{MessageRecord, SessionRecord, parse_iso8601_utc_ms};
use crate::usage::normalize::{
    ClaudeSessionRollup, CodexRecordIdentity, CodexThreadSnapshot, CodexUsageRecord,
    claude_code_session, codex_message, codex_session,
};
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::tokens::RawTokenCounts;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const DAY_MS: i64 = 86_400_000;
const D0: &str = "2026-10-05T00:00:00.000Z";
const D1: &str = "2026-10-06T09:00:00.000Z";
const D2: &str = "2026-10-07T20:00:00.000Z";
const PROJECT_ALPHA: &str = "D:\\Repos\\alpha";
const PROJECT_BETA: &str = "D:\\Repos\\beta";

fn ms(value: &str) -> i64 {
    parse_iso8601_utc_ms(value).expect("synthetic timestamp")
}

fn counts(
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    reasoning: Option<u64>,
    client_total: Option<u64>,
) -> RawTokenCounts {
    RawTokenCounts {
        input,
        output,
        cache_read,
        cache_write,
        reasoning,
        client_total,
    }
}

fn codex_record(
    thread: &str,
    started: &str,
    ended: &str,
    model: Option<&str>,
    project: Option<&str>,
    cumulative: RawTokenCounts,
) -> SessionRecord {
    codex_session(&CodexThreadSnapshot {
        thread_id: thread,
        parent_thread_id: None,
        cwd: project,
        model,
        started_at: Some(started),
        ended_at: Some(ended),
        first_ordinal: Some(0),
        message_count: 1,
        cumulative,
    })
}

fn claude_record(started: &str, project: &str, model: &str) -> SessionRecord {
    claude_code_session(&ClaudeSessionRollup {
        session_id: "session-d",
        cwd: Some(project),
        model: Some(model),
        started_at: Some(started),
        ended_at: Some(started),
        message_count: 1,
        usage: counts(Some(500), Some(50), Some(100), Some(0), None, None),
    })
}

fn message(thread: &str, response: &str, ordinal: i64, at: &str) -> MessageRecord {
    codex_message(&CodexUsageRecord {
        thread_id: thread,
        response_id: Some(response),
        identity: CodexRecordIdentity::Ordinal(ordinal),
        timestamp: Some(at),
        usage: counts(Some(10), Some(2), Some(0), Some(0), Some(0), Some(12)),
    })
}

fn source(client: Client, path: &str) -> SourceFile {
    SourceFile {
        client,
        path: path.to_string(),
        kind: SourceFileKind::Jsonl,
        size_bytes: Some(1_024),
        mtime_ms: Some(1_791_000_000_000),
        content_offset: 1_024,
        parser_version: 1,
        last_scan_ms: Some(1_791_000_000_500),
        unparsed_records: 0,
    }
}

/// 4 个会话：3 个 Codex（跨 3 天，其中一个完全没有用量数据）+ 1 个 Claude Code。
fn seeded_store() -> UsageStore {
    let store = UsageStore::open_in_memory().expect("store");
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\a.jsonl"),
            &codex_record(
                "thread-a",
                D0,
                D0,
                Some("gpt-5.6-luna"),
                Some(PROJECT_ALPHA),
                counts(
                    Some(1_000),
                    Some(100),
                    Some(400),
                    Some(5),
                    Some(20),
                    Some(1_100),
                ),
            ),
            &[
                message("thread-a", "resp-a1", 1, D0),
                message("thread-a", "resp-a2", 2, D0),
            ],
        )
        .expect("import a");
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\b.jsonl"),
            &codex_record(
                "thread-b",
                D1,
                D1,
                Some("gpt-6-astra"),
                Some(PROJECT_BETA),
                counts(
                    Some(2_000),
                    Some(300),
                    Some(0),
                    Some(0),
                    Some(0),
                    Some(2_300),
                ),
            ),
            &[message("thread-b", "resp-b1", 1, D1)],
        )
        .expect("import b");
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\c.jsonl"),
            &codex_record("thread-c", D2, D2, None, None, RawTokenCounts::default()),
            &[],
        )
        .expect("import c");
    store
        .import_session(
            &source(Client::ClaudeCode, "C:\\claude\\d.jsonl"),
            &claude_record(D1, PROJECT_ALPHA, "claude-sonnet-5"),
            &[],
        )
        .expect("import d");
    store
}

fn typed(value: Value) -> PanelSpec {
    serde_json::from_value(value).expect("synthetic spec")
}

/// 一个合法的默认规格（跟随页面范围、无客户端筛选）。
fn base(panel_type: &str) -> Value {
    json!({
        "specVersion": 1,
        "id": format!("p-{panel_type}"),
        "type": panel_type,
        "query": { "range": { "kind": "followPage" }, "client": null },
        "layout": { "colSpan": 12, "rowSpan": 1, "order": 0 },
        "source": { "origin": "manual" }
    })
}

fn parse_validate(value: Value, enabled: &[Client]) -> Result<(), String> {
    spec::parse_spec(value).and_then(|parsed| spec::validate_spec(parsed, enabled).map(|_| ()))
}

// ---------------------------------------------------------------------------------------------
// 目录 ↔ 分发器一致性
// ---------------------------------------------------------------------------------------------

#[test]
fn catalog_lists_every_panel_type_exactly_once() {
    assert_eq!(CATALOG.len(), PanelType::ALL.len());
    for panel_type in PanelType::ALL {
        let count = CATALOG
            .iter()
            .filter(|entry| entry.panel_type == panel_type)
            .count();
        assert_eq!(count, 1, "{} must appear exactly once", panel_type.as_str());
    }
}

#[test]
fn dispatcher_route_matches_catalog_for_every_basis() {
    for entry in CATALOG {
        for basis in [TimeBasis::Session, TimeBasis::Request] {
            let routed = query::route(entry.panel_type, basis);
            if entry.time_bases.contains(&basis) {
                let expected = entry
                    .routes
                    .iter()
                    .find(|(candidate, _)| *candidate == basis)
                    .map(|(_, query)| *query);
                assert_eq!(
                    routed,
                    expected,
                    "{} / {} route mismatch",
                    entry.panel_type.as_str(),
                    basis.as_str()
                );
            } else {
                assert_eq!(
                    routed,
                    None,
                    "{} must reject basis {}",
                    entry.panel_type.as_str(),
                    basis.as_str()
                );
            }
        }
    }
}

fn param<'a>(entry: &'a catalog::PanelTypeSpec, name: &str) -> &'a ParamSpec {
    entry
        .params
        .iter()
        .find(|param| param.name == name)
        .unwrap_or_else(|| panic!("missing param {name}"))
}

#[test]
fn catalog_limits_equal_dispatch_limits() {
    let ranking = catalog::entry(PanelType::Ranking).expect("ranking");
    assert_eq!(
        param(ranking, "limit").min,
        Some(spec::RANKING_LIMIT_MIN as i64)
    );
    assert_eq!(
        param(ranking, "limit").max,
        Some(spec::RANKING_LIMIT_MAX as i64)
    );

    let sessions = catalog::entry(PanelType::SessionTable).expect("sessions");
    assert_eq!(
        param(sessions, "limit").min,
        Some(spec::SESSION_LIMIT_MIN as i64)
    );
    assert_eq!(
        param(sessions, "limit").max,
        Some(spec::SESSION_LIMIT_MAX as i64)
    );
    assert_eq!(
        param(sessions, "offset").max,
        Some(spec::SESSION_OFFSET_MAX as i64)
    );

    let trend = catalog::entry(PanelType::TokenTrend).expect("trend");
    assert_eq!(
        param(trend, "dayCount").min,
        Some(spec::DAY_COUNT_MIN as i64)
    );
    assert_eq!(
        param(trend, "dayCount").max,
        Some(spec::DAY_COUNT_MAX as i64)
    );

    let calendar = catalog::entry(PanelType::ActivityCalendar).expect("calendar");
    assert_eq!(
        param(calendar, "dayCount").max,
        Some(spec::CALENDAR_DAY_COUNT_MAX as i64)
    );

    let activity = catalog::entry(PanelType::RequestTrend).expect("activity");
    assert_eq!(
        param(activity, "dayBoundaries").min,
        Some(spec::DAY_BOUNDARY_MIN as i64)
    );
    assert_eq!(
        param(activity, "dayBoundaries").max,
        Some(spec::DAY_BOUNDARY_MAX as i64)
    );
    assert_eq!(
        param(activity, "hourlyBoundaries").max,
        Some(spec::HOURLY_BOUNDARY_MAX as i64)
    );
}

#[test]
fn catalog_client_values_match_client_enum() {
    let expected: Vec<&str> = Client::ALL.iter().map(|client| client.as_str()).collect();
    assert_eq!(catalog::CLIENT_VALUES, expected.as_slice());
}

#[test]
fn stack_values_match_the_frontend_contract() {
    // 目录只暴露 client / tokens（前端 TokenTrendChart 固定取值）。
    let trend = catalog::entry(PanelType::TokenTrend).expect("trend");
    assert_eq!(
        param(trend, "stack").values.to_vec(),
        vec!["tokens", "client"]
    );

    let enabled = Client::ALL.to_vec();
    for value in ["client", "tokens"] {
        let mut spec = base("token_trend");
        spec["typeOptions"] = json!({ "stack": value });
        let validated = spec::validate_spec(spec::parse_spec(spec).expect("parse"), &enabled)
            .expect("validate");
        assert_eq!(
            serde_json::to_value(&validated.spec).expect("serialize")["typeOptions"]["stack"],
            json!(value)
        );
    }

    // 旧值 "total" 不再是合法取值。
    let mut spec = base("token_trend");
    spec["typeOptions"] = json!({ "stack": "total" });
    assert!(spec::parse_spec(spec).is_err());
}

// ---------------------------------------------------------------------------------------------
// 逐类型 round-trip 与边界
// ---------------------------------------------------------------------------------------------

#[test]
fn every_type_default_spec_round_trips_and_routes() {
    let enabled = Client::ALL.to_vec();
    for panel_type in PanelType::ALL {
        let parsed = spec::parse_spec(base(panel_type.as_str())).expect("parse");
        assert_eq!(parsed.panel_type, panel_type);
        let validated = spec::validate_spec(parsed.clone(), &enabled).expect("validate");
        assert!(query::route(validated.spec.panel_type, validated.time_basis).is_some());
        let serialized = serde_json::to_value(&validated.spec).expect("serialize");
        let reparsed: PanelSpec = serde_json::from_value(serialized).expect("reparse");
        assert_eq!(reparsed, validated.spec, "round trip must be lossless");
    }
}

#[test]
fn unknown_fields_types_clients_and_enums_are_rejected() {
    let enabled = Client::ALL.to_vec();
    let check = |value: Value| {
        assert!(
            parse_validate(value, &enabled).is_err(),
            "expected rejection"
        );
    };

    // 规格/参数不得携带 SQL、路径、公式或提示词等字段。
    for key in ["sql", "path", "file", "formula", "prompt", "bogus"] {
        let mut value = base("stat");
        value[key] = json!("select * from sessions");
        check(value);
    }

    let mut value = base("stat");
    value["typeOptions"] = json!({ "injected": "x" });
    check(value);

    let mut value = base("stat");
    value["query"]["range"] = json!({ "kind": "followPage", "sql": "x" });
    check(value);

    let mut value = base("stat");
    value["type"] = json!("sankey");
    check(value);

    let mut value = base("stat");
    value["query"]["client"] = json!("nope");
    check(value);

    let mut value = base("stat");
    value["query"]["range"] = json!({ "kind": "whenever" });
    check(value);

    // 类型专属参数用于错误类型。
    let mut value = base("stat");
    value["typeOptions"] = json!({ "metric": "tokens" });
    check(value);
}

#[test]
fn out_of_bounds_values_are_rejected_not_rewritten() {
    let enabled = Client::ALL.to_vec();
    let check = |value: Value| {
        assert!(
            parse_validate(value, &enabled).is_err(),
            "expected rejection"
        );
    };

    for layout in [
        json!({ "colSpan": 5, "rowSpan": 1, "order": 0 }),
        json!({ "colSpan": 12, "rowSpan": 3, "order": 0 }),
        json!({ "colSpan": 12, "rowSpan": 1, "order": 64 }),
    ] {
        let mut value = base("stat");
        value["layout"] = layout;
        check(value);
    }

    let mut value = base("stat");
    value["title"] = json!("x".repeat(spec::TITLE_MAX_LEN + 1));
    check(value);

    let mut value = base("stat");
    value["query"]["range"] = json!({ "kind": "custom", "fromMs": 10, "toMs": 10 });
    check(value);

    let mut value = base("ranking");
    value["typeOptions"] = json!({ "limit": spec::RANKING_LIMIT_MAX + 1 });
    check(value);

    let mut value = base("session_table");
    value["typeOptions"] = json!({ "limit": spec::SESSION_LIMIT_MAX + 1 });
    check(value);

    let mut value = base("session_table");
    value["typeOptions"] = json!({ "search": "x".repeat(spec::STRING_MAX_LEN + 1) });
    check(value);

    let mut value = base("stat");
    value["query"]["range"] = json!({ "kind": "preset" });
    check(value);

    let mut value = base("stat");
    value["query"]["range"] = json!({ "kind": "followPage", "key": "7d" });
    check(value);

    // client 必须属于已启用集合。
    let mut value = base("stat");
    value["query"]["client"] = json!("claude_code");
    let parsed = spec::parse_spec(value).expect("parse");
    assert!(spec::validate_spec(parsed, &[Client::Codex]).is_err());
}

#[test]
fn spec_version_is_classified_and_migrated() {
    use spec::SpecVersionClass::*;
    assert_eq!(spec::classify_spec_version(1), Current);
    assert_eq!(spec::classify_spec_version(0), Migrate(0));
    assert_eq!(spec::classify_spec_version(9), Unsupported(9));

    // 当前版本迁移是幂等的。
    let value = base("stat");
    assert_eq!(spec::migrate_spec(value.clone()).expect("migrate"), value);
    // 更高版本不得静默丢弃：解析被拒绝。
    let mut future = base("stat");
    future["specVersion"] = json!(9);
    assert!(spec::parse_spec(future).is_err());
}

// ---------------------------------------------------------------------------------------------
// 分发器：真实查询与上限
// ---------------------------------------------------------------------------------------------

#[test]
fn run_each_panel_type_against_synthetic_store() {
    let store = seeded_store();
    let enabled = Client::ALL.to_vec();
    let from = ms(D0);
    let to = ms(D2) + DAY_MS;
    let trend_args = PanelQueryArgs {
        from_ms: Some(from),
        to_ms: Some(to),
        day_start_ms: Some(from),
        day_count: Some(7),
        ..Default::default()
    };

    let result = query::execute(
        &store,
        &enabled,
        typed(base("stat")),
        PanelQueryArgs::default(),
    )
    .expect("stat session");
    assert!(matches!(result, PanelDataDto::Overview(_)));

    let mut spec = typed(base("stat"));
    spec.query.time_basis = Some(TimeBasis::Request);
    let result =
        query::execute(&store, &enabled, spec, PanelQueryArgs::default()).expect("stat request");
    assert!(matches!(result, PanelDataDto::RequestAnalysis(_)));

    let result = query::execute(
        &store,
        &enabled,
        typed(base("token_trend")),
        trend_args.clone(),
    )
    .expect("trend");
    assert!(matches!(result, PanelDataDto::DailyTokenTrend(_)));

    let calendar_args = PanelQueryArgs {
        day_count: Some(30),
        ..trend_args.clone()
    };
    let result = query::execute(
        &store,
        &enabled,
        typed(base("activity_calendar")),
        calendar_args,
    )
    .expect("calendar");
    assert!(matches!(result, PanelDataDto::DailyTokenTrend(_)));

    let activity_args = PanelQueryArgs {
        from_ms: Some(from),
        to_ms: Some(to),
        day_boundaries: vec![from, from + DAY_MS, to],
        ..Default::default()
    };
    let result = query::execute(
        &store,
        &enabled,
        typed(base("request_trend")),
        activity_args,
    )
    .expect("activity");
    assert!(matches!(result, PanelDataDto::RequestActivity(_)));

    let result = query::execute(
        &store,
        &enabled,
        typed(base("client_distribution")),
        PanelQueryArgs::default(),
    )
    .expect("clients");
    assert!(matches!(result, PanelDataDto::ClientUsage(_)));

    let result = query::execute(
        &store,
        &enabled,
        typed(base("ranking")),
        PanelQueryArgs::default(),
    )
    .expect("ranking");
    assert!(matches!(result, PanelDataDto::Rankings(_)));

    let result = query::execute(
        &store,
        &enabled,
        typed(base("session_table")),
        PanelQueryArgs::default(),
    )
    .expect("sessions");
    assert!(matches!(result, PanelDataDto::SessionTable(_)));
}

#[test]
fn dispatcher_enforces_catalog_limits_at_the_command_boundary() {
    let store = seeded_store();
    let enabled = Client::ALL.to_vec();

    let mut spec = typed(base("ranking"));
    spec.type_options.limit = Some(spec::RANKING_LIMIT_MAX);
    assert!(query::execute(&store, &enabled, spec.clone(), PanelQueryArgs::default()).is_ok());
    spec.type_options.limit = Some(spec::RANKING_LIMIT_MAX + 1);
    assert!(query::execute(&store, &enabled, spec, PanelQueryArgs::default()).is_err());

    let calendar = typed(base("activity_calendar"));
    let ok = PanelQueryArgs {
        from_ms: Some(ms(D0)),
        day_count: Some(spec::CALENDAR_DAY_COUNT_MAX),
        ..Default::default()
    };
    assert!(query::execute(&store, &enabled, calendar.clone(), ok).is_ok());
    let too_many = PanelQueryArgs {
        from_ms: Some(ms(D0)),
        day_count: Some(spec::CALENDAR_DAY_COUNT_MAX + 1),
        ..Default::default()
    };
    assert!(query::execute(&store, &enabled, calendar, too_many).is_err());

    let trend = typed(base("token_trend"));
    let too_many = PanelQueryArgs {
        from_ms: Some(ms(D0)),
        day_count: Some(spec::DAY_COUNT_MAX + 1),
        ..Default::default()
    };
    assert!(query::execute(&store, &enabled, trend, too_many).is_err());
}

#[test]
fn search_is_matched_literally_and_cannot_reach_sql() {
    let store = seeded_store();
    let enabled = Client::ALL.to_vec();
    let before = store.session_count().expect("count");

    let mut spec = typed(base("session_table"));
    spec.type_options.search = Some("'; DROP TABLE sessions;--".to_string());
    let result =
        query::execute(&store, &enabled, spec, PanelQueryArgs::default()).expect("injection try");
    assert!(matches!(result, PanelDataDto::SessionTable(rows) if rows.is_empty()));
    assert_eq!(
        store.session_count().expect("count"),
        before,
        "no row must be lost"
    );

    // 同样的字段做字面匹配仍然有效。
    let mut spec = typed(base("session_table"));
    spec.type_options.search = Some("alpha".to_string());
    let result =
        query::execute(&store, &enabled, spec, PanelQueryArgs::default()).expect("literal match");
    assert!(matches!(result, PanelDataDto::SessionTable(rows) if !rows.is_empty()));
}

#[test]
fn query_args_tolerate_frontend_superset_and_use_limit_offset_fallbacks() {
    let store = seeded_store();
    let enabled = Client::ALL.to_vec();

    // 宽容：前端可附带 groupBy / sessionQuery 等展示层字段而不报错。
    let args: PanelQueryArgs = serde_json::from_value(json!({
        "limit": 3,
        "offset": 0,
        "groupBy": "model",
        "sessionQuery": { "search": "alpha" },
    }))
    .expect("tolerant args");
    let result = query::execute(&store, &enabled, typed(base("ranking")), args).expect("ranking");
    assert!(matches!(result, PanelDataDto::Rankings(_)));

    // 越界的 args.limit 按规格 ④ 上限拒绝。
    let args: PanelQueryArgs =
        serde_json::from_value(json!({ "limit": spec::RANKING_LIMIT_MAX + 1 })).expect("parse");
    assert!(query::execute(&store, &enabled, typed(base("ranking")), args).is_err());

    // session_table 经 args 传入 offset（规格 ② typeOptions 无此字段）。
    let args: PanelQueryArgs =
        serde_json::from_value(json!({ "offset": 1 })).expect("parse offset");
    let result =
        query::execute(&store, &enabled, typed(base("session_table")), args).expect("offset");
    assert!(matches!(result, PanelDataDto::SessionTable(_)));

    // 越界 offset 被拒绝。
    let args: PanelQueryArgs =
        serde_json::from_value(json!({ "offset": spec::SESSION_OFFSET_MAX + 1 })).expect("parse");
    assert!(query::execute(&store, &enabled, typed(base("session_table")), args).is_err());
}

// ---------------------------------------------------------------------------------------------
// 持久化与存活语义
// ---------------------------------------------------------------------------------------------

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "toklens-panels-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("synthetic directory");
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn panels_persist_validate_update_and_delete() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    let enabled = Client::ALL.to_vec();

    let saved = panel_store::save(&store, &enabled, base("token_trend")).expect("save");
    assert_eq!(
        saved.len(),
        1,
        "write commands return the full updated list"
    );
    assert!(saved[0].readable);
    assert_eq!(panel_store::list(&store).expect("list").len(), 1);

    // 从磁盘重载（版本化文档 + 原子写）。
    let reloaded = SettingsStore::load(&directory.0);
    assert_eq!(panel_store::list(&reloaded).expect("list").len(), 1);

    // 相同 id 原位更新；写命令返回更新后的完整列表。
    let mut updated = base("token_trend");
    updated["title"] = json!("renamed");
    let panels = panel_store::save(&store, &enabled, updated).expect("update");
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0].spec["title"], json!("renamed"));

    // 越界规格被拒绝，且不改写既有数据。
    let mut invalid = base("stat");
    invalid["layout"] = json!({ "colSpan": 5, "rowSpan": 1, "order": 0 });
    assert!(panel_store::save(&store, &enabled, invalid).is_err());
    assert_eq!(panel_store::list(&store).expect("list").len(), 1);

    let panels = panel_store::delete(&store, "p-token_trend").expect("delete");
    assert!(panels.is_empty());
}

#[test]
fn reorder_writes_order_and_rejects_mismatched_sets() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    let enabled = Client::ALL.to_vec();
    panel_store::save(&store, &enabled, base("stat")).expect("a");
    panel_store::save(&store, &enabled, base("ranking")).expect("b");

    let panels = panel_store::reorder(&store, &["p-ranking".to_string(), "p-stat".to_string()])
        .expect("reorder");
    let ids: Vec<&str> = panels
        .iter()
        .map(|panel| panel.spec["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, vec!["p-ranking", "p-stat"]);
    assert_eq!(panels[0].spec["layout"]["order"], json!(0));
    assert_eq!(panels[1].spec["layout"]["order"], json!(1));

    assert!(panel_store::reorder(&store, &["p-ranking".to_string()]).is_err());
    assert!(
        panel_store::reorder(&store, &["p-ranking".to_string(), "p-ranking".to_string()]).is_err()
    );
}

#[test]
fn higher_spec_version_is_preserved_and_marked_read_only() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    store
        .transform_panels(|panels| {
            panels.push(json!({
                "specVersion": 99,
                "id": "p-future",
                "type": "quantum",
                "future": true,
            }));
            Ok(())
        })
        .expect("inject future spec");

    let panels = panel_store::list(&store).expect("list");
    assert_eq!(panels.len(), 1);
    assert!(!panels[0].readable);
    assert_eq!(panels[0].spec_version, 99);
    assert_eq!(panels[0].spec["future"], json!(true));

    // 只读面板不得被改写（reorder 不注入 layout）。
    panel_store::reorder(&store, &["p-future".to_string()]).expect("reorder single");
    let after = panel_store::list(&store).expect("list");
    assert!(after[0].spec.get("layout").is_none());
}

#[test]
fn clearing_and_resetting_settings_keep_panels() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    let enabled = Client::ALL.to_vec();
    panel_store::save(&store, &enabled, base("stat")).expect("save");

    // clear_statistics 只暂停采集（SettingsPatch），绝不删面板。
    store
        .update(SettingsPatch {
            collection_paused: Some(true),
            ..Default::default()
        })
        .expect("clear-like");
    assert_eq!(panel_store::list(&store).expect("list").len(), 1);

    // reset_settings 只重置 AppSettings，绝不触碰 panels。
    store
        .transform(|_| Ok(AppSettings::default()))
        .expect("reset-like");
    assert_eq!(panel_store::list(&store).expect("list").len(), 1);

    // 独立的重置入口只清空面板。
    let panels = panel_store::reset(&store).expect("reset panels");
    assert!(panels.is_empty());
}
