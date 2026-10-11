//! Stream schema-1 transcript metadata; blocks/content/tool payloads are never deserialized.
use super::ledger::Usage;
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

#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    schema: u64,
    session_id: String,
    created_at: String,
    cwd: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    role: String,
    created_at: Option<String>,
    meta: Option<Meta>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    model_id: Option<String>,
    usage: Option<Usage>,
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
            let start = parse_iso8601_utc_ms(&header.created_at).ok_or_else(invalid)?;
            if tag.kind != "session" || header.schema != 1 || header.session_id.is_empty() {
                return Err(invalid());
            }
            parsed = Some(Parsed {
                meta: SessionMeta {
                    id: header.session_id,
                    parent: None,
                    project: header.cwd,
                    start_ms: Some(start),
                    end_ms: Some(start),
                },
                requests: Vec::new(),
                unparsed: 0,
            });
            continue;
        }
        let parsed = parsed.as_mut().ok_or_else(invalid)?;
        if tag.kind != "message" {
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
        let time = row.created_at.as_deref().and_then(parse_iso8601_utc_ms);
        parsed.meta.end_ms = parsed.meta.end_ms.max(time);
        match row.role.as_str() {
            "assistant" => {}
            "user" | "system" | "tool" => continue,
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
            let message = request_ledger::request(
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
            parsed.requests.push(Candidate { message, raw });
        }
    }
    parsed.ok_or_else(invalid)
}
