//! Coordinate discovery, parsing, thread aggregation and atomic import.

use super::discovery::{decide_scan, discover_rollout_files, mtime_ms};
use super::parser::parse_file;
use super::thread::ThreadAggregate;
use super::{PARSER_VERSION, ScanReason, SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Codex 采集器。
pub struct CodexCollector {
    root: PathBuf,
}

impl CodexCollector {
    /// 默认根目录：`%USERPROFILE%\.codex\sessions`（取不到环境变量时返回 `None`）。
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("USERPROFILE")
            .map(|home| PathBuf::from(home).join(".codex").join("sessions"))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 扫描并增量导入。单个文件失败不会中断整次同步。
    pub fn sync(&self, store: &UsageStore, options: &SyncOptions) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.root.is_dir() {
            report.root_missing = true;
            return Ok(report);
        }
        let files = discover_rollout_files(&self.root, &mut report)?;
        report.files_seen = files.len();

        let mut parsed_files = Vec::new();
        for path in files {
            let (size_bytes, mtime_ms) = match std::fs::metadata(&path) {
                Ok(metadata) => (metadata.len() as i64, mtime_ms(&metadata)),
                Err(_) => {
                    report.files_failed += 1;
                    continue;
                }
            };
            let path_string = path.to_string_lossy().to_string();
            let watermark = store.source_file(Client::Codex, &path_string)?;
            let reason = decide_scan(
                watermark.as_ref(),
                size_bytes,
                mtime_ms,
                options.force_rescan,
            );
            *report.reasons.entry(reason).or_default() += 1;
            if reason == ScanReason::Unchanged {
                report.files_skipped_unchanged += 1;
                continue;
            }
            match parse_file(&path, size_bytes, mtime_ms) {
                Ok(parsed) => {
                    report.files_parsed += 1;
                    report.unparsed_lines += parsed.unparsed_lines;
                    parsed_files.push(parsed);
                }
                Err(_) => report.files_failed += 1,
            }
        }

        let mut threads: BTreeMap<String, ThreadAggregate> = BTreeMap::new();
        for parsed in parsed_files {
            match parsed.thread_id.clone() {
                Some(thread_id) => threads.entry(thread_id).or_default().push(parsed),
                None => report.files_without_identity += 1,
            }
        }

        for (thread_id, aggregate) in threads {
            let session = aggregate.session_record(&thread_id);
            let mut files = aggregate.files;
            files.sort_by(|left, right| {
                (left.mtime_ms, left.path.clone()).cmp(&(right.mtime_ms, right.path.clone()))
            });
            let mut session_counted = false;
            for file in files {
                let source = SourceFile {
                    client: Client::Codex,
                    path: file.path.to_string_lossy().to_string(),
                    kind: SourceFileKind::Jsonl,
                    size_bytes: Some(file.size_bytes),
                    mtime_ms: Some(file.mtime_ms),
                    content_offset: file.size_bytes,
                    parser_version: PARSER_VERSION,
                    last_scan_ms: Some(now_ms()),
                    unparsed_records: file.unparsed_lines as i64,
                };
                let summary = store.replace_file_import(&source, &session, &file.messages)?;
                if !session_counted {
                    session_counted = true;
                    match summary.session {
                        Some(crate::usage::store::UpsertOutcome::SkippedStale) => {
                            report.sessions_skipped_stale += 1;
                        }
                        Some(_) => report.sessions_imported += 1,
                        None => {}
                    }
                }
                report.messages_inserted += summary.messages_inserted;
                report.messages_updated += summary.messages_updated;
                report.messages_skipped_stale += summary.messages_skipped_stale;
                report.messages_pruned += summary.messages_pruned;
            }
        }

        Ok(report)
    }
}
