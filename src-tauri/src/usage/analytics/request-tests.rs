use super::*;
use rusqlite::params;

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
