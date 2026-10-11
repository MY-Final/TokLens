//! Cross-file message deduplication and complete session rollups.

use super::parser::{Record, Usage};
use crate::usage::normalize::{
    ClaudeMessage, ClaudeSessionRollup, claude_code_message, claude_code_session,
};
use crate::usage::store::SessionImport;
use crate::usage::time::parse_iso8601_utc_ms;
use crate::usage::tokens::RawTokenCounts;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Rollup {
    records: BTreeMap<String, (Record, String)>,
    cwd: Option<String>,
    start: Option<String>,
    end: Option<String>,
    source_path: String,
}

impl Rollup {
    pub(super) fn push(&mut self, record: Record, path: &str) {
        if self.source_path.is_empty() {
            self.source_path = path.to_string();
        }
        if let Some(timestamp) = record
            .timestamp
            .as_deref()
            .filter(|value| parse_iso8601_utc_ms(value).is_some())
        {
            let time_ms = parse_iso8601_utc_ms(timestamp);
            if self
                .start
                .as_deref()
                .and_then(parse_iso8601_utc_ms)
                .is_none_or(|start| time_ms < Some(start))
            {
                self.start = Some(timestamp.to_string());
            }
            if self
                .end
                .as_deref()
                .and_then(parse_iso8601_utc_ms)
                .is_none_or(|end| time_ms > Some(end))
            {
                self.end = Some(timestamp.to_string());
            }
        }
        if record.cwd.is_some() {
            self.cwd = record.cwd.clone();
        }
        if record.kind.as_deref() != Some("assistant") {
            return;
        }
        let Some(id) = record
            .message
            .as_ref()
            .and_then(|message| message.id.clone())
        else {
            return;
        };
        let replace = self.records.get(&id).is_none_or(|(existing, _)| {
            record.timestamp.as_deref().and_then(parse_iso8601_utc_ms)
                >= existing.timestamp.as_deref().and_then(parse_iso8601_utc_ms)
        });
        if replace {
            self.records.insert(id, (record, path.to_string()));
        }
    }

    pub(super) fn finish(self, session_id: &str) -> SessionImport {
        let mut counts = Vec::new();
        let mut messages = Vec::new();
        let mut message_source_paths = BTreeMap::new();
        for (id, (record, path)) in self.records {
            let Some(message) = record.message else {
                continue;
            };
            let usage = message
                .usage
                .as_ref()
                .map_or(RawTokenCounts::default(), Usage::counts);
            counts.push(usage);
            message_source_paths.insert(id.clone(), path);
            messages.push(claude_code_message(&ClaudeMessage {
                session_id,
                message_id: &id,
                timestamp: record.timestamp.as_deref(),
                model: message.model.as_deref(),
                cwd: record.cwd.as_deref(),
                is_sidechain: record.is_sidechain,
                usage,
            }));
        }
        messages.sort_by(|left, right| {
            (left.created_at_ms, &left.dedup_key).cmp(&(right.created_at_ms, &right.dedup_key))
        });
        let model = messages
            .iter()
            .rev()
            .find_map(|message| message.model.as_deref());
        let session = claude_code_session(&ClaudeSessionRollup {
            session_id,
            cwd: self.cwd.as_deref(),
            model,
            started_at: self.start.as_deref(),
            ended_at: self.end.as_deref(),
            message_count: messages.len(),
            usage: crate::usage::normalize::sum_complete_counts(&counts),
        });
        SessionImport {
            session,
            messages,
            source_path: self.source_path,
            message_source_paths,
        }
    }
}
