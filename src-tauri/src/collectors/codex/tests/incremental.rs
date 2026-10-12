use super::*;

#[test]
fn ordinal_free_cumulative_identity_survives_append_rescan_and_copied_files() {
    let dir = temp_dir("ordinal-free-append");
    let at = "2026-10-12T01:04:00.000Z";
    let usage = counts(100, 10, 20, 0, 2);
    let meta = without_ordinal(session_meta_at(THREAD, "2026-10-12T01:03:00.000Z", None, 0));
    let first = without_ordinal(legacy_token_count(
        None,
        at,
        1,
        usage.clone(),
        usage.clone(),
    ));
    let path = write_file(&dir, "rollout-first.jsonl", &[meta.clone(), first.clone()]);
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);
    // 状态重复通知发生在下一天，不能将昨天请求的时间搬到今天。
    append_lines(
        &path,
        &[
            without_ordinal(legacy_token_count(
                None,
                "2026-10-13T00:00:00.000Z",
                2,
                usage.clone(),
                usage.clone(),
            )),
            // 两次请求可以有相同增量；累计值增长时仍需保留第二次请求。
            without_ordinal(legacy_token_count(
                None,
                "2026-10-13T00:01:00.000Z",
                3,
                usage.clone(),
                counts(200, 20, 40, 0, 4),
            )),
        ],
    );
    sync(&store, &dir);
    assert_eq!(store.message_count().expect("messages"), 2);
    assert_eq!(codex_totals(&store).total_tokens, 220);
    let times: Vec<i64> = store
        .connection_for_tests()
        .prepare("SELECT created_at_ms FROM messages ORDER BY created_at_ms")
        .expect("query")
        .query_map([], |row| row.get(0))
        .expect("times")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(
        times,
        vec![
            crate::usage::time::parse_iso8601_utc_ms(at).expect("timestamp"),
            crate::usage::time::parse_iso8601_utc_ms("2026-10-13T00:01:00.000Z")
                .expect("timestamp"),
        ]
    );
    write_file(&dir, "rollout-copy.jsonl", &[meta, first]);
    sync(&store, &dir);
    assert_eq!(store.message_count().expect("messages"), 2);
    assert_eq!(sync(&store, &dir).files_parsed, 0);
    CodexCollector::new(&dir)
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("force");
    assert_eq!(store.message_count().expect("messages"), 2);
    assert_eq!(codex_totals(&store).total_tokens, 220);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_cumulative_identity_uses_file_and_physical_line_without_collisions() {
    let dir = temp_dir("ordinal-free-file-line");
    let mut request = without_ordinal(usage_record(
        THREAD,
        "2026-10-12T01:04:00.000Z",
        1,
        "",
        counts(100, 10, 0, 0, 0),
        Value::Null,
    ));
    request.as_object_mut().expect("object").remove("timestamp");
    let meta = without_ordinal(session_meta_at(THREAD, "2026-10-12T01:03:00.000Z", None, 0));
    let path = write_file(&dir, "rollout-one.jsonl", &[meta.clone(), request.clone()]);
    write_file(&dir, "rollout-two.jsonl", &[meta, request.clone()]);
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);
    assert_eq!(store.message_count().expect("messages"), 2);
    append_lines(&path, &[Value::Null, request]);
    sync(&store, &dir);
    assert_eq!(store.message_count().expect("messages"), 3);
    CodexCollector::new(&dir)
        .sync(&store, &SyncOptions { force_rescan: true })
        .expect("force");
    assert_eq!(store.message_count().expect("messages"), 3);
    let invalid_sequence: i64 = store
        .connection_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE seq IS NOT NULL OR created_at_ms IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("seq");
    assert_eq!(invalid_sequence, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parser_upgrade_reimports_previously_skipped_usage_without_file_changes() {
    let dir = temp_dir("parser-upgrade");
    let path = write_file(
        &dir,
        "rollout-upgrade.jsonl",
        &[
            without_ordinal(session_meta_at(THREAD, "2026-10-12T01:03:00.000Z", None, 0)),
            without_ordinal(legacy_token_count(
                None,
                "2026-10-12T01:04:00.000Z",
                1,
                counts(100, 10, 0, 0, 0),
                counts(100, 10, 0, 0, 0),
            )),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);
    // 模拟旧解析器只记下会话与水位，未导入任何请求。
    store
        .connection_for_tests()
        .execute("DELETE FROM messages", [])
        .expect("clear synthetic messages");
    store
        .connection_for_tests()
        .execute("UPDATE source_files SET parser_version = 1", [])
        .expect("old version");
    let report = sync(&store, &dir);
    assert_eq!(reason_count(&report, ScanReason::ParserVersionChanged), 1);
    assert_eq!(store.message_count().expect("messages"), 1);
    let watermark = store
        .source_file(Client::Codex, &path.to_string_lossy())
        .expect("watermark")
        .expect("exists");
    assert_eq!(watermark.parser_version, PARSER_VERSION);
    assert_eq!(sync(&store, &dir).files_parsed, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

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

#[test]
fn unfinished_tail_preserves_watermark_and_recovers_when_completed() {
    let dir = temp_dir("unfinished-tail");
    let first = usage_record(
        THREAD,
        "2026-10-11T12:01:00.000Z",
        1,
        "first",
        counts(10, 2, 0, 0, 0),
        counts(10, 2, 0, 0, 0),
    );
    let path = write_file(
        &dir,
        "rollout-tail.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 0),
            first,
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);
    let original = std::fs::read_to_string(&path).expect("fixture");
    let before = store
        .source_file(Client::Codex, &path.to_string_lossy())
        .expect("source")
        .expect("watermark");
    let second = usage_record(
        THREAD,
        "2026-10-11T12:02:00.000Z",
        2,
        "second",
        counts(20, 3, 0, 0, 0),
        counts(30, 5, 0, 0, 0),
    )
    .to_string();
    std::fs::write(&path, format!("{original}{}", &second[..second.len() / 2]))
        .expect("partial append");
    assert_eq!(sync(&store, &dir).files_failed, 1);
    assert_eq!(
        sync(&store, &dir).files_failed,
        1,
        "failed files are retried even unchanged"
    );
    let after = store
        .source_file(Client::Codex, &path.to_string_lossy())
        .expect("source")
        .expect("watermark");
    assert_eq!(before.content_offset, after.content_offset);
    assert_eq!(before.last_scan_ms, after.last_scan_ms);
    assert_eq!(store.message_count().expect("messages"), 1);
    assert_eq!(codex_totals(&store).total_tokens, 12);
    // A valid final record is accepted even without a trailing newline.
    std::fs::write(&path, format!("{original}{second}")).expect("complete append");
    assert_eq!(sync(&store, &dir).files_failed, 0);
    assert_eq!(store.message_count().expect("messages"), 2);
    assert_eq!(codex_totals(&store).total_tokens, 35);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn interrupted_reader_never_returns_a_successful_partial_snapshot() {
    struct FailAfterPrefix(std::io::Cursor<Vec<u8>>);
    impl std::io::Read for FailAfterPrefix {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            use std::io::Read;
            if self.0.position() as usize >= self.0.get_ref().len() {
                return Err(std::io::Error::other("synthetic interruption"));
            }
            Read::read(&mut self.0, buf)
        }
    }
    let prefix = lines_to_text(&[session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 0)]);
    let reader =
        std::io::BufReader::new(FailAfterPrefix(std::io::Cursor::new(prefix.into_bytes())));
    assert!(
        super::super::parser::parse_reader(reader, Path::new("synthetic.jsonl"), 0, 0).is_err()
    );
}

#[test]
fn changed_file_fingerprint_rejects_parsed_snapshot() {
    let dir = temp_dir("fingerprint");
    let path = write_file(
        &dir,
        "rollout-changing.jsonl",
        &[session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 0)],
    );
    let metadata = std::fs::metadata(&path).expect("metadata");
    let parsed = super::super::parser::parse_file(
        &path,
        metadata.len() as i64,
        super::super::discovery::mtime_ms(&metadata),
    )
    .expect("parse");
    append_lines(&path, &[json!({"type": "unknown"})]);
    assert!(!super::super::parser::fingerprint_matches(&parsed));
    let _ = std::fs::remove_dir_all(&dir);
}
