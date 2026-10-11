//! Shared synthetic fixtures for behavior-specific regression tests.
use super::*;
use crate::usage::model::{MessageRecord, SessionRecord};
use crate::usage::normalize::{
    ClaudeMessage, ClaudeSessionRollup, CodexThreadSnapshot, CodexUsageRecord, OpenCodeMessageRow,
    OpenCodeSessionRow, claude_code_message, claude_code_session, codex_message, codex_session,
    opencode_message, opencode_session,
};
use crate::usage::tokens::{Client, RawTokenCounts, UNKNOWN_CACHE_WRITE_RELATION};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
const CODEX_THREAD: &str = "01a129d9-6720-7020-9402-a0a5930c37e6";
const CLAUDE_SESSION: &str = "cfbc9166-dbf9-46d9-bbda-e2ef59b32a3c";

fn temp_db_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "toklens-usage-{}-{}-{}.sqlite3",
        tag,
        std::process::id(),
        counter
    ))
}

fn remove_db(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
    }
}

fn source_file(client: Client, path: &str) -> SourceFile {
    SourceFile {
        client,
        path: path.to_string(),
        kind: SourceFileKind::Jsonl,
        size_bytes: Some(4_200_000),
        mtime_ms: Some(1_791_704_287_676),
        content_offset: 12_345,
        parser_version: 1,
        last_scan_ms: Some(1_791_704_287_900),
        unparsed_records: 0,
    }
}

fn codex_cumulative() -> RawTokenCounts {
    RawTokenCounts {
        input: Some(4_941_884),
        output: Some(0),
        cache_read: Some(4_091_776),
        cache_write: Some(0),
        reasoning: Some(0),
        client_total: Some(4_941_884),
    }
}

fn codex_snapshot() -> CodexThreadSnapshot<'static> {
    CodexThreadSnapshot {
        thread_id: CODEX_THREAD,
        parent_thread_id: None,
        cwd: Some("D:\\WorkSpace\\TokLens"),
        model: Some("gpt-5.6-luna"),
        started_at: Some("2026-10-11T07:24:36.000Z"),
        ended_at: Some("2026-10-11T07:37:14.576Z"),
        first_ordinal: Some(0),
        message_count: 61,
        cumulative: codex_cumulative(),
    }
}

fn claude_raw() -> RawTokenCounts {
    RawTokenCounts {
        input: Some(1_608),
        output: Some(113),
        cache_read: Some(35_200),
        cache_write: Some(0),
        reasoning: None,
        client_total: None,
    }
}

fn claude_message(id: &str, is_sidechain: bool) -> MessageRecord {
    claude_code_message(&ClaudeMessage {
        session_id: CLAUDE_SESSION,
        message_id: id,
        timestamp: Some("2026-08-09T03:07:04.001Z"),
        model: Some("claude-sonnet-5"),
        cwd: Some("D:\\Repos\\example"),
        is_sidechain,
        usage: claude_raw(),
    })
}

fn claude_rollup(message_count: usize) -> SessionRecord {
    let raw = claude_raw();
    claude_code_session(&ClaudeSessionRollup {
        session_id: CLAUDE_SESSION,
        cwd: Some("D:\\Repos\\example"),
        model: Some("claude-sonnet-5"),
        started_at: Some("2026-08-09T03:07:04.000Z"),
        ended_at: Some("2026-08-09T03:10:00.000Z"),
        message_count,
        usage: RawTokenCounts::sum([&raw].into_iter().cycle().take(message_count)),
    })
}

mod imports;
mod schema;
mod snapshots;
mod totals;
