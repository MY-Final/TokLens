//! Stream usage metadata only. Content, tool arguments and context token estimates are skipped.

use super::metadata::invalid;
use crate::collectors::files::valid_counts;
use crate::usage::model::MessageRecord;
use crate::usage::normalize::kimi_code::kimi_message;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "type")]
    kind: String,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_other: Option<u64>,
    output: Option<u64>,
    input_cache_read: Option<u64>,
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
    let mut has_header = false;
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
            if tag.kind != "metadata" || !matches!(header.protocol_version.as_str(), "1.4" | "1.5")
            {
                return Err(invalid());
            }
            has_header = true;
            continue;
        }
        match tag.kind.as_str() {
            "usage.record" => {
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
                parsed.messages.push(kimi_message(
                    session_id,
                    agent,
                    ordinal,
                    record.time.filter(|time| *time >= 0),
                    record.model,
                    &counts,
                ));
                parsed.counts.push(counts);
            }
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
            | "tools.set_active_tools" => {}
            _ => parsed.unparsed += 1,
        }
    }
    if !has_header {
        return Err(invalid());
    }
    Ok(parsed)
}
