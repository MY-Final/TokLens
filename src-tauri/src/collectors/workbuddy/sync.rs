//! Detect changed transcripts, then import one complete cross-file snapshot.

use super::{PARSER_VERSION, parser, rollup::Rollup};
use crate::collectors::files::{fingerprint, home_dir, jsonl_files};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct WorkBuddyCollector {
    root: PathBuf,
}

impl WorkBuddyCollector {
    pub fn default_root() -> Option<PathBuf> {
        resolve_root(std::env::var_os("WORKBUDDY_CONFIG_DIR"), home_dir())
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
        let paths = jsonl_files(&self.root, &mut report)?;
        let mut sources = Vec::new();
        let mut unchanged = !options.force_rescan;
        for path in &paths {
            let (size, mtime) = fingerprint(path)?;
            let path = path.to_string_lossy().to_string();
            let old = store.source_file(Client::WorkBuddy, &path)?;
            unchanged &= old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(size)
                    && old.mtime_ms == Some(mtime)
                    && old.parser_version == PARSER_VERSION
            });
            sources.push(SourceFile {
                client: Client::WorkBuddy,
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
        unchanged &= store.source_paths(Client::WorkBuddy)?
            == sources
                .iter()
                .map(|source| source.path.clone())
                .collect::<Vec<_>>();
        if unchanged {
            report.files_skipped_unchanged = paths.len();
            return Ok(report);
        }
        let mut sessions: BTreeMap<String, Rollup> = BTreeMap::new();
        // Files that contained at least one corrupt line; only sessions which actually
        // drew records from them are marked partial.
        let mut dirty_sources: BTreeSet<String> = BTreeSet::new();
        for (path, source) in paths.iter().zip(&mut sources) {
            let parsed = parser::read(path)?;
            report.files_parsed += 1;
            report.unparsed_lines += parsed.unparsed;
            source.unparsed_records = parsed.unparsed as i64;
            if parsed.unparsed > 0 {
                dirty_sources.insert(source.path.clone());
            }
            for record in parsed.records {
                if let Some(id) = record.session_id.clone() {
                    sessions.entry(id).or_default().push(record, &source.path);
                }
            }
        }
        // Never publish a snapshot that raced an append, replacement or newly discovered file.
        if paths != jsonl_files(&self.root, &mut SyncReport::default())? {
            return Err(SyncError::ChangedDuringScan);
        }
        for (path, source) in paths.iter().zip(&sources) {
            let (size, mtime) = fingerprint(path)?;
            if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        let sessions = sessions
            .into_iter()
            .map(|(id, rollup)| rollup.finish(&id, &dirty_sources))
            .collect::<Vec<_>>();
        let imported = store.replace_client_snapshot(Client::WorkBuddy, &sources, &sessions)?;
        report.sessions_imported = sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}

pub(super) fn resolve_root(
    config: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    config
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".workbuddy")))
        .map(|root| root.join("projects"))
}
