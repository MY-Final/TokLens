use super::*;

#[test]
fn missing_root_is_not_an_error() {
    let dir = temp_dir("missing-root");
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir.join("does-not-exist"));

    assert!(report.root_missing);
    assert_eq!(report.files_seen, 0);
    assert_eq!(store.session_count().expect("sessions"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_lines_are_counted_and_do_not_abort_the_file() {
    let dir = temp_dir("invalid");
    let path = dir.join("rollout-2026-10-11T15-00-00-01a129d9.jsonl");
    let mut text = lines_to_text(&[
        session_meta_at(THREAD, "2026-10-11T15:00:00.000Z", None, 0),
        usage_record(
            THREAD,
            "2026-10-11T15:01:00.000Z",
            1,
            "resp_1",
            counts(1_000, 100, 0, 0, 0),
            counts(1_000, 100, 0, 0, 0),
        ),
    ]);
    text.push_str("not-json-at-all\n{\"type\": \"token_usage_record\", \"payload\": {\n");
    std::fs::write(&path, text).expect("write file with invalid lines");

    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.unparsed_lines, 2);
    assert_eq!(report.messages_inserted, 1, "合法记录仍应导入");
    let watermark = store
        .source_file(Client::Codex, &path.to_string_lossy())
        .expect("watermark")
        .expect("exists");
    assert_eq!(watermark.unparsed_records, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_without_identity_is_not_imported_and_keeps_no_watermark() {
    let dir = temp_dir("no-identity");
    // 既没有 session_meta，记录里也没有 thread_id：无法归属，不写库、不推进水位。
    let path = write_file(
        &dir,
        "rollout-2026-10-11T16-00-00-unknown.jsonl",
        &[legacy_token_count(
            None,
            "2026-10-11T16:00:00.000Z",
            1,
            counts(100, 10, 0, 0, 0),
            counts(100, 10, 0, 0, 0),
        )],
    );
    let store = UsageStore::open_in_memory().expect("store");
    let report = sync(&store, &dir);

    assert_eq!(report.files_without_identity, 1);
    assert_eq!(report.sessions_imported, 0);
    assert_eq!(store.session_count().expect("sessions"), 0);
    assert!(
        store
            .source_file(Client::Codex, &path.to_string_lossy())
            .expect("watermark")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn privacy_sentinel_content_never_reaches_the_database() {
    let dir = temp_dir("privacy");
    let meta = session_meta_at(THREAD, "2026-10-11T17:00:00.000Z", None, 0);
    let mut meta = meta;
    meta["payload"]["base_instructions"]["text"] = json!(SENTINEL);
    write_file(
        &dir,
        "rollout-2026-10-11T17-00-00-01a129d9.jsonl",
        &[
            meta,
            // 正文/提示词/工具输出行：解析器不读取这些字段。
            json!({ "timestamp": "2026-10-11T17:00:30.000Z", "ordinal": 1, "type": "response_item",
                    "payload": { "type": "message", "role": "user",
                                 "content": [{ "type": "input_text", "text": SENTINEL }] } }),
            json!({ "timestamp": "2026-10-11T17:01:00.000Z", "ordinal": 2, "type": "event_msg",
                    "payload": { "type": "item_completed",
                                 "item": { "type": "CommandExecution", "aggregated_output": SENTINEL } } }),
            usage_record(
                THREAD,
                "2026-10-11T17:02:00.000Z",
                3,
                "resp_1",
                counts(1_000, 100, 400, 0, 20),
                counts(1_000, 100, 400, 0, 20),
            ),
        ],
    );
    let store = UsageStore::open_in_memory().expect("store");
    sync(&store, &dir);

    let connection = store.connection_for_tests();
    let text_columns = [
        ("sessions", "id"),
        ("sessions", "client_session_id"),
        ("sessions", "parent_client_session_id"),
        ("sessions", "project_path"),
        ("sessions", "model"),
        ("sessions", "semantics_id"),
        ("sessions", "unknown_relations"),
        ("messages", "id"),
        ("messages", "dedup_key"),
        ("messages", "model"),
        ("messages", "agent"),
        ("messages", "source_ref"),
        ("source_files", "path"),
    ];
    let mut leaked = Vec::new();
    for (table, column) in text_columns {
        let sql = format!("SELECT {column} FROM {table}");
        let mut statement = connection.prepare(&sql).expect("prepare");
        let rows = statement
            .query_map([], |row| row.get::<_, Option<String>>(0))
            .expect("query");
        for value in rows.flatten().flatten() {
            if value.contains(SENTINEL) {
                leaked.push(format!("{table}.{column}"));
            }
        }
    }
    assert!(leaked.is_empty(), "敏感内容进入数据库：{leaked:?}");
    assert_eq!(store.message_count().expect("messages"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
