//! Detect changed transcripts, then import one complete cross-file snapshot.

use super::{PARSER_VERSION, discovery, metadata, parser};
use crate::collectors::files::{discover_files, fingerprint, home_dir};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct KimiCodeCollector {
    root: PathBuf,
}

impl KimiCodeCollector {
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("KIMI_CODE_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".kimi-code")))
            .map(|home| home.join("sessions"))
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
        let paths = discover_files(&self.root, &mut report, discovery::accepts)?;
        let mut sources = Vec::new();
        let mut unchanged = !options.force_rescan;
        for path in &paths {
            let (size, mtime) = fingerprint(path)?;
            let path = path.to_string_lossy().to_string();
            let old = store.source_file(Client::KimiCode, &path)?;
            unchanged &= old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(size)
                    && old.mtime_ms == Some(mtime)
                    && old.parser_version == PARSER_VERSION
            });
            sources.push(SourceFile {
                client: Client::KimiCode,
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
        unchanged &= store.source_paths(Client::KimiCode)? == source_paths;
        if unchanged {
            report.files_skipped_unchanged = paths.len();
            return Ok(report);
        }
        let mut sessions = Vec::new();
        for group in discovery::group(&paths).into_values() {
            let meta = metadata::read(&group.state)?;
            let mut messages = Vec::new();
            let mut counts = Vec::new();
            let mut message_source_paths = BTreeMap::new();
            let mut partial = false;
            report.files_parsed += 1;
            for wire in &group.wires {
                let parsed = parser::read(wire, &meta.id)?;
                report.files_parsed += 1;
                report.unparsed_lines += parsed.unparsed;
                partial |= parsed.unparsed > 0;
                let path = wire.to_string_lossy().to_string();
                if let Some(source) = sources.iter_mut().find(|source| source.path == path) {
                    source.unparsed_records = parsed.unparsed as i64;
                }
                for message in &parsed.messages {
                    message_source_paths.insert(message.dedup_key.clone(), path.clone());
                }
                messages.extend(parsed.messages);
                counts.extend(parsed.counts);
            }
            sessions.push(crate::usage::store::SessionImport {
                session: crate::usage::normalize::kimi_code::kimi_session(
                    meta, &messages, &counts, partial,
                ),
                messages,
                source_path: group.state.to_string_lossy().to_string(),
                message_source_paths,
            });
        }
        // Never publish a snapshot that raced an append, replacement or newly discovered file.
        if paths != discover_files(&self.root, &mut SyncReport::default(), discovery::accepts)? {
            return Err(SyncError::ChangedDuringScan);
        }
        for (path, source) in paths.iter().zip(&sources) {
            let (size, mtime) = fingerprint(path)?;
            if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        let imported = store.replace_client_snapshot(Client::KimiCode, &sources, &sessions)?;
        report.sessions_imported = sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}
