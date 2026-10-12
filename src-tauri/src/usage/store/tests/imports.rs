use super::*;

#[test]
fn import_is_idempotent_and_watermarks_persist_across_restart() {
    let path = temp_db_path("restart");
    let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
    let session = codex_session(&codex_snapshot());
    let messages = [codex_message(&CodexUsageRecord {
        thread_id: CODEX_THREAD,
        response_id: Some("resp_1"),
        identity: crate::usage::normalize::CodexRecordIdentity::Ordinal(0),
        timestamp: Some("2026-10-11T07:24:36.001Z"),
        usage: RawTokenCounts {
            input: Some(90_065),
            output: Some(161),
            cache_read: Some(13_568),
            cache_write: Some(0),
            reasoning: Some(56),
            client_total: Some(90_226),
        },
    })];
    {
        let store = UsageStore::open(&path).expect("open store");
        let summary = store
            .import_session(&file, &session, &messages)
            .expect("import");
        assert_eq!(summary.session, Some(UpsertOutcome::Inserted));
        assert_eq!(summary.messages_inserted, 1);
        assert_eq!(store.session_count().expect("sessions"), 1);
        assert_eq!(store.message_count().expect("messages"), 1);
    }
    // 重启后：水位仍在，重复导入不新增行。
    {
        let store = UsageStore::open(&path).expect("reopen store");
        assert_eq!(store.schema_version().expect("version"), 1);
        let watermark = store
            .source_file(Client::Codex, &file.path)
            .expect("read watermark")
            .expect("watermark exists");
        assert_eq!(watermark.content_offset, 12_345);
        assert_eq!(watermark.parser_version, 1);
        let second = store
            .import_session(&file, &session, &messages)
            .expect("re-import");
        assert_eq!(second.session, Some(UpsertOutcome::Updated));
        assert_eq!(second.messages_inserted, 0);
        assert_eq!(second.messages_updated, 1);
        assert_eq!(store.session_count().expect("sessions"), 1);
        assert_eq!(store.message_count().expect("messages"), 1);
        let totals = store.totals(&TotalsFilter::default()).expect("totals");
        assert_eq!(totals.sessions, 1);
        assert_eq!(totals.messages, 1);
        assert_eq!(totals.total_tokens, 4_941_884);
    }
    remove_db(&path);
}

#[test]
fn replace_file_import_prunes_messages_of_that_file() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(
        Client::ClaudeCode,
        "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
    );
    let session = claude_rollup(2);
    let first = store
        .replace_file_import(
            &file,
            &session,
            &[
                claude_message("msg_01", false),
                claude_message("msg_02", false),
            ],
        )
        .expect("first import");
    assert_eq!(first.messages_inserted, 2);
    assert_eq!(first.messages_pruned, 0);

    // 文件被截断后重新解析：只剩一条消息，旧明细必须被清理。
    let second = store
        .replace_file_import(&file, &session, &[claude_message("msg_01", false)])
        .expect("second import");
    assert_eq!(second.messages_pruned, 2);
    assert_eq!(second.messages_inserted, 1);
    assert_eq!(store.message_count().expect("messages"), 1);
}

#[test]
fn import_rejects_mismatched_session_and_file() {
    let store = UsageStore::open_in_memory().expect("open store");
    let codex_file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
    let claude = claude_rollup(1);
    let error = store
        .import_session(&codex_file, &claude, &[])
        .expect_err("mismatched client must fail");
    assert!(matches!(error, UsageError::InvalidInput(_)));
}
