use super::*;
use rusqlite::params;

#[test]
fn new_client_filters_preserve_unknown_usage_and_only_mcode_counts_cache_writes() {
    let store = UsageStore::open_in_memory().expect("store");
    for (index, client) in [Client::GeminiCli, Client::Grok, Client::Mcode]
        .into_iter()
        .enumerate()
    {
        let id = client.as_str();
        session(&store, id, client, "synthetic-project", 0, false);
        request(
            &store,
            &format!("{id}-known"),
            id,
            Some(10),
            Some(100),
            "measured",
        );
        request(
            &store,
            &format!("{id}-unknown"),
            id,
            Some(20),
            None,
            "derived",
        );
        store
            .conn
            .execute(
                "UPDATE messages SET cache_write_tokens=?1 WHERE id=?2",
                params![index as i64 + 4, format!("{id}-known")],
            )
            .expect("synthetic cache writes");
        store
            .conn
            .execute(
                "UPDATE sessions SET cache_write_tokens=?1 WHERE id=?2",
                params![index as i64 + 4, id],
            )
            .expect("synthetic session cache writes");
        let filter = AnalyticsFilter {
            client: Some(client),
            ..Default::default()
        };
        let analysis = store
            .request_analysis(&filter)
            .expect("new client analysis");
        assert_eq!(analysis.summary.totals.requests, 2);
        assert_eq!(analysis.summary.totals.requests_without_usage, 1);
        assert_eq!(analysis.summary.totals.sums.total_tokens, 100);
        assert_eq!(analysis.summary.average_tokens, Some(100.0));
        let expected = if client == Client::Mcode { 6 } else { 0 };
        assert_eq!(
            analysis.summary.totals.sums.counted_cache_write_tokens,
            expected
        );
        assert_eq!(
            store
                .range_totals(&filter)
                .expect("session totals")
                .sums
                .counted_cache_write_tokens,
            expected
        );
    }
}

fn session(store: &UsageStore, id: &str, client: Client, project: &str, start: i64, partial: bool) {
    store.conn.execute(
        "INSERT INTO sessions (id,client,client_session_id,project_path,started_at_ms,
         semantics_id,total_basis,message_coverage,source_kind,total_tokens,cost_source,imported_at_ms,updated_at_ms)
         VALUES (?1,?2,?1,?3,?4,'synthetic','client_reported',?5,'measured',9999,'none',0,0)",
        params![id,client.as_str(),project,start,if partial { "partial" } else { "complete" }],
    ).expect("synthetic session");
}

fn request(
    store: &UsageStore,
    id: &str,
    session: &str,
    at: Option<i64>,
    total: Option<i64>,
    kind: &str,
) {
    store.conn.execute(
        "INSERT INTO messages (id,session_id,client,dedup_key,created_at_ms,semantics_id,source_kind,
         input_tokens,output_tokens,cache_write_tokens,total_tokens,cost_source,imported_at_ms,updated_at_ms)
         SELECT ?1,id,client,?1,?3,'synthetic',?5,?4,0,0,?4,'none',0,0 FROM sessions WHERE id=?2",
        params![id,session,at,total,kind],
    ).expect("synthetic request");
}

#[test]
fn request_time_range_is_half_open_and_independent_of_session_start_and_totals() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "early", Client::Codex, "alpha", 0, true);
    request(&store, "before", "early", Some(99), Some(5), "measured");
    request(&store, "at-start", "early", Some(100), Some(12), "measured");
    request(&store, "next-day", "early", Some(200), Some(3), "derived");
    request(&store, "at-end", "early", Some(300), Some(7), "measured");
    let filter = AnalyticsFilter {
        from_ms: Some(100),
        to_ms: Some(300),
        ..Default::default()
    };
    let activity = store
        .request_activity(&filter, &[100, 200, 300])
        .expect("activity");
    assert_eq!(activity.range.requests, 2);
    assert_eq!(activity.range.sessions, 1);
    assert_eq!(activity.range.sums.total_tokens, 15);
    assert_eq!(activity.range.measured_requests, 1);
    assert_eq!(activity.range.derived_requests, 1);
    assert_eq!(activity.daily[0].totals.sums.total_tokens, 12);
    assert_eq!(activity.daily[1].totals.sums.total_tokens, 3);
    assert_eq!(activity.coverage.recorded_requests, 4);
    assert_eq!(activity.coverage.sessions_partial, 1);
    assert_eq!(store.range_totals(&filter).expect("legacy").sessions, 0);
}

#[test]
fn coverage_retains_untimed_and_estimated_requests_and_unknown_usage_is_not_zero() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "a", Client::Codex, "alpha", 0, true);
    session(&store, "empty", Client::Codex, "alpha", 0, false);
    request(&store, "unknown", "a", Some(10), None, "measured");
    request(&store, "zero", "a", Some(20), Some(0), "derived");
    request(&store, "untimed", "a", None, Some(50), "measured");
    request(&store, "guess", "a", Some(20), Some(999), "estimated");
    let filter = AnalyticsFilter {
        from_ms: Some(10),
        to_ms: Some(30),
        ..Default::default()
    };
    let result = store.request_activity(&filter, &[0, 30]).expect("activity");
    assert_eq!(result.range.requests, 2);
    assert_eq!(result.range.requests_with_usage, 1);
    assert_eq!(result.range.requests_without_usage, 1);
    assert_eq!(result.range.sums.total_tokens, 0);
    assert_eq!(
        result.coverage,
        RequestCoverage {
            recorded_requests: 4,
            timed_requests: 2,
            untimed_requests: 1,
            estimated_requests: 1,
            sessions_partial: 1,
            sessions_without_requests: 1,
        }
    );
    let empty = store
        .request_activity(
            &AnalyticsFilter {
                from_ms: Some(100),
                to_ms: Some(200),
                ..filter
            },
            &[100, 200],
        )
        .expect("empty range");
    assert_eq!(empty.range.requests, 0);
    assert_eq!(empty.coverage, result.coverage);
}

#[test]
fn client_and_project_filters_apply_to_request_and_coverage_queries() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "a", Client::Codex, "alpha", 0, false);
    session(&store, "b", Client::ClaudeCode, "alpha", 0, false);
    session(&store, "c", Client::Codex, "beta", 0, false);
    for id in ["a", "b", "c"] {
        request(&store, id, id, Some(10), Some(5), "measured");
    }
    let all = store
        .request_activity(&AnalyticsFilter::default(), &[0, 20])
        .expect("all");
    assert_eq!(all.daily[0].clients.len(), 2);
    let filter = AnalyticsFilter {
        client: Some(Client::Codex),
        project_path: Some("alpha"),
        ..Default::default()
    };
    let result = store.request_activity(&filter, &[0, 20]).expect("filtered");
    assert_eq!(result.range.requests, 1);
    assert_eq!(result.coverage.recorded_requests, 1);
    assert_eq!(result.daily[0].clients[0].client, Client::Codex);
}

#[test]
fn local_day_boundaries_support_dst_short_and_long_days() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "a", Client::Codex, "alpha", 0, false);
    let hour = 3_600_000;
    let boundaries = [0, 23 * hour, 48 * hour, 72 * hour];
    for (index, at) in [
        23 * hour - 1,
        23 * hour,
        48 * hour - 1,
        48 * hour,
        72 * hour,
    ]
    .into_iter()
    .enumerate()
    {
        request(
            &store,
            &index.to_string(),
            "a",
            Some(at),
            Some(1),
            "measured",
        );
    }
    let filter = AnalyticsFilter {
        from_ms: Some(0),
        to_ms: Some(72 * hour),
        ..Default::default()
    };
    let result = store.request_activity(&filter, &boundaries).expect("dst");
    assert_eq!(
        result
            .daily
            .iter()
            .map(|day| day.totals.requests)
            .collect::<Vec<_>>(),
        vec![1, 2, 1]
    );
    assert_eq!(result.range.requests, 4);
}

#[test]
fn four_hundred_display_days_do_not_truncate_range_and_invalid_boundaries_are_rejected() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "a", Client::Codex, "alpha", 0, false);
    request(&store, "older", "a", Some(0), Some(10), "measured");
    request(&store, "visible", "a", Some(450), Some(20), "measured");
    let filter = AnalyticsFilter {
        from_ms: Some(0),
        to_ms: Some(500),
        ..Default::default()
    };
    let boundaries: Vec<_> = (100..=500).collect();
    let result = store
        .request_activity(&filter, &boundaries)
        .expect("400 days");
    assert_eq!(result.daily.len(), 400);
    assert_eq!(result.range.requests, 2);
    assert_eq!(result.range.sums.total_tokens, 30);
    assert_eq!(result.clients[0].metrics.totals.requests, 2);
    assert_eq!(result.clients[0].metrics.totals.sums.total_tokens, 30);
    assert_eq!(
        result
            .daily
            .iter()
            .map(|day| day.totals.requests)
            .sum::<i64>(),
        1
    );
    assert_eq!(
        store
            .request_activity(&filter, &[])
            .expect("no daily")
            .range,
        result.range
    );
    for invalid in [vec![0], vec![0, 0], vec![1, 0], (0..402).collect()] {
        assert!(store.request_activity(&filter, &invalid).is_err());
    }
}

#[test]
fn overview_clients_match_analysis_for_continued_sessions_zero_and_unknown_usage() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "continued", Client::WorkBuddy, "alpha", 0, false);
    session(&store, "zero", Client::Codex, "alpha", 0, false);
    session(&store, "unknown", Client::ClaudeCode, "alpha", 0, false);
    session(&store, "inactive", Client::OpenCode, "alpha", 0, false);
    request(&store, "before", "continued", Some(99), Some(7), "measured");
    request(&store, "today", "continued", Some(100), Some(12), "derived");
    request(&store, "zero", "zero", Some(150), Some(0), "measured");
    request(&store, "unknown", "unknown", Some(150), None, "measured");
    request(&store, "old", "inactive", Some(99), Some(99), "measured");
    let filter = AnalyticsFilter {
        from_ms: Some(100),
        to_ms: Some(200),
        ..Default::default()
    };
    let activity = store
        .request_activity(&filter, &[100, 200])
        .expect("activity");
    let analysis = store.request_analysis(&filter).expect("analysis");
    assert_eq!(activity.clients, analysis.clients);
    assert_eq!(activity.clients.len(), 3);
    assert_eq!(activity.clients[0].key.as_deref(), Some("workbuddy"));
    assert_eq!(activity.clients[0].metrics.totals.sums.total_tokens, 12);
    assert_eq!(activity.clients[0].metrics.share_bps, Some(10_000));
    assert_eq!(activity.clients[1].key.as_deref(), Some("codex"));
    assert_eq!(activity.clients[1].metrics.totals.requests_with_usage, 1);
    assert_eq!(activity.clients[1].metrics.share_bps, Some(0));
    assert_eq!(activity.clients[2].key.as_deref(), Some("claude_code"));
    assert_eq!(activity.clients[2].metrics.totals.requests_without_usage, 1);
    assert_eq!(activity.clients[2].metrics.share_bps, None);
    assert_eq!(
        store.range_totals(&filter).expect("session start").sessions,
        0
    );
    let empty_filter = AnalyticsFilter {
        from_ms: Some(200),
        to_ms: Some(300),
        ..filter
    };
    assert!(
        store
            .request_activity(&empty_filter, &[200, 300])
            .expect("empty")
            .clients
            .is_empty()
    );
    let client_filter = AnalyticsFilter {
        client: Some(Client::WorkBuddy),
        ..filter
    };
    let filtered = store
        .request_activity(&client_filter, &[100, 200])
        .expect("client");
    assert_eq!(filtered.clients.len(), 1);
    assert_eq!(filtered.clients[0].metrics.totals, filtered.range);
}

#[test]
fn analysis_compares_full_range_and_client_drilldown_with_known_only_averages() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "a", Client::Codex, "alpha", -100, false);
    session(&store, "b", Client::ClaudeCode, "beta", 999, false);
    session(&store, "c", Client::OpenCode, "gamma", 0, false);
    request(&store, "a1", "a", Some(100), Some(100), "measured");
    request(&store, "a2", "a", Some(150), None, "measured");
    request(&store, "b1", "b", Some(120), Some(20), "derived");
    request(&store, "c1", "c", Some(130), None, "measured");
    request(
        &store,
        "excluded-start",
        "a",
        Some(99),
        Some(999),
        "measured",
    );
    request(
        &store,
        "excluded-end",
        "a",
        Some(200),
        Some(999),
        "measured",
    );
    request(
        &store,
        "excluded-guess",
        "a",
        Some(120),
        Some(999),
        "estimated",
    );
    request(&store, "excluded-time", "a", None, Some(999), "measured");
    store
        .conn
        .execute("UPDATE sessions SET model='main-model' WHERE id='a'", [])
        .expect("model");
    store
        .conn
        .execute(
            "UPDATE messages SET model='request-model' WHERE id='a1'",
            [],
        )
        .expect("request model");
    let filter = AnalyticsFilter {
        from_ms: Some(100),
        to_ms: Some(200),
        ..Default::default()
    };
    let result = store.request_analysis(&filter).expect("analysis");
    assert_eq!(result.summary.totals.requests, 4);
    assert_eq!(result.summary.totals.sessions, 3);
    assert_eq!(result.summary.average_tokens, Some(60.0));
    assert_eq!(result.clients[0].key.as_deref(), Some("codex"));
    assert_eq!(result.clients[0].metrics.share_bps, Some(8333));
    assert_eq!(result.clients[0].metrics.average_tokens, Some(100.0));
    assert_eq!(result.clients[2].key.as_deref(), Some("opencode"));
    assert_eq!(result.clients[2].metrics.average_tokens, None);
    assert_eq!(result.clients[2].metrics.share_bps, None);
    assert_eq!(result.models[0].key.as_deref(), Some("main-model"));
    assert!(result.models.iter().any(|entry| entry.key.is_none()));
    let selected = store
        .request_analysis(&AnalyticsFilter {
            client: Some(Client::Codex),
            ..filter
        })
        .expect("drilldown");
    assert_eq!(selected.clients, result.clients);
    assert_eq!(selected.summary.totals.requests, 2);
    assert_eq!(selected.summary.average_tokens, Some(100.0));
    assert_eq!(selected.projects.len(), 1);
    assert_eq!(selected.projects[0].metrics.share_bps, Some(10_000));
    assert_eq!(selected.coverage.untimed_requests, 1);
    assert_eq!(selected.coverage.estimated_requests, 1);
    assert!(
        store
            .request_analysis(&AnalyticsFilter {
                from_ms: Some(200),
                to_ms: Some(100),
                ..filter
            })
            .is_err()
    );
}

#[test]
fn analysis_cache_ratio_uses_paired_fields_and_retains_unknown_and_zero() {
    let store = UsageStore::open_in_memory().expect("store");
    session(&store, "a", Client::Codex, "alpha", 0, false);
    request(&store, "paired", "a", Some(10), Some(100), "measured");
    request(&store, "missing", "a", Some(10), None, "measured");
    store.conn.execute("UPDATE messages SET input_tokens=25, cache_read_tokens=75, reasoning_tokens=NULL WHERE id='paired'", []).expect("paired");
    store.conn.execute("UPDATE messages SET input_tokens=NULL, cache_read_tokens=100, reasoning_tokens=NULL WHERE id='missing'", []).expect("unpaired");
    let result = store
        .request_analysis(&AnalyticsFilter::default())
        .expect("analysis");
    assert_eq!(result.summary.cache_hit_bps, Some(7500));
    assert_eq!(result.summary.cache_known_requests, 1);
    assert_eq!(result.summary.totals.sums.cache_read_tokens, 175);
    assert!(
        !result
            .summary
            .known_token_fields
            .iter()
            .any(|f| f == "reasoningTokens")
    );
    assert!(
        result
            .summary
            .known_token_fields
            .iter()
            .any(|f| f == "outputTokens")
    );
    store
        .conn
        .execute(
            "UPDATE messages SET total_tokens=0, input_tokens=0, cache_read_tokens=0",
            [],
        )
        .expect("zeros");
    let zeros = store
        .request_analysis(&AnalyticsFilter::default())
        .expect("zeros");
    assert_eq!(zeros.summary.average_tokens, Some(0.0));
    assert_eq!(zeros.summary.cache_hit_bps, None);
    assert_eq!(zeros.clients[0].metrics.share_bps, None);
    assert_eq!(
        ratio_bps(i128::from(i64::MAX), i128::from(i64::MAX) * 2),
        Some(5000)
    );
    let empty = store
        .request_analysis(&AnalyticsFilter {
            from_ms: Some(100),
            ..Default::default()
        })
        .expect("empty");
    assert!(empty.clients.is_empty());
    assert_eq!(empty.summary.average_tokens, None);
}

#[test]
fn analysis_returns_all_models_and_top_projects_with_full_range_shares() {
    let store = UsageStore::open_in_memory().expect("store");
    for i in 0..10 {
        let id = format!("group-{i}");
        session(&store, &id, Client::Codex, &id, 0, false);
        store
            .conn
            .execute("UPDATE sessions SET model=?1 WHERE id=?1", [&id])
            .expect("model");
        request(&store, &id, &id, Some(10), Some(10), "measured");
    }
    let result = store
        .request_analysis(&AnalyticsFilter::default())
        .expect("top groups");
    assert_eq!(result.models.len(), 10);
    assert_eq!(result.projects.len(), 8);
    assert_eq!(result.models[0].metrics.share_bps, Some(1000));
    assert_eq!(result.summary.totals.requests, 10);
}

#[test]
fn all_model_groups_keep_client_scope_unknown_fields_and_zero_usage() {
    let store = UsageStore::open_in_memory().expect("store");
    for i in 0..10 {
        let id = format!("model-{i}");
        session(&store, &id, Client::Codex, "alpha", 0, false);
        store
            .conn
            .execute("UPDATE sessions SET model=?1 WHERE id=?1", [&id])
            .expect("model");
        request(&store, &id, &id, Some(10), Some(10), "measured");
    }
    session(&store, "zero", Client::Codex, "alpha", 0, false);
    session(&store, "unknown", Client::Codex, "alpha", 0, false);
    session(&store, "other", Client::ClaudeCode, "beta", 0, false);
    store
        .conn
        .execute(
            "UPDATE sessions SET model=id WHERE id IN ('zero', 'unknown')",
            [],
        )
        .expect("special models");
    request(&store, "zero", "zero", Some(10), Some(0), "derived");
    request(&store, "unknown", "unknown", Some(10), None, "measured");
    request(&store, "other", "other", Some(10), Some(500), "measured");
    store
        .conn
        .execute("UPDATE messages SET input_tokens=NULL, reasoning_tokens=NULL, cache_read_tokens=0 WHERE id='unknown'", [])
        .expect("missing fields and known zero");
    let all = store
        .request_analysis(&AnalyticsFilter::default())
        .expect("all");
    let selected = store
        .request_analysis(&AnalyticsFilter {
            client: Some(Client::Codex),
            ..Default::default()
        })
        .expect("client models");
    assert_eq!(selected.models.len(), 12);
    assert_eq!(selected.clients, all.clients);
    assert_eq!(selected.summary.totals.sums.total_tokens, 100);
    assert!(selected.models.iter().all(|group| group.key.is_some()));
    assert_eq!(selected.models[0].metrics.share_bps, Some(1000));
    let zero = selected
        .models
        .iter()
        .find(|group| group.key.as_deref() == Some("zero"))
        .expect("zero model");
    assert_eq!(zero.metrics.totals.requests_with_usage, 1);
    assert_eq!(zero.metrics.totals.sums.total_tokens, 0);
    assert_eq!(zero.metrics.totals.derived_requests, 1);
    assert_eq!(zero.metrics.share_bps, Some(0));
    let unknown = selected.models.last().expect("unknown last");
    assert_eq!(unknown.key.as_deref(), Some("unknown"));
    assert_eq!(unknown.metrics.share_bps, None);
    assert_eq!(unknown.metrics.totals.requests_without_usage, 1);
    assert!(
        !unknown
            .metrics
            .known_token_fields
            .iter()
            .any(|field| field == "inputTokens" || field == "reasoningTokens")
    );
    assert!(
        unknown
            .metrics
            .known_token_fields
            .iter()
            .any(|field| field == "cacheReadTokens")
    );
    assert_eq!(unknown.metrics.totals.sums.cache_read_tokens, 0);
}
