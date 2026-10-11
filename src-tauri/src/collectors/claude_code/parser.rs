//! Deserialize a whitelist: message content is skipped by serde.

use crate::collectors::files::valid_counts;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Record {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub is_sidechain: bool,
    pub message: Option<Message>,
}

#[derive(Deserialize)]
pub(super) struct Message {
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Default, Deserialize)]
pub(super) struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

impl Usage {
    pub(super) fn counts(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_input_tokens,
            cache_write: self.cache_creation_input_tokens,
            ..Default::default()
        }
    }
}

pub(super) struct ParsedFile {
    pub records: Vec<Record>,
    pub unparsed: usize,
}

pub(super) fn parse_file(path: &Path) -> std::io::Result<ParsedFile> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    let mut parsed = ParsedFile {
        records: Vec::new(),
        unparsed: 0,
    };
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let record: Record = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(_) if !line.ends_with('\n') => {
                // An active writer may not have completed this line. Do not advance the watermark.
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "incomplete transcript record",
                ));
            }
            Err(_) => {
                parsed.unparsed += 1;
                continue;
            }
        };
        match record.kind.as_deref() {
            Some("assistant" | "user") => {
                if record.session_id.as_deref().is_none_or(str::is_empty) {
                    parsed.unparsed += 1;
                    continue;
                }
                if record.kind.as_deref() == Some("assistant") {
                    let Some(message) = record.message.as_ref() else {
                        parsed.unparsed += 1;
                        continue;
                    };
                    if message.model.as_deref() == Some("<synthetic>") {
                        continue;
                    }
                    if message.id.as_deref().is_none_or(str::is_empty)
                        || !valid_counts(
                            &message
                                .usage
                                .as_ref()
                                .map_or(RawTokenCounts::default(), Usage::counts),
                        )
                    {
                        parsed.unparsed += 1;
                        continue;
                    }
                }
                parsed.records.push(record);
            }
            Some(
                "system"
                | "progress"
                | "summary"
                | "file-history-snapshot"
                | "queue-operation"
                | "last-prompt"
                | "mode"
                | "permission-mode"
                | "atis-latch"
                | "attachment"
                | "cost-state"
                | "ai-title"
                | "custom-title"
                | "file-history-delta",
            ) => {}
            _ => parsed.unparsed += 1,
        }
    }
    Ok(parsed)
}
