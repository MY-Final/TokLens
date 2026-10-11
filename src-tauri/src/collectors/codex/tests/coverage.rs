use super::*;

#[test]
fn thread_cumulative_is_not_summed_across_continuation_files() {
    let dir = temp_dir("continuation");
    write_file(
        &dir,
        "rollout-2026-10-11T07-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T07:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T07:01:00.000Z",
                1,
                "resp_1",
                counts(1_000, 100, 400, 0, 20),
                counts(1_000, 100, 400, 0, 20),
            ),
        ],
    );
    write_file(
        &dir,
        "rollout-2026-10-11T07-30-00-01a129d9_01a129d8.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T07:30:00.000Z", Some(PARENT), 807),
            usage_record(
                THREAD,
                "2026-10-11T07:31:00.000Z",
                807,
                "resp_2",
                counts(900, 50, 0, 0, 10),
                counts(1_900, 150, 400, 0, 30),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.files_seen, 2);
    assert_eq!(report.messages_inserted, 2);
    let totals = codex_totals(&store);
    assert_eq!(totals.sessions, 1, "同一 thread 只应有一个会话");
    // 取最新累计值 1_950，而不是把两份累计值相加（2_950）。
    assert_eq!(totals.total_tokens, 1_900 + 150);
    let parent: Option<String> = store
        .connection_for_tests()
        .query_row(
            "SELECT parent_client_session_id FROM sessions WHERE client_session_id = ?1",
            rusqlite::params![THREAD],
            |row| row.get(0),
        )
        .expect("parent");
    assert_eq!(parent.as_deref(), Some(PARENT));
    // 该线程有一个文件从 ordinal 0 开始（另一个是它的续接），合起来覆盖完整历史。
    assert_eq!(coverage(&store, THREAD), "complete");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_without_token_records_still_imports_a_session() {
    let dir = temp_dir("no-usage");
    write_file(
        &dir,
        "rollout-2026-10-11T09-00-00-01a1aaaa.jsonl",
        &[session_meta_at(
            "01a1aaaa-0000-0000-0000-000000000000",
            "2026-10-11T09:00:00.000Z",
            None,
            0,
        )],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.sessions_imported, 1);
    assert_eq!(report.messages_inserted, 0);
    let totals = codex_totals(&store);
    assert_eq!(totals.sessions, 1);
    assert_eq!(totals.total_tokens, 0, "没有用量时总量为 NULL，聚合为 0");
    assert_eq!(
        coverage(&store, "01a1aaaa-0000-0000-0000-000000000000"),
        "none"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paginated_only_thread_is_marked_partial() {
    let dir = temp_dir("paginated");
    write_file(
        &dir,
        "rollout-2026-09-12T01-28-46-01a09172.jsonl",
        &[
            // 分页文件：历史被裁掉，ordinal 从 807 开始。
            session_meta_at(THREAD, "2026-09-12T01:28:46.000Z", None, 807),
            usage_record(
                THREAD,
                "2026-09-12T01:29:00.000Z",
                808,
                "resp_1",
                counts(100, 10, 0, 0, 0),
                counts(1_907_000, 268, 0, 0, 0),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.messages_inserted, 1);
    assert_eq!(coverage(&store, THREAD), "partial");
    // 明细不完整，但会话总量仍取客户端累计值。
    assert_eq!(codex_totals(&store).total_tokens, 1_907_000 + 268);
    let _ = std::fs::remove_dir_all(&dir);
}
