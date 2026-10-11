//! Read only usage metadata; early chunk samples are replaced by final usage for each step.

use crate::collectors::files::valid_counts;
use crate::usage::normalize::dsh::{DshMessage, DshSession, dsh_message, dsh_session};
use crate::usage::store::SessionImport;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    #[serde(rename = "type")]
    kind: String,
    version: u64,
    id: String,
    created_at: Option<i64>,
    cwd: Option<String>,
    parent_session: Option<String>,
    seed_length: Option<i64>,
    origin: Option<String>,
}

#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    seq: Option<i64>,
    time: Option<i64>,
    data: T,
}

#[derive(Deserialize)]
struct Assistant {
    turn: Option<i64>,
    step: Option<i64>,
    usage: Option<Usage>,
    chunk: Option<Chunk>,
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(rename = "type")]
    kind: String,
    usage: Option<Usage>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "optional_count")]
    cache_read_tokens: Option<u64>,
    #[serde(default, deserialize_with = "optional_count")]
    cache_write_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
}

fn optional_count<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    u64::deserialize(deserializer).map(Some)
}

impl Usage {
    fn raw(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: Some(self.cache_read_tokens.unwrap_or(0)),
            cache_write: Some(self.cache_write_tokens.unwrap_or(0)),
            reasoning: self.reasoning_tokens,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct RequestHeader {
    header: Request,
}
#[derive(Deserialize)]
struct Request {
    config: Config,
}
#[derive(Deserialize)]
struct Config {
    provider: Option<String>,
    model: Option<String>,
}

pub(super) struct Parsed {
    pub import: SessionImport,
    pub unparsed: usize,
}

pub(super) fn accepts(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "session.jsonl" || name == "session.jsonl.zstd")
}

fn invalid() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unsupported or incomplete DSH session",
    )
}

pub(super) fn parse_file(path: &Path) -> std::io::Result<Parsed> {
    let file = std::fs::File::open(path)?;
    if path.extension().is_some_and(|ext| ext == "zstd") {
        parse_reader(
            BufReader::new(zstd::stream::read::Decoder::new(file)?),
            path,
        )
    } else {
        parse_reader(BufReader::new(file), path)
    }
}

fn parse_reader(mut reader: impl BufRead, path: &Path) -> std::io::Result<Parsed> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let header: Header = serde_json::from_str(&line).map_err(|_| invalid())?;
    if header.kind != "session" || header.version != 0 || header.id.is_empty() {
        return Err(invalid());
    }
    let mut unparsed = 0;
    let mut model = None;
    let mut end = header.created_at;
    let mut messages = BTreeMap::new();
    let mut usages = BTreeMap::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let tag: Tag = match serde_json::from_str(&line) {
            Ok(tag) => tag,
            Err(_) if !line.ends_with('\n') => return Err(invalid()),
            Err(_) => {
                unparsed += 1;
                continue;
            }
        };
        match tag.kind.as_str() {
            "request/header" => match serde_json::from_str::<Envelope<RequestHeader>>(&line) {
                Ok(event) => {
                    let config = event.data.header.config;
                    model = config.model.map(|model| {
                        config
                            .provider
                            .map_or_else(|| model.clone(), |provider| format!("{provider}/{model}"))
                    });
                }
                Err(_) => unparsed += 1,
            },
            "assistant/message" | "assistant/chunk" => {
                let event = match serde_json::from_str::<Envelope<Assistant>>(&line) {
                    Ok(event) => event,
                    Err(_) => {
                        unparsed += 1;
                        continue;
                    }
                };
                let Some(seq) = event.seq.filter(|seq| *seq >= 0) else {
                    unparsed += 1;
                    continue;
                };
                if seq < header.seed_length.unwrap_or(0) {
                    continue;
                }
                let (Some(turn), Some(step)) = (
                    event.data.turn.filter(|value| *value >= 0),
                    event.data.step.filter(|value| *value >= 0),
                ) else {
                    unparsed += 1;
                    continue;
                };
                let is_complete = tag.kind == "assistant/message";
                let sample = if is_complete {
                    event.data.usage
                } else {
                    let Some(chunk) = event.data.chunk else {
                        unparsed += 1;
                        continue;
                    };
                    if chunk.kind != "usage" {
                        continue;
                    }
                    chunk.usage
                };
                let key = (turn, step);
                let usage = sample
                    .as_ref()
                    .map(Usage::raw)
                    .or_else(|| usages.get(&key).copied())
                    .unwrap_or_default();
                if !valid_counts(&usage) {
                    unparsed += 1;
                    continue;
                }
                end = end.max(event.time);
                let mut message = dsh_message(&DshMessage {
                    session_id: &header.id,
                    seq,
                    turn,
                    step,
                    time_ms: event.time,
                    model: model.as_deref(),
                    usage,
                    is_sidechain: header.origin.as_deref() == Some("subagent"),
                });
                if is_complete {
                    message.completed_at_ms = event.time;
                }
                messages.insert(key, message);
                usages.insert(key, usage);
            }
            // These are content, boundary or plugin events, never another source of assistant usage.
            "text-chunks"
            | "reasoning-chunks"
            | "tool-call-chunks"
            | "user/message"
            | "tool/call"
            | "tool/result"
            | "turn/start"
            | "turn/end"
            | "step/start"
            | "step/end"
            | "request/context"
            | "permission/preset"
            | "sandbox/mode"
            | "approval/policy"
            | "session/end-seed"
            | "agent/inbox/spliced"
            | "session/title"
            | "session/title-llm-request"
            | "web/deepseek-search-llm-request"
            | "llm/retry"
            | "llm/retry-started"
            | "todo/write"
            | "command/run"
            | "command/done"
            | "approval/asked"
            | "approval/decided"
            | "compaction/start"
            | "compaction/summary"
            | "compaction/end"
            | "agent-preset/selected" => {}
            _ => unparsed += 1,
        }
    }
    let usages = usages.into_values().collect::<Vec<_>>();
    let counts = crate::usage::normalize::sum_complete_counts(&usages);
    let messages = messages.into_values().collect::<Vec<_>>();
    let mut session = dsh_session(&DshSession {
        id: &header.id,
        parent: header.parent_session.as_deref(),
        cwd: header.cwd.as_deref(),
        model: model.as_deref(),
        start_ms: header.created_at,
        end_ms: end,
        message_count: messages.len(),
        usage: counts,
    });
    if unparsed > 0 {
        session.coverage = crate::usage::model::MessageCoverage::Partial;
    }
    Ok(Parsed {
        import: SessionImport {
            session,
            messages,
            source_path: path.to_string_lossy().to_string(),
            message_source_paths: BTreeMap::new(),
        },
        unparsed,
    })
}
