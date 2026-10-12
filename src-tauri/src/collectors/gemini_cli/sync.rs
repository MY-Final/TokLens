//! Validate all source fingerprints before atomically replacing a journal snapshot.
use super::{PARSER_VERSION, discovery, parser};
use crate::collectors::files::fingerprint;
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::{
    client::Client,
    normalize::{
        gemini_cli,
        request_ledger::{RequestMeta, SessionMeta},
    },
    store::{SessionImport, SourceFile, SourceFileKind, UsageStore},
    time::{now_ms, parse_iso8601_utc_ms},
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct GeminiCliCollector {
    root: PathBuf,
}

impl GeminiCliCollector {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn default_root() -> Option<PathBuf> {
        discovery::default_root()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sync(&self, store: &UsageStore, options: &SyncOptions) -> Result<SyncReport, SyncError> {
        self.sync_before_commit(store, options, || {})
    }

    fn source_paths(&self, journals: &[PathBuf]) -> Result<Vec<PathBuf>, SyncError> {
        let mut paths: BTreeSet<PathBuf> = journals.iter().cloned().collect();
        for journal in journals {
            if let Some(marker) = discovery::project_marker(&self.root, journal)? {
                paths.insert(marker);
            }
        }
        Ok(paths.into_iter().collect())
    }

    pub(super) fn sync_before_commit(
        &self,
        store: &UsageStore,
        options: &SyncOptions,
        before_commit: impl FnOnce(),
    ) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.root.try_exists()? || !self.root.join("tmp").try_exists()? {
            report.root_missing = true;
            return Ok(report);
        }
        let journals = discovery::discover(&self.root, &mut report)?;
        let paths = self.source_paths(&journals)?;
        let mut sources = Vec::new();
        let mut unchanged = !options.force_rescan;
        for path in &paths {
            let (size, mtime) = fingerprint(path)?;
            let path = path.to_string_lossy().to_string();
            let old = store.source_file(Client::GeminiCli, &path)?;
            // An emptied existing journal may be a rewrite in progress.
            if size == 0
                && old
                    .as_ref()
                    .is_some_and(|old| old.size_bytes.is_some_and(|size| size > 0))
            {
                return Err(SyncError::ChangedDuringScan);
            }
            unchanged &= old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(size)
                    && old.mtime_ms == Some(mtime)
                    && old.parser_version == PARSER_VERSION
            });
            sources.push(SourceFile {
                client: Client::GeminiCli,
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
        unchanged &= store.source_paths(Client::GeminiCli)?
            == sources
                .iter()
                .map(|source| source.path.clone())
                .collect::<Vec<_>>();
        if unchanged {
            report.files_skipped_unchanged = journals.len();
            return Ok(report);
        }
        let mut sessions: BTreeMap<String, SessionImport> = BTreeMap::new();
        for path in &journals {
            let mut parsed = match parser::read(path) {
                Ok(parsed) => parsed,
                Err(_) => {
                    report.files_failed += 1;
                    continue;
                }
            };
            report.files_parsed += 1;
            report.unparsed_lines += parsed.unparsed;
            let source_path = path.to_string_lossy().to_string();
            if !parsed.nonempty
                && store
                    .source_file(Client::GeminiCli, &source_path)?
                    .is_some_and(|old| old.size_bytes.is_some_and(|size| size > 0))
            {
                return Err(SyncError::ChangedDuringScan);
            }
            if let Some(source) = sources.iter_mut().find(|source| source.path == source_path) {
                source.unparsed_records = parsed.unparsed as i64;
            }
            let Some(id) = parsed.metadata.session_id.take() else {
                continue;
            };
            let project = if let Some(directories) = parsed.metadata.directories.take() {
                directories.into_iter().next()
            } else if let Some(marker) = discovery::project_marker(&self.root, path)? {
                if std::fs::metadata(&marker)?.len() > 16384 {
                    return Err(SyncError::Source(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid Gemini project marker",
                    )));
                }
                let value = std::fs::read_to_string(marker)?;
                let value = value.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            } else {
                None
            };
            let sidechain = parsed.metadata.kind.as_deref() == Some("subagent");
            let mut messages = Vec::new();
            let mut message_source_paths = BTreeMap::new();
            for (seq, record) in parsed.records.into_iter().enumerate() {
                if record.kind != "gemini" {
                    continue;
                }
                let key = record.id;
                let meta = RequestMeta {
                    key: key.clone(),
                    model: record.model,
                    agent: None,
                    created_ms: record.timestamp.as_deref().and_then(parse_iso8601_utc_ms),
                    completed_ms: None,
                    sidechain,
                    source_ref: format!("message:{key}"),
                };
                let mut message =
                    gemini_cli::request(&id, meta, &record.tokens.unwrap_or_default());
                message.seq = Some(seq as i64);
                messages.push(message);
                message_source_paths.insert(key, source_path.clone());
            }
            let meta = SessionMeta {
                id: id.clone(),
                parent: None,
                project,
                start_ms: parsed
                    .metadata
                    .start_time
                    .as_deref()
                    .and_then(parse_iso8601_utc_ms),
                end_ms: parsed
                    .metadata
                    .last_updated
                    .as_deref()
                    .and_then(parse_iso8601_utc_ms),
            };
            let session = gemini_cli::session(meta, &messages, parsed.unparsed > 0);
            let import = SessionImport {
                session,
                messages,
                source_path,
                message_source_paths,
            };
            // Mirrors of the same session are snapshots, never independent token streams.
            if let Some(old) = sessions.get(&id)
                && import.session.ended_at_ms == old.session.ended_at_ms
                && !same_snapshot(old, &import)
            {
                report.files_failed += 1;
                continue;
            }
            if sessions
                .get(&id)
                .is_none_or(|old| import.session.ended_at_ms >= old.session.ended_at_ms)
            {
                sessions.insert(id, import);
            }
        }
        // A failed replay may contain an identity rewrite, compaction, or mirror.
        // Without the full journal set, retain every historical row and watermark.
        if report.files_failed > 0 {
            return Ok(report);
        }
        before_commit();
        let new_journals = discovery::discover(&self.root, &mut SyncReport::default())?;
        if journals != new_journals || paths != self.source_paths(&new_journals)? {
            return Err(SyncError::ChangedDuringScan);
        }
        for (path, source) in paths.iter().zip(&sources) {
            let (size, mtime) = fingerprint(path)?;
            if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        let sessions = sessions.into_values().collect::<Vec<_>>();
        let imported = store.replace_client_snapshot(Client::GeminiCli, &sources, &sessions)?;
        report.sessions_imported = sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}

fn same_snapshot(a: &SessionImport, b: &SessionImport) -> bool {
    a.session.project_path == b.session.project_path
        && a.session.started_at_ms == b.session.started_at_ms
        && a.session.model == b.session.model
        && a.session.usage == b.session.usage
        && a.messages.len() == b.messages.len()
        && a.messages.iter().zip(&b.messages).all(|(a, b)| {
            a.dedup_key == b.dedup_key
                && a.model == b.model
                && a.created_at_ms == b.created_at_ms
                && a.is_sidechain == b.is_sidechain
                && a.usage == b.usage
        })
}
