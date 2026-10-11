//! Detect changed transcripts, then import one complete cross-file snapshot.

use super::{PARSER_VERSION, format, parser};
use crate::collectors::files::{discover_files, fingerprint, home_dir};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct DshCollector {
    root: PathBuf,
}

impl DshCollector {
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("DSH_SESSIONS_DIR")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".dsh").join("sessions")))
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
        let paths = format::select(discover_files(&self.root, &mut report, format::accepts)?);
        let mut sources = Vec::new();
        let mut unchanged = !options.force_rescan;
        for path in &paths {
            let (size, mtime) = fingerprint(path)?;
            let path = path.to_string_lossy().to_string();
            let old = store.source_file(Client::Dsh, &path)?;
            unchanged &= old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(size)
                    && old.mtime_ms == Some(mtime)
                    && old.parser_version == PARSER_VERSION
            });
            sources.push(SourceFile {
                client: Client::Dsh,
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
        let mut source_paths = sources
            .iter()
            .map(|source| source.path.clone())
            .collect::<Vec<_>>();
        // PathBuf order on Windows differs from SQLite's string order for nested paths.
        source_paths.sort();
        unchanged &= store.source_paths(Client::Dsh)? == source_paths;
        if unchanged {
            report.files_skipped_unchanged = paths.len();
            return Ok(report);
        }
        let mut sessions: BTreeMap<String, crate::usage::store::SessionImport> = BTreeMap::new();
        let mut successful_sources = Vec::new();
        for (path, source) in paths.iter().zip(&mut sources) {
            let parsed = match parser::parse_file(path) {
                Ok(parsed) => parsed,
                Err(_) => {
                    report.files_failed += 1;
                    continue;
                }
            };
            report.files_parsed += 1;
            report.unparsed_lines += parsed.unparsed;
            source.unparsed_records = parsed.unparsed as i64;
            successful_sources.push(source.clone());
            let id = parsed.import.session.client_session_id.clone();
            let replace = sessions.get(&id).is_none_or(|existing| {
                parsed.import.session.ended_at_ms >= existing.session.ended_at_ms
            });
            if replace {
                sessions.insert(id, parsed.import);
            }
        }
        // Never publish a snapshot that raced an append, replacement or newly discovered file.
        if paths
            != format::select(discover_files(
                &self.root,
                &mut SyncReport::default(),
                format::accepts,
            )?)
        {
            return Err(SyncError::ChangedDuringScan);
        }
        for (path, source) in paths.iter().zip(&sources) {
            let (size, mtime) = fingerprint(path)?;
            if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        let sessions = sessions.into_values().collect::<Vec<_>>();
        let imported = if report.files_failed == 0 {
            store.replace_client_snapshot(Client::Dsh, &successful_sources, &sessions)?
        } else {
            store.merge_client_snapshot(Client::Dsh, &successful_sources, &sessions)?
        };
        report.sessions_imported = sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}
