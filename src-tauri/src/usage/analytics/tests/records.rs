use super::*;

#[test]
fn session_summaries_are_newest_first_with_coverage_metadata() {
    let store = seeded_store();
    let sessions = store
        .session_summaries(
            &AnalyticsFilter::default(),
            &SessionListOptions::default(),
            2,
            0,
        )
        .expect("sessions");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].client_session_id, "thread-c");
    assert_eq!(sessions[0].total_tokens, None, "无用量会话保持 NULL");
    assert_eq!(sessions[0].messages, 0);
    assert_eq!(sessions[1].client_session_id, "thread-b");

    let page_two = store
        .session_summaries(
            &AnalyticsFilter::default(),
            &SessionListOptions::default(),
            2,
            2,
        )
        .expect("sessions page 2");
    assert_eq!(page_two.len(), 2);
    let codex_a = page_two
        .iter()
        .find(|session| session.client_session_id == "thread-a")
        .expect("thread-a");
    assert_eq!(codex_a.coverage, "complete");
    assert_eq!(codex_a.total_basis, "client_cumulative");
    assert_eq!(codex_a.input_tokens, Some(600), "分桶已扣除缓存子集");
    assert_eq!(codex_a.cache_write_tokens, Some(5));
    assert_eq!(
        codex_a.unknown_relations,
        vec!["cache_write.relation_to_input".to_string()]
    );
}

#[test]
fn source_status_and_bounds_reflect_imported_files() {
    let store = seeded_store();
    let status = store.client_source_status().expect("status");
    let codex = status
        .iter()
        .find(|entry| entry.client == Client::Codex)
        .expect("codex");
    assert_eq!(codex.sessions, 3);
    assert_eq!(codex.files_tracked, 3);
    assert_eq!(codex.unparsed_records, 0);
    assert_eq!(codex.last_scan_ms, Some(1_791_000_000_500));
    assert_eq!(codex.sessions_without_usage, 1);
    assert_eq!(
        codex.last_request_ms,
        Some(ms(D1)),
        "不得借用较新的无请求会话时间"
    );
    let opencode = status
        .iter()
        .find(|entry| entry.client == Client::OpenCode)
        .expect("opencode");
    assert_eq!(opencode.sessions, 0);
    assert_eq!(opencode.files_tracked, 0);
    assert_eq!(opencode.sessions_without_usage, 0);
    assert_eq!(opencode.last_request_ms, None);

    let (first, last) = store.data_bounds().expect("bounds");
    assert_eq!(first, Some(ms(D0)));
    assert_eq!(last, Some(ms(D2)));
}

#[test]
fn session_search_matches_literal_metadata_and_exact_groups() {
    let store = seeded_store();
    let filter = AnalyticsFilter::default();
    for (value, expected) in [("THREAD-B", 1), ("ASTRA", 1), ("alpha", 2), ("%", 0)] {
        let options = SessionListOptions {
            search: Some(value),
            ..Default::default()
        };
        assert_eq!(
            store
                .session_summaries(&filter, &options, 50, 0)
                .expect("search")
                .len(),
            expected
        );
    }
    let unknown = SessionListOptions {
        model_unknown: true,
        project_unknown: true,
        ..Default::default()
    };
    assert_eq!(
        store
            .session_summaries(&filter, &unknown, 50, 0)
            .expect("unknown")[0]
            .client_session_id,
        "thread-c"
    );
    let model = SessionListOptions {
        model: Some("gpt-5.6-luna"),
        ..Default::default()
    };
    assert_eq!(
        store
            .session_summaries(
                &AnalyticsFilter {
                    project_path: Some(PROJECT_ALPHA),
                    ..filter
                },
                &model,
                50,
                0
            )
            .expect("combined exact")
            .len(),
        1
    );
}

#[test]
fn session_sort_preserves_unknowns_and_stable_pages() {
    let store = seeded_store();
    let filter = AnalyticsFilter::default();
    let tokens = SessionListOptions {
        sort: SessionSort::TotalTokens,
        ..Default::default()
    };
    let rows = store
        .session_summaries(&filter, &tokens, 50, 0)
        .expect("tokens");
    assert_eq!(rows[0].client_session_id, "thread-b");
    assert_eq!(rows.last().expect("last").client_session_id, "thread-c");
    for sort in [SessionSort::Oldest, SessionSort::Requests] {
        assert_eq!(
            store
                .session_summaries(
                    &filter,
                    &SessionListOptions {
                        sort,
                        ..Default::default()
                    },
                    1,
                    0
                )
                .expect("sort")[0]
                .client_session_id,
            "thread-a"
        );
    }
    let first = store
        .session_summaries(&filter, &tokens, 2, 0)
        .expect("first page");
    let second = store
        .session_summaries(&filter, &tokens, 2, 2)
        .expect("second page");
    assert_eq!([first, second].concat(), rows);
}

#[test]
fn request_drilldown_includes_sessions_started_before_range() {
    let store = seeded_store();
    store
        .conn
        .execute(
            "UPDATE messages SET created_at_ms = ?1 WHERE dedup_key = 'resp-a1'",
            [ms(D2)],
        )
        .expect("cross-day fixture");
    let filter = AnalyticsFilter {
        from_ms: Some(ms(D2)),
        to_ms: Some(ms(D2) + 1),
        ..Default::default()
    };
    let options = SessionListOptions {
        request_time: true,
        ..Default::default()
    };
    let sessions = store
        .session_summaries(&filter, &options, 50, 0)
        .expect("request range");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].client_session_id, "thread-a");
    assert_eq!(
        sessions[0].total_tokens,
        Some(1_100),
        "full session cumulative values remain unchanged"
    );
    store
        .conn
        .execute(
            "UPDATE messages SET source_kind = 'estimated' WHERE dedup_key = 'resp-a1'",
            [],
        )
        .expect("estimated fixture");
    assert!(
        store
            .session_summaries(&filter, &options, 50, 0)
            .expect("estimated excluded")
            .is_empty()
    );
}

#[test]
fn source_diagnostics_keep_zero_usage_known_and_exclude_estimated_or_untimed_requests() {
    use crate::usage::model::SourceKind;
    let store = UsageStore::open_in_memory().expect("store");
    let session = codex_record(
        "zero",
        D0,
        D2,
        None,
        None,
        counts(Some(0), Some(0), Some(0), Some(0), Some(0), Some(0)),
    );
    let known = message("zero", "known", 1, D1);
    let mut estimated = message("zero", "estimated", 2, D2);
    estimated.source_kind = SourceKind::Estimated;
    let mut untimed = message("zero", "untimed", 3, D2);
    untimed.created_at_ms = None;
    store
        .import_session(
            &source(Client::Codex, "synthetic.jsonl"),
            &session,
            &[known, estimated, untimed],
        )
        .expect("synthetic records");
    let status = store.client_source_status().expect("status");
    let codex = status
        .iter()
        .find(|entry| entry.client == Client::Codex)
        .expect("codex");
    assert_eq!(codex.sessions, 1);
    assert_eq!(codex.sessions_without_usage, 0, "known zero is not unknown");
    assert_eq!(codex.last_request_ms, Some(ms(D1)));
    assert_eq!(
        codex.messages, 3,
        "diagnostic counts retain all imported requests"
    );
}

#[test]
fn session_list_skips_rows_with_unrecognized_client_ids() {
    let store = UsageStore::open_in_memory().expect("store");
    store
        .conn
        .execute(
            "INSERT INTO sessions
                (id, client, client_session_id, semantics_id, total_basis, message_coverage,
                 source_kind, cost_source, imported_at_ms, updated_at_ms)
             VALUES ('future:1', 'future_client', 'future-1', 'future.usage.v1',
                     'sum_of_messages', 'complete', 'measured', 'none', 0, 0)",
            [],
        )
        .expect("synthetic unrecognized client");
    let sessions = store
        .session_summaries(
            &AnalyticsFilter::default(),
            &SessionListOptions::default(),
            50,
            0,
        )
        .expect("sessions");
    assert!(
        sessions.is_empty(),
        "unrecognized clients are skipped, not relabeled as codex"
    );
}

#[test]
fn session_pagination_reaches_beyond_two_hundred_with_stable_ties() {
    let store = UsageStore::open_in_memory().expect("store");
    for index in 0..205 {
        let session = codex_record(
            &format!("session-{index:03}"),
            D0,
            D0,
            None,
            None,
            RawTokenCounts::default(),
        );
        store.upsert_session(&session).expect("fixture");
    }
    let mut claude = claude_record(D0, PROJECT_ALPHA, "fixture");
    claude.client_session_id = "session-100".into();
    store.upsert_session(&claude).expect("cross-client tie");
    let filter = AnalyticsFilter::default();
    let mut ids = std::collections::BTreeSet::new();
    for page in 0..5 {
        let rows = store
            .session_summaries(&filter, &SessionListOptions::default(), 50, page * 50)
            .expect("page");
        assert_eq!(rows.len(), if page == 4 { 6 } else { 50 });
        for row in rows {
            assert!(ids.insert((row.client.as_str(), row.client_session_id)));
        }
    }
    assert_eq!(ids.len(), 206);
    let filtered = store
        .session_summaries(
            &AnalyticsFilter {
                client: Some(Client::Codex),
                ..filter
            },
            &SessionListOptions::default(),
            51,
            200,
        )
        .expect("last codex page");
    assert_eq!(filtered.len(), 5);
    assert!(
        store
            .session_summaries(&filter, &SessionListOptions::default(), 51, 206)
            .expect("end")
            .is_empty()
    );
}
