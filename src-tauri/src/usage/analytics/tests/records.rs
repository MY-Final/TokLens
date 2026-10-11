use super::*;

#[test]
fn session_summaries_are_newest_first_with_coverage_metadata() {
    let store = seeded_store();
    let sessions = store
        .session_summaries(&AnalyticsFilter::default(), 2, 0)
        .expect("sessions");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].client_session_id, "thread-c");
    assert_eq!(sessions[0].total_tokens, None, "无用量会话保持 NULL");
    assert_eq!(sessions[0].messages, 0);
    assert_eq!(sessions[1].client_session_id, "thread-b");

    let page_two = store
        .session_summaries(&AnalyticsFilter::default(), 2, 2)
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
    let opencode = status
        .iter()
        .find(|entry| entry.client == Client::OpenCode)
        .expect("opencode");
    assert_eq!(opencode.sessions, 0);
    assert_eq!(opencode.files_tracked, 0);

    let (first, last) = store.data_bounds().expect("bounds");
    assert_eq!(first, Some(ms(D0)));
    assert_eq!(last, Some(ms(D2)));
}
