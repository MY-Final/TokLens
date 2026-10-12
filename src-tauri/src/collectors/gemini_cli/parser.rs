//! Replay journal operations using metadata-only deserialization.
use crate::usage::normalize::gemini_cli::GeminiUsage;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub session_id: Option<String>,
    project_hash: Option<String>,
    pub start_time: Option<String>,
    pub last_updated: Option<String>,
    pub kind: Option<String>,
    pub directories: Option<Vec<String>>,
    messages: Option<Vec<Record>>,
}

#[derive(Deserialize)]
pub(super) struct Record {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub timestamp: Option<String>,
    pub model: Option<String>,
    pub tokens: Option<GeminiUsage>,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(flatten)]
    metadata: Metadata,
    #[serde(rename = "$set")]
    patch: Option<Metadata>,
    #[serde(rename = "$rewindTo")]
    rewind: Option<String>,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    model: Option<String>,
    tokens: Option<GeminiUsage>,
}

pub(super) struct ParsedFile {
    pub metadata: Metadata,
    pub records: Vec<Record>,
    pub unparsed: usize,
    pub nonempty: bool,
}

fn invalid() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unsupported Gemini CLI journal",
    )
}

fn push(
    record: Record,
    order: &mut Vec<String>,
    records: &mut BTreeMap<String, Record>,
    unparsed: &mut usize,
) -> std::io::Result<()> {
    if record.id.is_empty() {
        return Err(invalid());
    }
    if !matches!(record.kind.as_str(), "user" | "gemini" | "info" | "error") {
        *unparsed += 1;
    }
    if record.tokens.as_ref().is_some_and(|tokens| !tokens.valid()) {
        return Err(invalid());
    }
    if !records.contains_key(&record.id) {
        order.push(record.id.clone());
    }
    // pushMessage writes the entire updated record, not a field-level patch.
    records.insert(record.id.clone(), record);
    Ok(())
}

fn update(target: &mut Metadata, patch: Metadata) {
    if patch.session_id.is_some() {
        target.session_id = patch.session_id;
    }
    if patch.project_hash.is_some() {
        target.project_hash = patch.project_hash;
    }
    if patch.start_time.is_some() {
        target.start_time = patch.start_time;
    }
    if patch.last_updated.is_some() {
        target.last_updated = patch.last_updated;
    }
    if patch.kind.is_some() {
        target.kind = patch.kind;
    }
    if patch.directories.is_some() {
        target.directories = patch.directories;
    }
}

pub(super) fn read(path: &Path) -> std::io::Result<ParsedFile> {
    read_from(BufReader::new(std::fs::File::open(path)?))
}

pub(super) fn read_from(mut reader: impl BufRead) -> std::io::Result<ParsedFile> {
    let mut parsed = ParsedFile {
        metadata: Metadata::default(),
        records: Vec::new(),
        unparsed: 0,
        nonempty: false,
    };
    let mut records = BTreeMap::new();
    let mut order = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        parsed.nonempty = true;
        // A damaged row may be a replacement or rewind. Never publish a partial replay.
        let mut row: Envelope = serde_json::from_str(&line).map_err(|_| invalid())?;
        if let Some(rewind) = row.rewind {
            let index = order.iter().position(|id| id == &rewind).unwrap_or(0);
            for id in order.drain(index..) {
                records.remove(&id);
            }
        } else if let Some(id) = row.id {
            push(
                Record {
                    id,
                    kind: row.kind.ok_or_else(invalid)?,
                    timestamp: row.timestamp,
                    model: row.model,
                    tokens: row.tokens,
                },
                &mut order,
                &mut records,
                &mut parsed.unparsed,
            )?;
        } else if let Some(mut patch) = row.patch {
            if let Some(messages) = patch.messages.take() {
                // $set.messages replaces the entire conversation (e.g. compaction).
                order.clear();
                records.clear();
                for message in messages {
                    push(message, &mut order, &mut records, &mut parsed.unparsed)?;
                }
            }
            update(&mut parsed.metadata, patch);
        } else if row.metadata.session_id.is_some() && row.metadata.project_hash.is_some() {
            if let Some(messages) = row.metadata.messages.take() {
                for message in messages {
                    push(message, &mut order, &mut records, &mut parsed.unparsed)?;
                }
            }
            update(&mut parsed.metadata, row.metadata);
        } else {
            parsed.unparsed += 1;
        }
    }
    if parsed.nonempty
        && parsed
            .metadata
            .session_id
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(invalid());
    }
    // A new operation-only representation must not silently erase previous usage.
    if parsed.unparsed > 0 && records.is_empty() {
        return Err(invalid());
    }
    parsed.records = order
        .into_iter()
        .filter_map(|id| records.remove(&id))
        .collect();
    Ok(parsed)
}
