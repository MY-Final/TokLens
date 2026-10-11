use super::*;

#[test]
fn claude_duplicate_message_ids_are_written_once() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(
        Client::ClaudeCode,
        "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
    );
    let first = claude_message("msg_01", false);
    let duplicate = claude_message("msg_01", false);
    let sidechain = claude_message("msg_02", true);
    let session = claude_rollup(2);
    let summary = store
        .import_session(&file, &session, &[first, duplicate, sidechain])
        .expect("import");
    assert_eq!(summary.messages_inserted, 2);
    assert_eq!(summary.messages_updated, 1);
    assert_eq!(store.message_count().expect("messages"), 2);

    let session_row = store
        .session_tokens(Client::ClaudeCode, CLAUDE_SESSION)
        .expect("session row")
        .expect("session exists");
    let messages_row = store
        .messages_sum(Client::ClaudeCode, CLAUDE_SESSION)
        .expect("messages sum");
    assert_eq!(messages_row.total_tokens, session_row.total_tokens);
    assert_eq!(session_row.total_tokens, 2 * (1_608 + 113 + 35_200));
    let sidechain_count: i64 = store
        .conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE is_sidechain = 1",
            [],
            |row| row.get(0),
        )
        .expect("sidechain count");
    assert_eq!(sidechain_count, 1);
}

#[test]
fn opencode_session_total_equals_deduped_message_sum() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(
        Client::OpenCode,
        "C:\\Users\\demo\\.local\\share\\opencode\\opencode.db",
    );
    let raw = RawTokenCounts {
        input: Some(288_249),
        output: Some(13_418),
        cache_read: Some(756_224),
        cache_write: Some(0),
        reasoning: Some(1_006),
        client_total: Some(1_058_897),
    };
    let row = OpenCodeSessionRow {
        id: "ses_ee3e1b96",
        parent_id: None,
        directory: Some("D:\\Repos\\example"),
        model: Some("deepseek-flash"),
        created_ms: Some(1_786_371_183_576),
        updated_ms: Some(1_791_473_749_139),
        cost_usd: Some(0.0123),
        usage: raw,
    };
    let message = opencode_message(&OpenCodeMessageRow {
        id: "msg_1",
        session_id: "ses_ee3e1b96",
        model_id: Some("deepseek-flash"),
        provider_id: Some("kuncode"),
        agent: Some("build"),
        created_ms: Some(1_786_371_183_576),
        completed_ms: Some(1_786_371_183_900),
        cost_usd: Some(0.0123),
        usage: raw,
    });
    store
        .import_session(&file, &opencode_session(&row), &[message])
        .expect("import");
    let session_row = store
        .session_tokens(Client::OpenCode, "ses_ee3e1b96")
        .expect("session row")
        .expect("session exists");
    let messages_row = store
        .messages_sum(Client::OpenCode, "ses_ee3e1b96")
        .expect("messages sum");
    assert_eq!(session_row.total_tokens, messages_row.total_tokens);
    assert_eq!(session_row.cache_read_tokens, 756_224);
    assert_eq!(session_row.reasoning_tokens, 1_006);
}

#[test]
fn totals_filter_by_client_and_time_range() {
    let store = UsageStore::open_in_memory().expect("open store");
    let codex_file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
    store
        .import_session(&codex_file, &codex_session(&codex_snapshot()), &[])
        .expect("import codex");
    let claude_file = source_file(
        Client::ClaudeCode,
        "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
    );
    store
        .import_session(&claude_file, &claude_rollup(2), &[])
        .expect("import claude");

    let all = store.totals(&TotalsFilter::default()).expect("totals");
    assert_eq!(all.sessions, 2);
    assert_eq!(all.total_tokens, 4_941_884 + 2 * (1_608 + 113 + 35_200));

    let codex_only = store
        .totals(&TotalsFilter {
            client: Some(Client::Codex),
            ..TotalsFilter::default()
        })
        .expect("totals");
    assert_eq!(codex_only.sessions, 1);
    assert_eq!(codex_only.total_tokens, 4_941_884);

    let codex_window = store
        .totals(&TotalsFilter {
            client: Some(Client::Codex),
            from_ms: Some(1_791_700_000_000),
            to_ms: Some(1_791_705_000_000),
            ..TotalsFilter::default()
        })
        .expect("totals");
    assert_eq!(codex_window.sessions, 1);

    let outside_window = store
        .totals(&TotalsFilter {
            from_ms: Some(1_800_000_000_000),
            ..TotalsFilter::default()
        })
        .expect("totals");
    assert_eq!(outside_window.sessions, 0);
    assert_eq!(outside_window.total_tokens, 0);
}
