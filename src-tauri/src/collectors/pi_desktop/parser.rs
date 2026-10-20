//! Stream schema-1 transcript metadata; blocks/content/tool payloads are never deserialized.
use super::ledger::Usage;
use crate::collectors::files::valid_counts;
use crate::usage::{
    model::MessageRecord,
    normalize::request_ledger::{self, RequestMeta, SessionMeta},
    time::parse_iso8601_utc_ms,
    tokens::{Client, RawTokenCounts},
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
};

/// 时间戳：schema 1 是 ISO 字符串，版本 3 的消息行是纪元毫秒整数。
#[derive(Deserialize)]
#[serde(untagged)]
enum Timestamp {
    Millis(i64),
    Iso(String),
}

impl Timestamp {
    fn millis(&self) -> Option<i64> {
        match self {
            Self::Millis(value) => Some(*value),
            Self::Iso(value) => parse_iso8601_utc_ms(value),
        }
        .filter(|value| *value >= 0)
    }
}

#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "type")]
    kind: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    schema: Option<u64>,
    session_id: Option<String>,
    created_at: Option<Timestamp>,
    version: Option<u64>,
    id: Option<String>,
    timestamp: Option<Timestamp>,
    cwd: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    role: Option<String>,
    created_at: Option<Timestamp>,
    meta: Option<Meta>,
    timestamp: Option<Timestamp>,
    message: Option<RowMessage>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    model_id: Option<String>,
    usage: Option<Usage>,
}

/// 版本 3 的消息体。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RowMessage {
    role: Option<String>,
    model: Option<String>,
    usage: Option<MessageUsage>,
    timestamp: Option<Timestamp>,
}

/// 版本 3 的用量：输入与缓存各自独立，推理是输出的子集。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageUsage {
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    reasoning: Option<u64>,
    total_tokens: Option<u64>,
}

impl MessageUsage {
    fn raw(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input,
            output: self.output,
            reasoning: self.reasoning,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            client_total: self.total_tokens,
        }
    }
}
pub(super) struct Candidate {
    pub message: MessageRecord,
    pub raw: RawTokenCounts,
}
pub(super) struct Parsed {
    pub meta: SessionMeta,
    pub requests: Vec<Candidate>,
    pub unparsed: usize,
}
fn invalid() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unsupported or incomplete Pi Desktop session",
    )
}
/// 版本 3 里不携带用量的事件类型。
fn known_event(kind: &str) -> bool {
    matches!(kind, "model_change" | "thinking_level_change")
}

pub(super) fn accepts(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "jsonl")
}
pub(super) fn read(path: &Path) -> std::io::Result<Parsed> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    let mut parsed: Option<Parsed> = None;
    let mut ordinal = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        ordinal += 1;
        if line.trim().is_empty() {
            continue;
        }
        let tag: Tag = match serde_json::from_str(&line) {
            Ok(tag) => tag,
            Err(_) if !line.ends_with('\n') => return Err(invalid()),
            Err(_) => {
                let parsed = parsed.as_mut().ok_or_else(invalid)?;
                parsed.unparsed += 1;
                continue;
            }
        };
        if parsed.is_none() {
            let header: Header = serde_json::from_str(&line).map_err(|_| invalid())?;
            if tag.kind.as_deref() != Some("session") {
                return Err(invalid());
            }
            let (id, created, cwd) = match (header.schema, header.version) {
                (Some(1), _) => (header.session_id, header.created_at, header.cwd),
                (_, Some(3)) => (header.id, header.timestamp, header.cwd),
                _ => return Err(invalid()),
            };
            let id = id.filter(|id| !id.is_empty()).ok_or_else(invalid)?;
            let start = created
                .as_ref()
                .and_then(Timestamp::millis)
                .ok_or_else(invalid)?;
            parsed = Some(Parsed {
                meta: SessionMeta {
                    id,
                    parent: None,
                    project: cwd,
                    start_ms: Some(start),
                    end_ms: Some(start),
                },
                requests: Vec::new(),
                unparsed: 0,
            });
            continue;
        }
        let parsed = parsed.as_mut().ok_or_else(invalid)?;
        if tag.kind.as_deref() != Some("message") {
            // 模型切换等事件不携带用量，不算异常记录。
            if tag.kind.as_deref().is_some_and(known_event) {
                continue;
            }
            parsed.unparsed += 1;
            continue;
        }
        let row: Row = match serde_json::from_str(&line) {
            Ok(row) => row,
            Err(_) => {
                parsed.unparsed += 1;
                continue;
            }
        };
        if let Some(message) = &row.message {
            // 版本 3：用量挂在消息体上，一条消息一条用量。
            let time = row
                .timestamp
                .as_ref()
                .or(message.timestamp.as_ref())
                .and_then(Timestamp::millis);
            parsed.meta.end_ms = parsed.meta.end_ms.max(time);
            match message.role.as_deref() {
                Some("assistant") => {}
                Some("user" | "system" | "toolResult") => continue,
                _ => {
                    parsed.unparsed += 1;
                    continue;
                }
            }
            if row.id.is_empty() {
                parsed.unparsed += 1;
                continue;
            }
            let raw = message
                .usage
                .as_ref()
                .map_or(RawTokenCounts::default(), MessageUsage::raw);
            if message.usage.is_some() && !valid_counts(&raw) {
                parsed.unparsed += 1;
                continue;
            }
            let key = format!("message:{}", row.id);
            let mut request = request_ledger::request(
                Client::PiDesktop,
                &parsed.meta.id,
                RequestMeta {
                    key,
                    model: message.model.clone(),
                    agent: None,
                    created_ms: time,
                    completed_ms: time,
                    sidechain: false,
                    source_ref: format!("line={ordinal}"),
                },
                &raw,
            );
            // 没有客户端总量时，完整分桶换算出的总量是确定性换算而不是实测。
            request_ledger::classify_request_source(&mut request);
            parsed.requests.push(Candidate {
                message: request,
                raw,
            });
            continue;
        }
        let time = row.created_at.as_ref().and_then(Timestamp::millis);
        parsed.meta.end_ms = parsed.meta.end_ms.max(time);
        match row.role.as_deref() {
            Some("assistant") => {}
            Some("user" | "system" | "tool") => continue,
            _ => {
                parsed.unparsed += 1;
                continue;
            }
        }
        if row.id.is_empty() {
            parsed.unparsed += 1;
            continue;
        }
        let meta = row.meta;
        let model = meta.as_ref().and_then(|m| m.model_id.clone());
        let expanded = match meta.as_ref().and_then(|m| m.usage.as_ref()) {
            Some(usage) => match usage.expand(&row.id) {
                Some(values) => values,
                None => {
                    parsed.unparsed += 1;
                    continue;
                }
            },
            None => vec![(
                format!("message:{}", row.id),
                None,
                RawTokenCounts::default(),
            )],
        };
        for (key, usage_model, raw) in expanded {
            let mut message = request_ledger::request(
                Client::PiDesktop,
                &parsed.meta.id,
                RequestMeta {
                    key,
                    model: usage_model.or_else(|| model.clone()),
                    agent: None,
                    created_ms: time,
                    completed_ms: time,
                    sidechain: false,
                    source_ref: format!("line={ordinal}"),
                },
                &raw,
            );
            // No client-reported total: an operation total derived from complete
            // buckets is a deterministic conversion, not measured.
            request_ledger::classify_request_source(&mut message);
            parsed.requests.push(Candidate { message, raw });
        }
    }
    parsed.ok_or_else(invalid)
}
