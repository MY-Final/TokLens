//! Fingerprint both the authoritative ledger and its metadata sidecar, atomically replace.
use super::{PARSER_VERSION, discovery, parser};
use crate::collectors::files::{fingerprint, home_dir};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SessionImport, SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct GrokCollector {
    root: PathBuf,
}

impl GrokCollector {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> Option<PathBuf> {
        discovery::resolve_root(std::env::var_os("GROK_HOME"), home_dir())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sync(&self, store: &UsageStore, options: &SyncOptions) -> Result<SyncReport, SyncError> {
        self.sync_before_commit(store, options, || {})
    }

    fn sync_before_commit(
        &self,
        store: &UsageStore,
        options: &SyncOptions,
        before_commit: impl FnOnce(),
    ) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.root.try_exists()? {
            report.root_missing = true;
            return Ok(report);
        }
        let paths = discovery::files(&self.root, &mut report)?;
        let mut sources = Vec::new();
        let mut unchanged = !options.force_rescan;
        for path in &paths {
            let (size, mtime) = fingerprint(path)?;
            let path = path.to_string_lossy().to_string();
            let old = store.source_file(Client::Grok, &path)?;
            unchanged &= old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(size)
                    && old.mtime_ms == Some(mtime)
                    && old.parser_version == PARSER_VERSION
            });
            sources.push(SourceFile {
                client: Client::Grok,
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
        unchanged &= store.source_paths(Client::Grok)?
            == sources
                .iter()
                .map(|source| source.path.clone())
                .collect::<Vec<_>>();
        if unchanged {
            report.files_skipped_unchanged = paths.len();
            return Ok(report);
        }
        let mut sessions: BTreeMap<String, SessionImport> = BTreeMap::new();
        for path in paths
            .iter()
            .filter(|path| path.file_name().is_some_and(|name| name == "usage.json"))
        {
            let snapshot = match parser::read_snapshot(path) {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    report.files_failed += 1;
                    return Ok(report);
                }
            };
            let summary_path = path.with_file_name("summary.json");
            let summary = if paths.binary_search(&summary_path).is_ok() {
                match parser::read_summary(&summary_path) {
                    Ok(summary) => summary,
                    Err(_) => {
                        report.files_failed += 1;
                        return Ok(report);
                    }
                }
            } else {
                parser::Summary::default()
            };
            let import =
                match parser::import(snapshot, summary, path.to_string_lossy().into_owned()) {
                    Ok(import) => import,
                    Err(_) => {
                        report.files_failed += 1;
                        return Ok(report);
                    }
                };
            if let Some(old) = sessions.get(&import.session.client_session_id) {
                if old.session.ended_at_ms == import.session.ended_at_ms {
                    // Same session mirror must agree; don't merge or add two absolute snapshots.
                    if old.session.usage != import.session.usage
                        || old.messages.len() != import.messages.len()
                        || old
                            .messages
                            .iter()
                            .zip(&import.messages)
                            .any(|(a, b)| a.dedup_key != b.dedup_key || a.usage != b.usage)
                    {
                        report.files_failed += 1;
                        return Ok(report);
                    }
                    continue;
                }
                if old.session.ended_at_ms > import.session.ended_at_ms {
                    continue;
                }
            }
            sessions.insert(import.session.client_session_id.clone(), import);
        }
        report.files_parsed = paths.len();
        before_commit();
        verify_sources(&self.root, &paths, &sources)?;
        let sessions: Vec<_> = sessions.into_values().collect();
        let result = store.replace_client_snapshot(Client::Grok, &sources, &sessions)?;
        report.sessions_imported = sessions.len();
        report.messages_inserted = result.messages_inserted;
        report.messages_pruned = result.messages_pruned;
        Ok(report)
    }

    #[cfg(test)]
    pub(super) fn sync_with_mutation(
        &self,
        store: &UsageStore,
        before_commit: impl FnOnce(),
    ) -> Result<SyncReport, SyncError> {
        self.sync_before_commit(store, &SyncOptions { force_rescan: true }, before_commit)
    }
}

fn verify_sources(root: &Path, paths: &[PathBuf], sources: &[SourceFile]) -> Result<(), SyncError> {
    if discovery::files(root, &mut SyncReport::default())? != paths {
        return Err(SyncError::ChangedDuringScan);
    }
    for (path, source) in paths.iter().zip(sources) {
        let (size, mtime) = fingerprint(path)?;
        if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
            return Err(SyncError::ChangedDuringScan);
        }
    }
    Ok(())
}
