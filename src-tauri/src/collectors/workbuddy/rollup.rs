//! Cross-file native item identity and metadata-only session rollups.
use super::parser::Record;
use crate::usage::client::Client;
use crate::usage::normalize::request_ledger::{self, RequestMeta, SessionMeta};
use crate::usage::store::SessionImport;
use crate::usage::tokens::RawTokenCounts;
use std::collections::{BTreeMap, BTreeSet};

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
    pub(super) fn finish(self, id: &str, dirty_sources: &BTreeSet<String>) -> SessionImport {
        // Only a file that actually contributed a parsed record to this session and
        // contained a corrupt line makes the session partial; a bad line elsewhere
        // must not pollute unrelated sessions.
        let partial = self
            .requests
            .values()
            .any(|(_, _, path)| dirty_sources.contains(path));
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
            let mut message = request_ledger::request(Client::WorkBuddy, id, meta, &raw);
            // No client-reported total: a total derived from complete buckets is a
            // deterministic conversion, not measured.
            request_ledger::classify_request_source(&mut message);
            messages.push(message);
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
        let mut session =
            request_ledger::session(Client::WorkBuddy, meta, &messages, &counts, partial);
        request_ledger::classify_session_source(&mut session, &messages);
        SessionImport {
            session,
            messages,
            source_path: self.source,
            message_source_paths,
        }
    }
}
