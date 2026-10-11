use super::*;

#[test]
fn imports_new_token_format_with_codex_semantics() {
    let dir = temp_dir("new-format");
    write_file(
        &dir,
        "rollout-2026-10-11T07-24-36-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T07:24:36.001Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T07:25:00.000Z",
                1,
                "resp_1",
                counts(90_065, 161, 13_568, 0, 56),
                counts(90_065, 161, 13_568, 0, 56),
            ),
            usage_record(
                THREAD,
                "2026-10-11T07:26:00.000Z",
                2,
                "resp_2",
                counts(4_851_819, 0, 4_078_208, 0, 40),
                counts(4_941_884, 161, 4_091_776, 0, 96),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.files_seen, 1);
    assert_eq!(report.files_parsed, 1);
    assert_eq!(report.sessions_imported, 1);
    assert_eq!(report.messages_inserted, 2);
    assert_eq!(report.unparsed_lines, 0);

    let totals = codex_totals(&store);
    assert_eq!(totals.sessions, 1);
    assert_eq!(totals.messages, 2);
    // 会话总量取线程累计值，而不是两条请求之和。
    assert_eq!(totals.total_tokens, 4_941_884 + 161);
    // 子集字段已扣除：input 不含 cached，output 不含 reasoning。
    let session = store
        .session_tokens(Client::Codex, THREAD)
        .expect("session")
        .expect("exists");
    assert_eq!(session.input_tokens, 4_941_884 - 4_091_776);
    assert_eq!(session.output_tokens, 161 - 96);
    assert_eq!(session.reasoning_tokens, 96);
    assert_eq!(session.cache_read_tokens, 4_091_776);
    assert_eq!(
        store
            .session_unknown_relations(Client::Codex, THREAD)
            .expect("unknown"),
        vec!["cache_write.relation_to_input".to_string()]
    );
    assert_eq!(coverage(&store, THREAD), "complete");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn imports_legacy_token_count_only_files() {
    let dir = temp_dir("legacy");
    write_file(
        &dir,
        "rollout-2026-07-09T23-37-38-019f4786.jsonl",
        &[
            session_meta_at(THREAD, "2026-07-09T23:37:38.000Z", None, 0),
            legacy_token_count(
                None,
                "2026-07-09T23:38:00.000Z",
                1,
                counts(13_000, 100, 0, 0, 10),
                counts(13_000, 100, 0, 0, 10),
            ),
            legacy_token_count(
                None,
                "2026-07-09T23:39:00.000Z",
                2,
                counts(528_000, 168, 512_000, 0, 68),
                counts(541_000, 268, 512_000, 0, 78),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.messages_inserted, 2);
    let totals = codex_totals(&store);
    // 旧格式：last_token_usage 是每请求增量，total_token_usage 是线程累计。
    assert_eq!(totals.total_tokens, 541_000 + 268);
    let session = store
        .session_tokens(Client::Codex, THREAD)
        .expect("session")
        .expect("exists");
    assert_eq!(session.input_tokens, 541_000 - 512_000);
    assert_eq!(session.reasoning_tokens, 78);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mixed_format_keeps_legacy_prefix_and_takes_max_cumulative() {
    // 真实场景（rollout-2026-10-06T00-38-35-…）：文件前半段只有旧格式历史，
    // 后半段是新格式记录 + 每条旧格式镜像。镜像必须丢弃，但旧格式前缀要保留。
    let dir = temp_dir("mixed-prefix");
    write_file(
        &dir,
        "rollout-2026-10-06T00-38-35-01a10cee.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-06T00:38:35.000Z", None, 11),
            legacy_token_count(
                None,
                "2026-10-06T00:39:00.000Z",
                12,
                counts(12_000, 180, 0, 0, 0),
                counts(12_000, 180, 0, 0, 0),
            ),
            legacy_token_count(
                None,
                "2026-10-06T00:40:00.000Z",
                13,
                counts(12_600, 206, 0, 0, 0),
                counts(24_600, 386, 0, 0, 0),
            ),
            usage_record(
                THREAD,
                "2026-10-06T00:41:00.000Z",
                20,
                "resp_1",
                counts(44_000, 698, 0, 0, 0),
                counts(44_000, 698, 0, 0, 0),
            ),
            // 新格式记录的镜像：值相同，必须丢弃。
            legacy_token_count(
                None,
                "2026-10-06T00:41:00.000Z",
                25,
                counts(44_000, 698, 0, 0, 0),
                counts(32_000_000, 253_468, 0, 0, 0),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    // 明细 = 2 条旧格式前缀 + 1 条新格式记录（镜像已丢弃）。
    assert_eq!(report.messages_inserted, 3);
    assert_eq!(store.message_count().expect("messages"), 3);
    // 线程累计 = 两个计数器的最大值（旧格式链覆盖更早的历史），绝不是相加。
    assert_eq!(codex_totals(&store).total_tokens, 32_000_000 + 253_468);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mixed_format_files_use_new_records_only() {
    let dir = temp_dir("mixed");
    let usage = counts(1_000, 100, 400, 0, 20);
    write_file(
        &dir,
        "rollout-2026-10-11T08-00-00-01a129d9.jsonl",
        &[
            session_meta_at(THREAD, "2026-10-11T08:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T08:01:00.000Z",
                1,
                "resp_1",
                usage.clone(),
                usage.clone(),
            ),
            // 同一条请求的旧格式镜像记录：必须被忽略，不能重复计数。
            legacy_token_count(None, "2026-10-11T08:01:00.000Z", 1, usage.clone(), usage),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.messages_inserted, 1, "旧格式镜像记录必须被忽略");
    assert_eq!(codex_totals(&store).total_tokens, 1_100);
    let _ = std::fs::remove_dir_all(&dir);
}
