//! Shared synthetic fixtures for behavior-specific regression tests.
use super::*;
use crate::usage::client::Client;
use crate::usage::model::{MessageRecord, SessionRecord, parse_iso8601_utc_ms};
use crate::usage::normalize::{
    ClaudeSessionRollup, CodexRecordIdentity, CodexThreadSnapshot, CodexUsageRecord,
    claude_code_session, codex_message, codex_session,
};
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::tokens::RawTokenCounts;

const D0: &str = "2026-10-05T00:00:00.000Z";
const D1: &str = "2026-10-06T09:00:00.000Z";
const D2: &str = "2026-10-07T20:00:00.000Z";
const PROJECT_ALPHA: &str = "D:\\Repos\\alpha";
const PROJECT_BETA: &str = "D:\\Repos\\beta";

fn ms(value: &str) -> i64 {
    parse_iso8601_utc_ms(value).expect("test timestamp")
}

fn counts(
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    reasoning: Option<u64>,
    client_total: Option<u64>,
) -> RawTokenCounts {
    RawTokenCounts {
        input,
        output,
        cache_read,
        cache_write,
        reasoning,
        client_total,
    }
}

fn codex_record(
    thread: &str,
    started: &str,
    ended: &str,
    model: Option<&str>,
    project: Option<&str>,
    cumulative: RawTokenCounts,
) -> SessionRecord {
    codex_session(&CodexThreadSnapshot {
        thread_id: thread,
        parent_thread_id: None,
        cwd: project,
        model,
        started_at: Some(started),
        ended_at: Some(ended),
        first_ordinal: Some(0),
        message_count: 1,
        cumulative,
    })
}

fn claude_record(started: &str, project: &str, model: &str) -> SessionRecord {
    claude_code_session(&ClaudeSessionRollup {
        session_id: "session-d",
        cwd: Some(project),
        model: Some(model),
        started_at: Some(started),
        ended_at: Some(started),
        message_count: 1,
        usage: counts(Some(500), Some(50), Some(100), Some(0), None, None),
    })
}

fn message(thread: &str, response: &str, ordinal: i64, at: &str) -> MessageRecord {
    codex_message(&CodexUsageRecord {
        thread_id: thread,
        response_id: Some(response),
        identity: CodexRecordIdentity::Ordinal(ordinal),
        timestamp: Some(at),
        usage: counts(Some(10), Some(2), Some(0), Some(0), Some(0), Some(12)),
    })
}

fn source(client: Client, path: &str) -> SourceFile {
    SourceFile {
        client,
        path: path.to_string(),
        kind: SourceFileKind::Jsonl,
        size_bytes: Some(1_024),
        mtime_ms: Some(1_791_000_000_000),
        content_offset: 1_024,
        parser_version: 1,
        last_scan_ms: Some(1_791_000_000_500),
        unparsed_records: 0,
    }
}

/// 4 个会话：3 个 Codex（跨 3 天，其中一个完全没有用量数据）+ 1 个 Claude Code。
fn seeded_store() -> UsageStore {
    let store = UsageStore::open_in_memory().expect("store");
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\a.jsonl"),
            &codex_record(
                "thread-a",
                D0,
                D0,
                Some("gpt-5.6-luna"),
                Some(PROJECT_ALPHA),
                counts(
                    Some(1_000),
                    Some(100),
                    Some(400),
                    Some(5),
                    Some(20),
                    Some(1_100),
                ),
            ),
            &[
                message("thread-a", "resp-a1", 1, D0),
                message("thread-a", "resp-a2", 2, D0),
            ],
        )
        .expect("import a");
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\b.jsonl"),
            &codex_record(
                "thread-b",
                D1,
                D1,
                Some("gpt-6-astra"),
                Some(PROJECT_BETA),
                counts(
                    Some(2_000),
                    Some(300),
                    Some(0),
                    Some(0),
                    Some(0),
                    Some(2_300),
                ),
            ),
            &[message("thread-b", "resp-b1", 1, D1)],
        )
        .expect("import b");
    store
        .import_session(
            &source(Client::Codex, "C:\\sessions\\c.jsonl"),
            &codex_record("thread-c", D2, D2, None, None, RawTokenCounts::default()),
            &[],
        )
        .expect("import c");
    store
        .import_session(
            &source(Client::ClaudeCode, "C:\\claude\\d.jsonl"),
            &claude_record(D1, PROJECT_ALPHA, "claude-sonnet-5"),
            &[],
        )
        .expect("import d");
    store
}

mod aggregates;
mod rankings;
mod records;
