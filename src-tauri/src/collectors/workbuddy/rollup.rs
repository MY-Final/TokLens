//! Cross-file native item identity and metadata-only session rollups.
use super::parser::Record;
use crate::usage::client::Client;
use crate::usage::normalize::request_ledger::{self, RequestMeta, SessionMeta};
use crate::usage::store::SessionImport;
use crate::usage::tokens::RawTokenCounts;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Rollup {
    requests: BTreeMap<String, (Record, RawTokenCounts, String)>,
    project: Option<String>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    source: String,
}
impl Rollup {
    pub(super) fn push(&mut self, record: Record, path: &str) {
        if self.source.is_empty() {
            self.source = path.to_string();
        }
        if let Some(time) = record.timestamp {
            self.start_ms = Some(self.start_ms.map_or(time, |old| old.min(time)));
            self.end_ms = Some(self.end_ms.map_or(time, |old| old.max(time)));
        }
        if self.project.is_none() {
            self.project = record.cwd.clone();
        }
        let Some(raw) = record.counts() else {
            return;
        };
        let Some(id) = record.id.clone() else {
            return;
        };
        if self
            .requests
            .get(&id)
            .is_none_or(|(old, _, _)| record.timestamp >= old.timestamp)
        {
            self.requests.insert(id, (record, raw, path.to_string()));
        }
    }
    pub(super) fn finish(self, id: &str, partial: bool) -> SessionImport {
        let mut messages = Vec::new();
        let mut counts = Vec::new();
        let mut message_source_paths = BTreeMap::new();
        for (key, (record, raw, path)) in self.requests {
            let provider = record.provider_data;
            let meta = RequestMeta {
                key: key.clone(),
                model: provider.as_ref().and_then(|p| p.model.clone()),
                agent: provider.and_then(|p| p.agent),
                created_ms: record.timestamp,
                completed_ms: None,
                sidechain: record.is_sidechain,
                source_ref: format!("item:{key}"),
            };
            messages.push(request_ledger::request(Client::WorkBuddy, id, meta, &raw));
            counts.push(raw);
            message_source_paths.insert(key, path);
        }
        let meta = SessionMeta {
            id: id.to_string(),
            parent: None,
            project: self.project,
            start_ms: self.start_ms,
            end_ms: self.end_ms,
        };
        SessionImport {
            session: request_ledger::session(Client::WorkBuddy, meta, &messages, &counts, partial),
            messages,
            source_path: self.source,
            message_source_paths,
        }
    }
}
