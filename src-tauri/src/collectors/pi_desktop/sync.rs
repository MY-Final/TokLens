//! Detect changed transcripts, then import one complete cross-file snapshot.

use super::{PARSER_VERSION, parser};
use crate::collectors::files::{discover_files, fingerprint, home_dir};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct PiDesktopCollector {
    root: PathBuf,
}

impl PiDesktopCollector {
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("PI_DESKTOP_SESSIONS_DIR")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".pi-desktop").join("sessions")))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sync(&self, store: &UsageStore, options: &SyncOptions) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.root.try_exists()? {
            report.root_missing = true;
            return Ok(report);
        }
        let paths = discover_files(&self.root, &mut report, parser::accepts)?;
        let mut sources = Vec::new();
        let mut unchanged = !options.force_rescan;
        for path in &paths {
            let (size, mtime) = fingerprint(path)?;
            let path = path.to_string_lossy().to_string();
            let old = store.source_file(Client::PiDesktop, &path)?;
            unchanged &= old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(size)
                    && old.mtime_ms == Some(mtime)
                    && old.parser_version == PARSER_VERSION
            });
            sources.push(SourceFile {
                client: Client::PiDesktop,
                path,
                kind: SourceFileKind::Jsonl,
                size_bytes: Some(size),
                mtime_ms: Some(mtime),
                content_offset: size,
                parser_version: PARSER_VERSION,
                last_scan_ms: Some(now_ms()),
                unparsed_records: 0,
            });
        }
        unchanged &= store.source_paths(Client::PiDesktop)?
            == sources
                .iter()
                .map(|source| source.path.clone())
                .collect::<Vec<_>>();
        if unchanged {
            report.files_skipped_unchanged = paths.len();
            return Ok(report);
        }
        let mut metas = BTreeMap::new();
        let mut requests: BTreeMap<String, (super::parser::Candidate, String)> = BTreeMap::new();
        for (path, source) in paths.iter().zip(&mut sources) {
            let parsed = parser::read(path)?;
            report.files_parsed += 1;
            report.unparsed_lines += parsed.unparsed;
            source.unparsed_records = parsed.unparsed as i64;
            if metas.contains_key(&parsed.meta.id) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "duplicate Pi Desktop session identity",
                )
                .into());
            }
            for candidate in parsed.requests {
                // Operation UUID is global. Legacy message ids are scoped to their session.
                let key = if candidate.message.dedup_key.starts_with("operation:") {
                    candidate.message.dedup_key.clone()
                } else {
                    format!("{}:{}", parsed.meta.id, candidate.message.dedup_key)
                };
                if requests.get(&key).is_none_or(|(old, _)| {
                    candidate.message.usage.total_tokens > old.message.usage.total_tokens
                }) {
                    requests.insert(key, (candidate, source.path.clone()));
                }
            }
            metas.insert(
                parsed.meta.id.clone(),
                (parsed.meta, source.path.clone(), parsed.unparsed > 0),
            );
        }
        let mut details: BTreeMap<String, Vec<_>> = BTreeMap::new();
        for (candidate, path) in requests.into_values() {
            details
                .entry(candidate.message.client_session_id.clone())
                .or_default()
                .push((candidate, path));
        }
        let mut sessions = Vec::new();
        for (id, (meta, path, partial)) in metas {
            let mut messages = Vec::new();
            let mut counts = Vec::new();
            let mut message_source_paths = BTreeMap::new();
            for (candidate, source) in details.remove(&id).unwrap_or_default() {
                message_source_paths.insert(candidate.message.dedup_key.clone(), source);
                counts.push(candidate.raw);
                messages.push(candidate.message);
            }
            sessions.push(crate::usage::store::SessionImport {
                session: crate::usage::normalize::request_ledger::session(
                    Client::PiDesktop,
                    meta,
                    &messages,
                    &counts,
                    partial,
                ),
                messages,
                source_path: path,
                message_source_paths,
            });
        }
        // Never publish a snapshot that raced an append, replacement or newly discovered file.
        if paths != discover_files(&self.root, &mut SyncReport::default(), parser::accepts)? {
            return Err(SyncError::ChangedDuringScan);
        }
        for (path, source) in paths.iter().zip(&sources) {
            let (size, mtime) = fingerprint(path)?;
            if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        let imported = store.replace_client_snapshot(Client::PiDesktop, &sources, &sessions)?;
        report.sessions_imported = sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}
