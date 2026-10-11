//! Shared synthetic rollout fixtures; never read real client directories.

use super::*;
use crate::usage::client::Client;
use crate::usage::store::{TotalsFilter, UsageStore};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const THREAD: &str = "01a129d9-6720-7020-9402-a0a5930c37e6";
const PARENT: &str = "01a129d8-f6be-72b1-a243-c141d54e2791";
/// 用于验证"正文绝不入库"的哨兵字符串。
const SENTINEL: &str = "SENTINEL-PROMPT-TEXT-DO-NOT-STORE";

fn temp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "toklens-codex-{}-{}-{}",
        tag,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn lines_to_text(lines: &[Value]) -> String {
    let mut text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    text
}

fn write_file(dir: &Path, name: &str, lines: &[Value]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, lines_to_text(lines)).expect("write synthetic jsonl");
    path
}

fn append_lines(path: &Path, lines: &[Value]) {
    let mut text = std::fs::read_to_string(path).expect("read synthetic jsonl");
    text.push_str(&lines_to_text(lines));
    std::fs::write(path, text).expect("append synthetic jsonl");
}

/// Codex 的 `usage` / `thread_token_usage` 结构：`total = input + output`。
fn counts(input: u64, output: u64, cached: u64, cache_write: u64, reasoning: u64) -> Value {
    json!({
        "input_tokens": input,
        "output_tokens": output,
        "cached_input_tokens": cached,
        "cache_write_input_tokens": cache_write,
        "reasoning_output_tokens": reasoning,
        "total_tokens": input + output,
    })
}

fn session_meta_at(thread: &str, at: &str, parent: Option<&str>, ordinal: i64) -> Value {
    let mut payload = json!({
        "id": thread,
        "session_id": thread,
        "timestamp": at,
        "cwd": "D:\\WorkSpace\\TokLens",
        "cli_version": "0.162.0-alpha.17.2",
        "base_instructions": { "provenance": { "model": "gpt-5.6-luna" } },
    });
    if let Some(parent) = parent {
        payload["history_base"] = json!({ "thread_id": parent, "end_ordinal_exclusive": 807 });
    }
    json!({ "timestamp": at, "ordinal": ordinal, "type": "session_meta", "payload": payload })
}

fn usage_record(
    thread: &str,
    at: &str,
    ordinal: i64,
    response_id: &str,
    usage: Value,
    thread_usage: Value,
) -> Value {
    json!({
        "timestamp": at,
        "ordinal": ordinal,
        "type": "token_usage_record",
        "payload": {
            "thread_id": thread,
            "session_id": thread,
            "turn_id": "turn-1",
            "response_id": response_id,
            "usage": usage,
            "turn_token_usage": usage.clone(),
            "thread_token_usage": thread_usage,
        },
    })
}

fn legacy_token_count(
    thread: Option<&str>,
    at: &str,
    ordinal: i64,
    last: Value,
    total: Value,
) -> Value {
    let mut payload = json!({
        "type": "token_count",
        "info": { "last_token_usage": last, "total_token_usage": total, "model_context_window": 200000 },
    });
    if let Some(thread) = thread {
        payload["thread_id"] = json!(thread);
    }
    json!({ "timestamp": at, "ordinal": ordinal, "type": "event_msg", "payload": payload })
}

fn sync(store: &UsageStore, root: &Path) -> SyncReport {
    CodexCollector::new(root)
        .sync(store, &SyncOptions::default())
        .expect("sync")
}

fn reason_count(report: &SyncReport, reason: ScanReason) -> usize {
    report.reasons.get(&reason).copied().unwrap_or(0)
}

fn codex_totals(store: &UsageStore) -> crate::usage::store::UsageTotals {
    store
        .totals(&TotalsFilter {
            client: Some(Client::Codex),
            ..TotalsFilter::default()
        })
        .expect("totals")
}

fn coverage(store: &UsageStore, client_session_id: &str) -> String {
    store
        .connection_for_tests()
        .query_row(
            "SELECT message_coverage FROM sessions WHERE client = 'codex' AND client_session_id = ?1",
            rusqlite::params![client_session_id],
            |row| row.get::<_, String>(0),
        )
        .expect("coverage")
}

mod coverage;
mod formats;
mod incremental;
mod privacy;
