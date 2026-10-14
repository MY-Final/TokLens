//! Combine all metadata sources before atomically importing a stable snapshot.

use super::{PARSER_VERSION, agent_host, database, events, queries};
use crate::collectors::files::{fingerprint, home_dir};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::normalize::copilot;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct CopilotCollector {
    database: PathBuf,
    agent_host: Option<PathBuf>,
}

impl CopilotCollector {
    pub fn default_database() -> Option<PathBuf> {
        resolve_database(std::env::var_os("COPILOT_HOME"), home_dir())
    }

    pub fn new(database: impl Into<PathBuf>) -> Self {
        Self {
            database: database.into(),
            agent_host: None,
        }
    }
    pub fn with_agent_host(mut self, code_data: Option<PathBuf>) -> Self {
        self.agent_host = code_data;
        self
    }
    pub fn default_agent_host() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Code/agentSessionData"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }
    pub fn database(&self) -> &Path {
        &self.database
    }

    pub fn sync(
        &self,
        store: &UsageStore,
        _options: &SyncOptions,
    ) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        let mut sources = Vec::new();
        let mut sessions = BTreeMap::new();
        let database_present = self.database.try_exists()?;
        let host_missing = self
            .agent_host
            .as_ref()
            .map(|p| p.try_exists())
            .transpose()?
            .is_some_and(|exists| !exists);
        let state = self.database.parent().map(|p| p.join("session-state"));
        let event_paths = match state {
            Some(ref root) => children(root, "events.jsonl")?,
            None => Vec::new(),
        };
        let host_paths = match &self.agent_host {
            Some(root) => children(root, "session.db")?,
            None => Vec::new(),
        };
        let mut observed = BTreeMap::new();
        for path in event_paths
            .iter()
            .chain(&host_paths)
            .chain(database_present.then_some(&self.database))
        {
            observed.insert(path.clone(), database::signature(path)?);
        }
        if database_present {
            let conn = database::open(&self.database)?;
            let path = self.database.to_string_lossy().to_string();
            let snapshot = queries::snapshot(&conn, &path)?;
            sources.push(source(
                &self.database,
                SourceFileKind::Sqlite,
                snapshot.unparsed,
            )?);
            report.unparsed_lines += snapshot.unparsed;
            for entry in snapshot.sessions {
                sessions.insert(entry.session.client_session_id.clone(), entry);
            }
        }
        let mut sdk: BTreeMap<String, events::SdkSession> = BTreeMap::new();
        for path in &event_paths {
            let entry = events::read(path)?;
            let id = entry.import.session.client_session_id.clone();
            // The same SDK session id may appear in mirror event directories. That is
            // one snapshot, never two token streams: keep the newer event-time mirror,
            // and on a tie keep the first path in sorted order for determinism.
            let replace = sdk.get(&id).is_none_or(|existing| {
                entry.import.session.ended_at_ms > existing.import.session.ended_at_ms
            });
            if replace {
                sdk.insert(id, entry);
            }
            sources.push(source(path, SourceFileKind::Jsonl, 0)?);
        }
        let mut turns = Vec::new();
        {
            for path in &host_paths {
                let found = agent_host::read(path)?;
                if !found.is_empty() {
                    sources.push(source(path, SourceFileKind::Sqlite, 0)?);
                }
                turns.extend(found);
            }
        }
        let mut host = BTreeMap::<String, Vec<agent_host::Turn>>::new();
        for turn in agent_host::dedup(turns)? {
            host.entry(turn.sdk_id.clone()).or_default().push(turn);
        }
        for (id, mut entry) in sdk {
            if let Some(turns) = host.remove(&id) {
                let complete = turns.len() == entry.user_events.len()
                    && turns
                        .iter()
                        .all(|t| entry.user_events.contains(&t.event_id));
                let totals: Vec<_> = turns
                    .iter()
                    .flat_map(|t| t.totals.iter().copied())
                    .collect();
                copilot::agent_host_totals(&mut entry.import.session, &totals, complete);
                for turn in turns {
                    if !entry.user_events.contains(&turn.event_id) {
                        return Err(SyncError::ChangedDuringScan);
                    }
                    if let Some(message) = entry
                        .import
                        .messages
                        .iter_mut()
                        .find(|m| m.dedup_key == format!("api:{}", turn.api_id))
                    {
                        message.usage = copilot::agent_host_usage(turn.last_call);
                        entry
                            .import
                            .message_source_paths
                            .insert(message.dedup_key.clone(), turn.source_path);
                    }
                }
                // SDK and outer Agent Host IDs refer to one logical session. Host
                // totals are authoritative; never sum them with CLI or call rows.
                sessions.insert(id, entry.import);
            } else if (!database_present || host_missing)
                && store.session_tokens(Client::Copilot, &id)?.is_some()
            {
                // An absent source is not a correction to zero/unknown. Preserve
                // the previously imported session, including its native ledger.
                sessions.remove(&id);
            } else if sessions.get(&id).is_none_or(|e| e.messages.is_empty()) {
                sessions.insert(id, entry.import);
            }
        }
        // Host turns whose SDK events are gone cannot be reliably attributed, so they
        // are skipped rather than fabricated. One or more orphan host sessions must not
        // block the files that can be linked; record them as a diagnostic instead.
        report.files_without_identity += host.len();
        if sources.is_empty() {
            report.root_missing = true;
            return Ok(report);
        }
        let sessions: Vec<_> = sessions.into_values().collect();
        if self.database.try_exists()? != database_present
            || state
                .as_ref()
                .map(|root| children(root, "events.jsonl"))
                .transpose()?
                .unwrap_or_default()
                != event_paths
            || self
                .agent_host
                .as_ref()
                .map(|root| children(root, "session.db"))
                .transpose()?
                .unwrap_or_default()
                != host_paths
        {
            return Err(SyncError::ChangedDuringScan);
        }
        for (path, stamp) in observed {
            if database::signature(&path)? != stamp {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        let imported = if !database_present || host_missing {
            store.merge_client_snapshot(Client::Copilot, &sources, &sessions)?
        } else {
            store.replace_client_snapshot(Client::Copilot, &sources, &sessions)?
        };
        report.files_seen = sources.len();
        report.files_parsed = sources.len();
        report.sessions_imported = sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}

fn children(root: &Path, filename: &str) -> Result<Vec<PathBuf>, SyncError> {
    if !root.try_exists()? {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).map_err(SyncError::Directory)? {
        let entry = entry.map_err(SyncError::Directory)?;
        if entry.file_type()?.is_dir() {
            let path = entry.path().join(filename);
            if path.try_exists()? && std::fs::symlink_metadata(&path)?.file_type().is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}
fn source(path: &Path, kind: SourceFileKind, unparsed: usize) -> Result<SourceFile, SyncError> {
    let (size, mtime) = fingerprint(path)?;
    Ok(SourceFile {
        client: Client::Copilot,
        path: path.to_string_lossy().into(),
        kind,
        size_bytes: Some(size),
        mtime_ms: Some(mtime),
        content_offset: 0,
        parser_version: PARSER_VERSION,
        last_scan_ms: Some(now_ms()),
        unparsed_records: unparsed as i64,
    })
}

pub(super) fn resolve_database(
    config: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    config
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".copilot")))
        .map(|root| root.join("session-store.db"))
}
