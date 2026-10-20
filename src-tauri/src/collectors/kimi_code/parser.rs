//! Stream usage metadata only. Content, tool arguments and context token estimates are skipped.
//!
//! Two wire protocols are supported:
//!
//! - `1.1`：每行是 `{"timestamp": 秒, "message": {"type": ..., "payload": {...}}}`，
//!   用量在 `payload.token_usage`，每条用量带一个 `message_id`。
//! - `1.4` / `1.5`：每行自带 `type`，用量行是 `usage.record`。

use super::metadata::invalid;
use crate::collectors::files::valid_counts;
use crate::usage::model::MessageRecord;
use crate::usage::normalize::kimi_code::{KimiMessageMeta, kimi_message};
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 记录标签。1.1 的记录没有顶层 `type`，因此这里允许缺失。
#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Header {
    protocol_version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    model: Option<String>,
    usage: Option<Usage>,
    usage_scope: Option<String>,
    time: Option<i64>,
}

/// 1.1 的记录外壳。
#[derive(Deserialize)]
struct Envelope {
    timestamp: Option<f64>,
    message: Option<EnvelopeMessage>,
}

#[derive(Deserialize)]
struct EnvelopeMessage {
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<EnvelopePayload>,
}

/// 1.1 的负载字段用下划线命名（`message_id` / `token_usage`）。
#[derive(Deserialize)]
struct EnvelopePayload {
    message_id: Option<String>,
    token_usage: Option<Usage>,
}

/// 用量桶。1.4 / 1.5 用驼峰键，1.1 用下划线键，两种写法都接受。
#[derive(Deserialize)]
struct Usage {
    #[serde(alias = "inputOther")]
    input_other: Option<u64>,
    output: Option<u64>,
    #[serde(alias = "inputCacheRead")]
    input_cache_read: Option<u64>,
    #[serde(alias = "inputCacheCreation")]
    input_cache_creation: Option<u64>,
}

impl Usage {
    fn counts(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_other,
            output: self.output,
            cache_read: self.input_cache_read,
            cache_write: self.input_cache_creation,
            ..Default::default()
        }
    }
}

pub(super) struct ParsedWire {
    pub messages: Vec<MessageRecord>,
    pub counts: Vec<RawTokenCounts>,
    pub unparsed: usize,
}

/// 同一去重键只保留最后一次出现，避免重复记录被重复计入会话总量。
fn push_record(
    parsed: &mut ParsedWire,
    seen: &mut std::collections::BTreeMap<String, usize>,
    message: MessageRecord,
    counts: RawTokenCounts,
) {
    let key = message.dedup_key.clone();
    match seen.get(&key) {
        Some(index) => {
            parsed.messages[*index] = message;
            parsed.counts[*index] = counts;
        }
        None => {
            seen.insert(key, parsed.messages.len());
            parsed.messages.push(message);
            parsed.counts.push(counts);
        }
    }
}

pub(super) fn read(path: &Path, session_id: &str) -> std::io::Result<ParsedWire> {
    let agent = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(invalid)?;
    let mut parsed = ParsedWire {
        messages: Vec::new(),
        counts: Vec::new(),
        unparsed: 0,
    };
    let mut seen: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut has_header = false;
    let mut protocol = String::new();
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    let mut ordinal = 0i64;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        ordinal = ordinal.checked_add(1).ok_or_else(invalid)?;
        if line.trim().is_empty() {
            continue;
        }
        let tag: Tag = match serde_json::from_str(&line) {
            Ok(tag) => tag,
            Err(_) if !line.ends_with('\n') => return Err(invalid()),
            Err(_) => {
                parsed.unparsed += 1;
                continue;
            }
        };
        if !has_header {
            let header: Header = serde_json::from_str(&line).map_err(|_| invalid())?;
            if tag.kind.as_deref() != Some("metadata") {
                return Err(invalid());
            }
            protocol = header.protocol_version;
            if !matches!(protocol.as_str(), "1.1" | "1.4" | "1.5") {
                return Err(invalid());
            }
            has_header = true;
            continue;
        }
        if protocol == "1.1" {
            read_v11(&line, session_id, ordinal, &mut parsed, &mut seen);
            continue;
        }
        match tag.kind.as_deref() {
            Some("usage.record") => {
                let record: Record = match serde_json::from_str(&line) {
                    Ok(record) => record,
                    Err(_) => {
                        parsed.unparsed += 1;
                        continue;
                    }
                };
                if record
                    .usage_scope
                    .as_deref()
                    .is_some_and(|scope| !matches!(scope, "turn" | "session"))
                {
                    parsed.unparsed += 1;
                    continue;
                }
                let counts = record
                    .usage
                    .as_ref()
                    .map_or(RawTokenCounts::default(), Usage::counts);
                if !valid_counts(&counts) {
                    parsed.unparsed += 1;
                    continue;
                }
                let message = kimi_message(
                    session_id,
                    KimiMessageMeta {
                        key: format!("agent:{agent}:line:{ordinal}"),
                        agent: Some(agent.to_string()),
                        seq: Some(ordinal),
                        source_ref: format!("line={ordinal}"),
                    },
                    record.time.filter(|time| *time >= 0),
                    record.model,
                    &counts,
                );
                push_record(&mut parsed, &mut seen, message, counts);
            }
            Some(
                "runtime.set_binding"
                | "mcp.tools_discovered"
                | "profile.bind"
                | "config.update"
                | "permission.set_mode"
                | "turn.prompt"
                | "context.append_message"
                | "agent.message.appended"
                | "agent.turn.started"
                | "plugin.session_start"
                | "context.append_loop_event"
                | "llm.tools_snapshot"
                | "llm.request"
                | "token_counting.measured"
                | "agent.turn.ended"
                | "turn.ended"
                | "token_counting.turn_recorded"
                | "prompt.completed"
                | "tools.set_active_tools",
            ) => {}
            _ => parsed.unparsed += 1,
        }
    }
    if !has_header {
        return Err(invalid());
    }
    Ok(parsed)
}

/// 1.1 协议：用量挂在消息负载上，键取自消息 id。
fn read_v11(
    line: &str,
    session_id: &str,
    ordinal: i64,
    parsed: &mut ParsedWire,
    seen: &mut std::collections::BTreeMap<String, usize>,
) {
    let envelope: Envelope = match serde_json::from_str(line) {
        Ok(envelope) => envelope,
        Err(_) => {
            parsed.unparsed += 1;
            return;
        }
    };
    let Some(message) = envelope.message else {
        parsed.unparsed += 1;
        return;
    };
    let Some(payload) = message.payload else {
        // 已知的不带负载的消息类型不算异常记录。
        if message.kind.as_deref().is_some_and(known_v11_kind) {
            return;
        }
        parsed.unparsed += 1;
        return;
    };
    let Some(usage) = payload.token_usage else {
        if message.kind.as_deref().is_some_and(known_v11_kind) {
            return;
        }
        parsed.unparsed += 1;
        return;
    };
    let counts = usage.counts();
    if !valid_counts(&counts) {
        parsed.unparsed += 1;
        return;
    }
    let key = payload
        .message_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .map_or_else(|| format!("line:{ordinal}"), |id| format!("message:{id}"));
    let message = kimi_message(
        session_id,
        KimiMessageMeta {
            key,
            agent: None,
            seq: Some(ordinal),
            source_ref: payload
                .message_id
                .as_deref()
                .map_or_else(|| format!("line={ordinal}"), |id| format!("message={id}")),
        },
        envelope.timestamp.and_then(millis_from_seconds),
        None,
        &counts,
    );
    push_record(parsed, seen, message, counts);
}

/// 1.1 已知的非用量消息类型。
fn known_v11_kind(kind: &str) -> bool {
    matches!(
        kind,
        "ContentPart"
            | "StepBegin"
            | "StatusUpdate"
            | "ToolCall"
            | "ToolResult"
            | "TurnBegin"
            | "ToolCallPart"
            | "ApprovalRequest"
            | "ApprovalResponse"
            | "StepInterrupted"
            | "CompactionBegin"
            | "CompactionEnd"
    )
}

/// 1.1 的时间戳是带小数的秒。
fn millis_from_seconds(seconds: f64) -> Option<i64> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as i64)
}
