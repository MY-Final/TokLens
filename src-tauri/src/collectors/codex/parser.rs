//! Streaming JSONL parser: extract only whitelisted metadata.

use super::parsed::ParsedFile;
use crate::collectors::files::valid_counts;
use crate::usage::model::MessageRecord;
use crate::usage::normalize::{
    CodexRecordIdentity, CodexUsageRecord, codex_dedup_key, codex_message,
};
use crate::usage::tokens::RawTokenCounts;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 采集白名单：只提取这些字段，其余内容一律不读、不存、不记录。
pub const FIELD_WHITELIST: &[&str] = &[
    "type",
    "ordinal",
    "timestamp",
    "payload.id",
    "payload.session_id",
    "payload.cwd",
    "payload.model",
    "payload.info.last_token_usage",
    "payload.info.total_token_usage",
    "payload.response_id",
    "payload.thread_id",
    "payload.usage",
    "payload.thread_token_usage",
    "payload.type",
    "payload.history_base.thread_id",
    "payload.base_instructions.provenance.model",
];

/// 一条旧格式 Token 事件（`event_msg.token_count`）。
#[derive(Debug, Clone)]
struct LegacyEvent {
    ordinal: Option<i64>,
    line: usize,
    cumulative: Option<RawTokenCounts>,
    timestamp: Option<String>,
    thread_id: String,
    /// `info.last_token_usage`：本次请求的增量。
    usage: RawTokenCounts,
}

/// 汇总明细：新版记录全部保留；旧格式事件只保留"新格式尚未开始"的前缀，
/// 与新格式逐条配对的镜像事件（值相同、按顺序一一对应）必须丢弃，避免同一请求被统计两次。
fn resolve_messages(
    parsed: &mut ParsedFile,
    new_records: &[MessageRecord],
    legacy_events: &[LegacyEvent],
) {
    let mut mirror_index = 0usize;
    let mut seen_legacy = BTreeSet::new();
    let source_path = parsed.path.to_string_lossy();
    for event in legacy_events {
        let record = CodexUsageRecord {
            thread_id: &event.thread_id,
            response_id: None,
            identity: record_identity(event.ordinal, event.cumulative, &source_path, event.line),
            timestamp: event.timestamp.as_deref(),
            usage: event.usage,
        };
        if event.ordinal.is_none() && !seen_legacy.insert(codex_dedup_key(&record)) {
            continue;
        }
        let is_mirror = mirror_index < new_records.len()
            && event.usage.client_total.is_some()
            && new_records[mirror_index].usage.total_tokens == event.usage.client_total;
        if is_mirror {
            mirror_index += 1;
            continue;
        }
        parsed.messages.push(codex_message(&record));
    }
    parsed.messages.extend(new_records.iter().cloned());
    // ordinal-free 日志可反复通知同一累计值（例如状态更新），保留首次观测时间。
    let mut seen = BTreeSet::new();
    parsed
        .messages
        .retain(|message| message.seq.is_some() || seen.insert(message.dedup_key.clone()));
}

fn record_identity<'a>(
    ordinal: Option<i64>,
    cumulative: Option<RawTokenCounts>,
    path: &'a str,
    line: usize,
) -> CodexRecordIdentity<'a> {
    if let Some(ordinal) = ordinal {
        CodexRecordIdentity::Ordinal(ordinal)
    } else if let Some(counts) = cumulative.filter(|counts| counts.client_total.is_some()) {
        CodexRecordIdentity::Cumulative(counts)
    } else {
        CodexRecordIdentity::FileLine { path, line }
    }
}

/// 按行解析一个 rollout 文件。只访问白名单字段，正文不会被取出。
pub(super) fn parse_file(
    path: &Path,
    size_bytes: i64,
    mtime_ms: i64,
) -> std::io::Result<ParsedFile> {
    let reader = BufReader::new(File::open(path)?);
    let parsed = parse_reader(reader, path, size_bytes, mtime_ms)?;
    if !fingerprint_matches(&parsed) {
        return Err(std::io::Error::other(
            "Codex log changed during reading; retry on next scan",
        ));
    }
    Ok(parsed)
}

pub(super) fn fingerprint_matches(parsed: &ParsedFile) -> bool {
    std::fs::metadata(&parsed.path).is_ok_and(|metadata| {
        metadata.len() as i64 == parsed.size_bytes
            && super::discovery::mtime_ms(&metadata) == parsed.mtime_ms
    })
}

pub(super) fn parse_reader(
    mut reader: impl BufRead,
    path: &Path,
    size_bytes: i64,
    mtime_ms: i64,
) -> std::io::Result<ParsedFile> {
    let mut parsed = ParsedFile::new(path, size_bytes, mtime_ms);
    let mut new_records: Vec<MessageRecord> = Vec::new();
    let mut legacy_events: Vec<LegacyEvent> = Vec::new();

    let mut line = String::new();
    let mut line_number = 0;
    loop {
        line.clear();
        // 读取失败必须放弃本次快照，不能把半份日志提交并推进水位。
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        line_number += 1;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                if !line.ends_with('\n') {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Incomplete Codex log tail; retry on next scan",
                    ));
                }
                parsed.unparsed_lines += 1;
                continue;
            }
        };
        // 明细完整度只看文件自身的 ordinal 起点（调研口径：起点不为 0 说明是分页/续接文件）。
        if let Some(ordinal) = value.get("ordinal").and_then(Value::as_i64) {
            parsed.mark_ordinal(ordinal);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => parse_session_meta(&mut parsed, &value),
            Some("turn_context") => parse_turn_context(&mut parsed, &value),
            Some("token_usage_record") => {
                parse_token_usage_record(&mut parsed, &value, line_number, &mut new_records)
            }
            Some("event_msg") => {
                parse_legacy_token_count(&mut parsed, &value, line_number, &mut legacy_events)
            }
            // 其余行类型（正文、工具输出、状态等）一律忽略，不读取任何字段。
            _ => {}
        }
    }

    resolve_messages(&mut parsed, &new_records, &legacy_events);
    Ok(parsed)
}

fn parse_session_meta(parsed: &mut ParsedFile, value: &Value) {
    let payload = value.get("payload");
    if parsed.thread_id.is_none() {
        parsed.thread_id = payload
            .and_then(|payload| payload.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.session_id.is_none() {
        parsed.session_id = payload
            .and_then(|payload| payload.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.parent_thread_id.is_none() {
        parsed.parent_thread_id = payload
            .and_then(|payload| payload.get("history_base"))
            .and_then(|history| history.get("thread_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.cwd.is_none() {
        parsed.cwd = payload
            .and_then(|payload| payload.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.model.is_none() {
        parsed.model = payload
            .and_then(|payload| payload.get("base_instructions"))
            .and_then(|instructions| instructions.get("provenance"))
            .and_then(|provenance| provenance.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    parsed.mark_time(line_timestamp(value));
}

fn parse_turn_context(parsed: &mut ParsedFile, value: &Value) {
    let payload = value.get("payload");
    // 实际运行的模型以 `turn_context` 为准（会话元数据里的可能只是默认值）。
    if let Some(model) = payload
        .and_then(|payload| payload.get("model"))
        .and_then(Value::as_str)
    {
        parsed.model = Some(model.to_string());
    }
    if parsed.cwd.is_none() {
        parsed.cwd = payload
            .and_then(|payload| payload.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    parsed.mark_time(line_timestamp(value));
}

fn parse_token_usage_record(
    parsed: &mut ParsedFile,
    value: &Value,
    line: usize,
    records: &mut Vec<MessageRecord>,
) {
    let payload = value.get("payload");
    let timestamp = line_timestamp(value);
    let ordinal = value.get("ordinal").and_then(Value::as_i64);
    let thread_id = payload
        .and_then(|payload| payload.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| parsed.thread_id.clone());
    let Some(thread_id) = thread_id else {
        parsed.unparsed_lines += 1;
        return;
    };
    if parsed.session_id.is_none() {
        parsed.session_id = payload
            .and_then(|payload| payload.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    // 计数可能超过 i64（损坏或异常输入）；写库前拒绝，避免 `as i64` 回绕成负数。
    let usage = raw_counts(payload.and_then(|payload| payload.get("usage")));
    if !valid_counts(&usage) {
        parsed.unparsed_lines += 1;
        return;
    }
    parsed.mark_time(timestamp);
    let cumulative = payload
        .and_then(|payload| payload.get("thread_token_usage"))
        .filter(|usage| usage.is_object())
        .map(|usage| raw_counts(Some(usage)))
        .filter(valid_counts);
    let source_path = parsed.path.to_string_lossy();
    records.push(codex_message(&CodexUsageRecord {
        thread_id: &thread_id,
        response_id: payload
            .and_then(|payload| payload.get("response_id"))
            .and_then(Value::as_str),
        identity: record_identity(ordinal, cumulative, &source_path, line),
        timestamp,
        usage,
    }));
    // 文件按追加顺序写入：最后一条即为文件末尾的线程累计。
    if let Some(cumulative) = cumulative {
        parsed.new_cumulative = Some(cumulative);
    }
}

fn parse_legacy_token_count(
    parsed: &mut ParsedFile,
    value: &Value,
    line: usize,
    events: &mut Vec<LegacyEvent>,
) {
    let payload = value.get("payload");
    if payload
        .and_then(|payload| payload.get("type"))
        .and_then(Value::as_str)
        != Some("token_count")
    {
        return;
    }
    let Some(info) = payload
        .and_then(|payload| payload.get("info"))
        .filter(|info| info.is_object())
    else {
        return;
    };
    let Some(last_usage) = info
        .get("last_token_usage")
        .filter(|usage| usage.is_object())
    else {
        return;
    };
    let timestamp = line_timestamp(value);
    let ordinal = value.get("ordinal").and_then(Value::as_i64);
    let thread_id = payload
        .and_then(|payload| payload.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| parsed.thread_id.clone());
    let Some(thread_id) = thread_id else {
        parsed.unparsed_lines += 1;
        return;
    };
    // 与新版路径一致：超过 i64 的计数拒绝并计数，不入库、不回绕。
    let usage = raw_counts(Some(last_usage));
    if !valid_counts(&usage) {
        parsed.unparsed_lines += 1;
        return;
    }
    parsed.mark_time(timestamp);
    let cumulative = info
        .get("total_token_usage")
        .filter(|usage| usage.is_object())
        .map(|usage| raw_counts(Some(usage)))
        .filter(valid_counts);
    events.push(LegacyEvent {
        ordinal,
        line,
        cumulative,
        timestamp: timestamp.map(str::to_string),
        thread_id,
        usage,
    });
    if let Some(cumulative) = cumulative {
        parsed.legacy_cumulative = Some(cumulative);
    }
}

fn line_timestamp(value: &Value) -> Option<&str> {
    value.get("timestamp").and_then(Value::as_str).or_else(|| {
        value
            .get("payload")
            .and_then(|payload| payload.get("timestamp"))
            .and_then(Value::as_str)
    })
}

/// 从客户端字段映射到统一计数（Codex 字段名 → `RawTokenCounts`）。
fn raw_counts(usage: Option<&Value>) -> RawTokenCounts {
    let Some(usage) = usage else {
        return RawTokenCounts::default();
    };
    RawTokenCounts {
        input: usage.get("input_tokens").and_then(Value::as_u64),
        output: usage.get("output_tokens").and_then(Value::as_u64),
        cache_read: usage.get("cached_input_tokens").and_then(Value::as_u64),
        cache_write: usage
            .get("cache_write_input_tokens")
            .and_then(Value::as_u64),
        reasoning: usage.get("reasoning_output_tokens").and_then(Value::as_u64),
        client_total: usage.get("total_tokens").and_then(Value::as_u64),
    }
}
