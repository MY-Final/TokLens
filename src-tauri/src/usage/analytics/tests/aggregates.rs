use super::*;

#[test]
fn range_totals_respect_range_and_client_filters() {
    let store = seeded_store();
    let all = store
        .range_totals(&AnalyticsFilter::default())
        .expect("all totals");
    assert_eq!(all.sessions, 4);
    assert_eq!(all.messages, 3);
    assert_eq!(
        all.sessions_total_mismatch, 0,
        "测试数据的分项之和与客户端总量一致"
    );
    assert_eq!(all.sessions_without_usage, 1, "没有用量数据的会话单独计数");
    assert_eq!(all.sums.input_tokens, 600 + 2_000 + 500);
    assert_eq!(all.sums.output_tokens, 80 + 300 + 50);
    assert_eq!(all.sums.reasoning_tokens, 20);
    assert_eq!(all.sums.cache_read_tokens, 500);
    assert_eq!(all.sums.cache_write_tokens, 5);
    assert_eq!(all.sums.total_tokens, 1_100 + 2_300 + 650);

    let codex_only = store
        .range_totals(&AnalyticsFilter {
            client: Some(Client::Codex),
            ..AnalyticsFilter::default()
        })
        .expect("codex totals");
    assert_eq!(codex_only.sessions, 3);
    assert_eq!(codex_only.messages, 3);
    assert_eq!(codex_only.sums.total_tokens, 1_100 + 2_300);

    let last_day = store
        .range_totals(&AnalyticsFilter {
            from_ms: Some(ms("2026-10-07T00:00:00.000Z")),
            ..AnalyticsFilter::default()
        })
        .expect("range totals");
    assert_eq!(last_day.sessions, 1);
    assert_eq!(last_day.sums.total_tokens, 0);
    assert_eq!(last_day.sessions_without_usage, 1);
}

#[test]
fn daily_trend_buckets_by_session_start_and_fills_gaps() {
    let store = seeded_store();
    let buckets = store
        .daily_token_trend(ms("2026-10-05T00:00:00.000Z"), 4, Some(Client::Codex))
        .expect("trend");
    assert_eq!(buckets.len(), 4, "缺失日期补 0");
    assert_eq!(buckets[0].day_start_ms, ms("2026-10-05T00:00:00.000Z"));
    assert_eq!(buckets[0].sessions, 1);
    assert_eq!(buckets[0].messages, 2);
    assert_eq!(buckets[0].sums.total_tokens, 1_100);
    assert_eq!(buckets[1].sessions, 1);
    assert_eq!(buckets[1].sums.total_tokens, 2_300);
    assert_eq!(buckets[2].sessions, 1);
    assert_eq!(buckets[2].sums.total_tokens, 0);
    assert_eq!(buckets[3].sessions, 0);
    assert_eq!(buckets[3].sums, TokenSums::default());
}

#[test]
fn client_usage_groups_by_client() {
    let store = seeded_store();
    let usage = store
        .client_usage(&AnalyticsFilter::default())
        .expect("usage");
    assert_eq!(usage.len(), 2, "只返回库中真实存在的客户端");
    let codex = usage
        .iter()
        .find(|entry| entry.client == Client::Codex)
        .expect("codex usage");
    assert_eq!(codex.sessions, 3);
    assert_eq!(codex.messages, 3);
    assert_eq!(codex.sums.total_tokens, 3_400);
    let claude = usage
        .iter()
        .find(|entry| entry.client == Client::ClaudeCode)
        .expect("claude usage");
    assert_eq!(claude.sums.total_tokens, 650);
}

#[test]
fn totals_flag_sessions_whose_client_total_disagrees_with_its_buckets() {
    let store = UsageStore::open_in_memory().expect("store");
    // 客户端自报总量 (2_000) 与 input + output (1_100) 不一致：保留客户端值并计数。
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\m.jsonl"),
            &codex_record(
                "thread-m",
                D0,
                D0,
                Some("gpt-5.6-luna"),
                Some(PROJECT_ALPHA),
                counts(
                    Some(1_000),
                    Some(100),
                    Some(0),
                    Some(0),
                    Some(0),
                    Some(2_000),
                ),
            ),
            &[],
        )
        .expect("import mismatch");
    let totals = store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals");
    assert_eq!(totals.sessions_total_mismatch, 1);
    assert_eq!(totals.sums.total_tokens, 2_000, "总量取客户端值");
    assert_eq!(
        totals.sums.input_tokens + totals.sums.output_tokens,
        1_100,
        "分桶按客户端分项计算"
    );
}
