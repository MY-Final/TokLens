use super::*;

#[test]
fn daily_exact_range_filters_sessions_and_requests_with_independent_buckets() {
    let store = UsageStore::open_in_memory().expect("store");
    let from = ms("2026-10-05T10:30:00.000Z");
    // An inclusive minute selection ends immediately before this exclusive boundary.
    let to = ms("2026-10-06T11:46:00.000Z");
    for (id, at) in [
        ("before", from - 1),
        ("start", from),
        ("last-minute", to - 1),
        ("end", to),
        ("outside-buckets", ms(D0) + 4 * 86_400_000),
    ] {
        let mut record = codex_record(
            id,
            D0,
            D0,
            None,
            None,
            counts(Some(10), Some(2), Some(0), Some(0), Some(0), Some(12)),
        );
        record.started_at_ms = Some(at);
        store
            .import_session(
                &source(Client::Codex, id),
                &record,
                &[message(id, id, 1, "2026-10-10T00:00:00.000Z")],
            )
            .expect("synthetic session");
    }
    let mut unknown = codex_record("unknown", D0, D0, None, None, RawTokenCounts::default());
    unknown.started_at_ms = Some(from + 1);
    unknown.coverage = crate::usage::model::MessageCoverage::Partial;
    store.upsert_session(&unknown).expect("unknown");
    let mut claude = claude_record(D0, PROJECT_ALPHA, "fixture");
    claude.started_at_ms = Some(from);
    store.upsert_session(&claude).expect("claude");
    let buckets = store
        .daily_token_trend_in_range(ms(D0), 4, Some(from), Some(to), Some(Client::Codex))
        .expect("exact trend");
    assert_eq!(buckets.len(), 4);
    assert_eq!(buckets[0].day_start_ms, ms(D0));
    assert_eq!(
        (
            buckets[0].sessions,
            buckets[0].messages,
            buckets[0].sessions_without_usage,
            buckets[0].sessions_partial,
            buckets[0].sums.total_tokens
        ),
        (2, 1, 1, 1, 12)
    );
    assert_eq!(
        (
            buckets[1].sessions,
            buckets[1].messages,
            buckets[1].sums.total_tokens
        ),
        (1, 1, 12)
    );
    assert_eq!((buckets[2].sessions, buckets[3].sessions), (0, 0));
    let all = store
        .daily_token_trend_in_range(ms(D0), 4, Some(from), Some(to), None)
        .expect("all clients");
    assert_eq!(all[0].sessions, 3);
    assert_eq!(all[0].sums.total_tokens, 662);
    let legacy = store
        .daily_token_trend(ms(D0), 4, Some(Client::Codex))
        .expect("legacy");
    assert_eq!(legacy.iter().map(|bucket| bucket.sessions).sum::<i64>(), 5);
    assert_eq!(legacy.iter().map(|bucket| bucket.messages).sum::<i64>(), 4);
    assert_eq!(
        legacy
            .iter()
            .map(|bucket| bucket.sums.total_tokens)
            .sum::<i64>(),
        48
    );
    let clipped = store
        .daily_token_trend_in_range(
            ms(D0),
            1,
            Some(from),
            Some(to + 10 * 86_400_000),
            Some(Client::Codex),
        )
        .expect("bucket bounds");
    assert_eq!(clipped[0].sessions, 2);
}

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
    assert_eq!(codex.share_bps, 8_395);
    let claude = usage
        .iter()
        .find(|entry| entry.client == Client::ClaudeCode)
        .expect("claude usage");
    assert_eq!(claude.sums.total_tokens, 650);
    assert_eq!(claude.share_bps, 1_604);
}

#[test]
fn counted_cache_writes_include_claude_and_dsh_but_exclude_codex() {
    let store = seeded_store();
    let mut claude = claude_record(D0, PROJECT_ALPHA, "fixture-model");
    claude.client_session_id = "cache-write-fixture".into();
    claude.usage = crate::usage::tokens::normalize_usage(
        crate::usage::tokens::semantics_for(Client::ClaudeCode),
        &counts(Some(10), Some(5), Some(0), Some(4), None, None),
    );
    store.upsert_session(&claude).expect("claude cache write");
    let dsh =
        crate::usage::normalize::dsh::dsh_session(&crate::usage::normalize::dsh::DshSession {
            id: "cache-write-fixture",
            parent: None,
            cwd: None,
            model: None,
            start_ms: Some(ms(D0)),
            end_ms: Some(ms(D0)),
            message_count: 1,
            usage: counts(Some(10), Some(5), Some(100), Some(7), Some(2), None),
        });
    store.upsert_session(&dsh).expect("dsh cache write");
    let totals = store
        .range_totals(&AnalyticsFilter::default())
        .expect("totals");
    assert_eq!(totals.sums.cache_write_tokens, 16);
    assert_eq!(totals.sums.counted_cache_write_tokens, 11);
    let trend = store.daily_token_trend(ms(D0), 1, None).expect("trend");
    assert_eq!(trend[0].sums.counted_cache_write_tokens, 11);
    let codex = store
        .range_totals(&AnalyticsFilter {
            client: Some(Client::Codex),
            ..Default::default()
        })
        .expect("codex");
    assert_eq!(codex.sums.counted_cache_write_tokens, 0);
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

#[test]
fn daily_completeness_distinguishes_unknown_zero_mixed_and_partial() {
    let store = UsageStore::open_in_memory().expect("store");
    let mut unknown = codex_record("unknown", D0, D0, None, None, RawTokenCounts::default());
    unknown.coverage = crate::usage::model::MessageCoverage::Partial;
    store.upsert_session(&unknown).expect("unknown session");
    let zero = codex_record(
        "zero",
        D1,
        D1,
        None,
        None,
        counts(Some(0), Some(0), Some(0), Some(0), Some(0), Some(0)),
    );
    store.upsert_session(&zero).expect("zero session");
    let unknown_mixed = codex_record(
        "mixed-unknown",
        D2,
        D2,
        None,
        None,
        RawTokenCounts::default(),
    );
    store.upsert_session(&unknown_mixed).expect("mixed unknown");
    let mut known_mixed = codex_record(
        "mixed-known",
        D2,
        D2,
        None,
        None,
        counts(Some(10), Some(2), Some(0), Some(0), Some(0), Some(12)),
    );
    known_mixed.coverage = crate::usage::model::MessageCoverage::Partial;
    // Requests retain start-date attribution even when their timestamps differ.
    store
        .import_session(
            &source(Client::Codex, "synthetic-mixed.jsonl"),
            &known_mixed,
            &[message(
                "mixed-known",
                "later-request",
                1,
                "2026-10-08T01:00:00.000Z",
            )],
        )
        .expect("mixed known");
    let claude = claude_record(D0, PROJECT_ALPHA, "fixture-model");
    store.upsert_session(&claude).expect("other client");

    let codex = store
        .daily_token_trend(ms(D0), 4, Some(Client::Codex))
        .expect("codex days");
    assert_eq!(
        (
            codex[0].sessions,
            codex[0].sessions_without_usage,
            codex[0].sums.total_tokens
        ),
        (1, 1, 0)
    );
    assert_eq!(codex[0].sessions_partial, 1);
    assert_eq!(
        (
            codex[1].sessions,
            codex[1].sessions_without_usage,
            codex[1].sums.total_tokens
        ),
        (1, 0, 0)
    );
    assert_eq!(
        (
            codex[2].sessions,
            codex[2].sessions_without_usage,
            codex[2].sums.total_tokens
        ),
        (2, 1, 12)
    );
    assert_eq!(
        codex[2].sessions_partial, 1,
        "known cumulative total can have partial request coverage"
    );
    assert_eq!(codex[2].messages, 1);
    assert_eq!(codex[3].messages, 0);
    assert_eq!(
        (
            codex[3].sessions,
            codex[3].sessions_without_usage,
            codex[3].sessions_partial
        ),
        (0, 0, 0)
    );
    assert_eq!(codex[3].sums, TokenSums::default());

    let all = store.daily_token_trend(ms(D0), 4, None).expect("all days");
    assert_eq!(
        (
            all[0].sessions,
            all[0].sessions_without_usage,
            all[0].sums.total_tokens
        ),
        (2, 1, 650)
    );
    let claude = store
        .daily_token_trend(ms(D0), 4, Some(Client::ClaudeCode))
        .expect("claude days");
    assert_eq!(
        (
            claude[0].sessions,
            claude[0].sessions_without_usage,
            claude[0].sessions_partial
        ),
        (1, 0, 0)
    );
    assert_eq!(claude[1].sessions, 0);
    assert_eq!(claude[1].sessions_without_usage, 0);
    assert!(
        store
            .daily_token_trend(ms(D0), 0, None)
            .expect("no days")
            .is_empty()
    );
}
