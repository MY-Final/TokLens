use super::*;

#[test]
fn stale_cumulative_snapshot_never_overwrites_newer_total() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
    store
        .import_session(&file, &codex_session(&codex_snapshot()), &[])
        .expect("first import");

    let mut stale = codex_snapshot();
    stale.cumulative = RawTokenCounts {
        input: Some(1_000),
        client_total: Some(1_000),
        ..RawTokenCounts::default()
    };
    stale.ended_at = Some("2026-10-10T00:00:00.000Z");
    let summary = store
        .import_session(&file, &codex_session(&stale), &[])
        .expect("stale import");
    assert_eq!(summary.session, Some(UpsertOutcome::SkippedStale));
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(totals.total_tokens, 4_941_884, "累计值不可回退");
}

#[test]
fn unknown_snapshot_does_not_erase_known_total() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
    store
        .import_session(&file, &codex_session(&codex_snapshot()), &[])
        .expect("first import");

    // 文件刚追加、还没写入任何 token 记录：不得把已知累计值抹成 NULL。
    let mut empty = codex_snapshot();
    empty.cumulative = RawTokenCounts::default();
    empty.message_count = 0;
    empty.ended_at = Some("2026-10-11T09:00:00.000Z");
    let outcome = store
        .upsert_session(&codex_session(&empty))
        .expect("empty snapshot");
    assert_eq!(outcome, UpsertOutcome::SkippedStale);
    let totals = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(totals.total_tokens, 4_941_884);
}

#[test]
fn null_is_distinct_from_zero_and_missing_total_stays_unknown() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(
        Client::ClaudeCode,
        "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
    );
    let session = claude_code_session(&ClaudeSessionRollup {
        session_id: "s-missing",
        cwd: None,
        model: None,
        started_at: None,
        ended_at: None,
        message_count: 0,
        usage: RawTokenCounts {
            input: Some(10),
            output: None,
            cache_read: None,
            cache_write: None,
            reasoning: None,
            client_total: None,
        },
    });
    store
        .import_session(&file, &session, &[])
        .expect("import session");
    let (cache_read, total): (Option<i64>, Option<i64>) = store
        .conn
        .query_row(
            "SELECT cache_read_tokens, total_tokens FROM sessions WHERE client_session_id = 's-missing'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read row");
    assert_eq!(cache_read, None, "缺失字段必须写 NULL，不能写 0");
    assert_eq!(total, None, "缺失字段导致总量未知");
    assert_eq!(
        store
            .session_unknown_relations(Client::ClaudeCode, "s-missing")
            .expect("unknown relations"),
        vec!["total.unavailable_due_to_missing_fields".to_string()]
    );
}

#[test]
fn unknown_relations_are_persisted_for_review() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
    store
        .import_session(&file, &codex_session(&codex_snapshot()), &[])
        .expect("import");
    let relations = store
        .session_unknown_relations(Client::Codex, CODEX_THREAD)
        .expect("relations");
    assert_eq!(relations, vec![UNKNOWN_CACHE_WRITE_RELATION.to_string()]);
}
