use super::*;

#[test]
fn schema_rejects_unknown_enum_values() {
    let store = UsageStore::open_in_memory().expect("open store");
    let result = store.conn.execute(
        "INSERT INTO sessions (id, client, client_session_id, semantics_id, total_basis,
             message_coverage, source_kind, cost_source, imported_at_ms, updated_at_ms)
         VALUES ('x', 'codex', 'x', 's', 'nonsense', 'complete', 'measured', 'none', 0, 0)",
        [],
    );
    assert!(
        result.is_err(),
        "CHECK constraint should reject unknown enum"
    );
}

#[test]
fn deleting_a_session_cascades_to_its_messages() {
    let store = UsageStore::open_in_memory().expect("open store");
    let file = source_file(
        Client::ClaudeCode,
        "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
    );
    store
        .import_session(&file, &claude_rollup(1), &[claude_message("msg_01", false)])
        .expect("import");
    assert_eq!(store.message_count().expect("messages"), 1);
    store
        .conn
        .execute("DELETE FROM sessions WHERE client = 'claude_code'", [])
        .expect("delete session");
    assert_eq!(store.message_count().expect("messages"), 0);
}
