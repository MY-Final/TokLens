use super::*;

#[test]
fn rescan_skips_unchanged_files_and_is_idempotent() {
    let dir = temp_dir("rescan");
    write_file(
        &dir,
        "rollout-2026-10-11T10-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T10:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T10:01:00.000Z",
                1,
                "resp_1",
                counts(1_000, 100, 400, 0, 20),
                counts(1_000, 100, 400, 0, 20),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let first = sync(&store, &dir);
    assert_eq!(first.files_parsed, 1);
    assert_eq!(first.messages_inserted, 1);

    let second = sync(&store, &dir);
    assert_eq!(second.files_parsed, 0, "未变化的文件不应重新解析");
    assert_eq!(second.files_skipped_unchanged, 1);
    assert_eq!(reason_count(&second, ScanReason::Unchanged), 1);
    assert_eq!(store.message_count().expect("messages"), 1);
    assert_eq!(codex_totals(&store).total_tokens, 1_100);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn appended_file_imports_new_messages_without_duplicating() {
    let dir = temp_dir("append");
    let path = write_file(
        &dir,
        "rollout-2026-10-11T11-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T11:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T11:01:00.000Z",
                1,
                "resp_1",
                counts(1_000, 100, 400, 0, 20),
                counts(1_000, 100, 400, 0, 20),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);

    append_lines(
        &path,
        &[usage_record(
            THREAD,
            "2026-10-11T11:05:00.000Z",
            2,
            "resp_2",
            counts(900, 50, 0, 0, 10),
            counts(1_900, 150, 400, 0, 30),
        )],
    );
    let report = sync(&store, &dir);

    assert_eq!(report.files_parsed, 1);
    assert_eq!(reason_count(&report, ScanReason::Appended), 1);
    assert_eq!(store.message_count().expect("messages"), 2);
    assert_eq!(report.messages_pruned, 1, "覆盖式导入先清理该文件旧明细");
    assert_eq!(codex_totals(&store).total_tokens, 1_900 + 150);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn truncated_file_is_reparsed_and_detail_is_rebuilt() {
    let dir = temp_dir("truncate");
    let path = write_file(
        &dir,
        "rollout-2026-10-11T12-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T12:01:00.000Z",
                1,
                "resp_1",
                counts(1_000, 100, 0, 0, 0),
                counts(1_000, 100, 0, 0, 0),
            ),
            usage_record(
                THREAD,
                "2026-10-11T12:02:00.000Z",
                2,
                "resp_2",
                counts(2_000, 0, 0, 0, 0),
                counts(3_000, 100, 0, 0, 0),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);
    assert_eq!(store.message_count().expect("messages"), 2);

    // 文件被截断/替换：内容变短、ordinal 起点改变。
    std::fs::write(
        &path,
        lines_to_text(&[
            session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 807),
            usage_record(
                THREAD,
                "2026-10-11T12:01:00.000Z",
                808,
                "resp_1",
                counts(1_000, 100, 0, 0, 0),
                counts(1_000, 100, 0, 0, 0),
            ),
        ]),
    )
    .expect("truncate synthetic file");
    let report = sync(&store, &dir);

    assert_eq!(reason_count(&report, ScanReason::TruncatedOrReplaced), 1);
    // 明细按新内容重建（旧的两条已不存在）。
    assert_eq!(report.messages_pruned, 2);
    assert_eq!(store.message_count().expect("messages"), 1);
    // 累计值不回退：曾观测到的 3_100 仍是该线程的已知累计值，且陈旧快照整体被忽略
    // （会话行的口径、完整度、时间与总量一并保持上一次的值，不做部分更新）。
    assert_eq!(codex_totals(&store).total_tokens, 3_100);
    assert_eq!(coverage(&store, THREAD), "complete");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_response_ids_across_files_land_once() {
    let dir = temp_dir("duplicate-response");
    let usage = counts(1_000, 100, 400, 0, 20);
    write_file(
        &dir,
        "rollout-2026-10-11T13-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T13:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T13:01:00.000Z",
                1,
                "resp_X",
                usage.clone(),
                usage.clone(),
            ),
        ],
    );
    write_file(
        &dir,
        "rollout-2026-10-11T13-10-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T13:10:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T13:01:00.000Z",
                1,
                "resp_X",
                usage.clone(),
                usage,
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.files_seen, 2);
    assert_eq!(store.message_count().expect("messages"), 1);
    assert_eq!(codex_totals(&store).total_tokens, 1_100);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn force_rescan_reparses_unchanged_files() {
    let dir = temp_dir("force");
    write_file(
        &dir,
        "rollout-2026-10-11T14-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T14:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T14:01:00.000Z",
                1,
                "resp_1",
                counts(1_000, 100, 0, 0, 0),
                counts(1_000, 100, 0, 0, 0),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);

    let report = CodexCollector::new(&dir)
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("forced sync");
    assert_eq!(report.files_parsed, 1);
    assert_eq!(reason_count(&report, ScanReason::Forced), 1);
    assert_eq!(store.message_count().expect("messages"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
