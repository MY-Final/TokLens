//! 扫描 Cline 会话目录，导入清单累计用量与逐轮明细。

use super::{PARSER_VERSION, discovery, parser, rollup};
use crate::collectors::files::{fingerprint, home_dir};
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct ClineCollector {
    root: PathBuf,
}

/// 一个会话的读取结果：合成内容与读失败的文件。
struct SessionRead {
    built: Option<rollup::Built>,
    failed: Vec<PathBuf>,
}

/// 读取一个可选源文件；文件存在但读取失败时记录到 `failed`，由调用方保留历史数据。
fn read_optional<T>(
    path: Option<&PathBuf>,
    failed: &mut Vec<PathBuf>,
    read: impl Fn(&Path) -> Result<T, SyncError>,
) -> Option<T> {
    let path = path?;
    match read(path) {
        Ok(value) => Some(value),
        Err(_) => {
            failed.push(path.clone());
            None
        }
    }
}

impl ClineCollector {
    pub fn default_root() -> Option<PathBuf> {
        discovery::resolve_root(
            std::env::var_os("CLINE_SESSION_DATA_DIR"),
            std::env::var_os("CLINE_DATA_DIR"),
            std::env::var_os("CLINE_DIR"),
            home_dir(),
        )
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sync(&self, store: &UsageStore, options: &SyncOptions) -> Result<SyncReport, SyncError> {
        self.sync_before_commit(store, options, || {})
    }

    /// 在写入快照之前执行回调，用于测试"扫描期间源文件被改写"的情况。
    #[cfg(test)]
    pub(super) fn sync_with_mutation(
        &self,
        store: &UsageStore,
        before_commit: impl FnOnce(),
    ) -> Result<SyncReport, SyncError> {
        self.sync_before_commit(store, &SyncOptions { force_rescan: true }, before_commit)
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
        let sessions = discovery::sessions(&self.root, &mut report)?;
        let mut fingerprints: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for session in &sessions {
            for path in session.paths() {
                fingerprints.insert(path.to_string_lossy().to_string(), fingerprint(path)?);
            }
        }
        if !options.force_rescan && self.unchanged(store, &fingerprints)? {
            report.files_skipped_unchanged = fingerprints.len();
            return Ok(report);
        }
        let mut sources: Vec<SourceFile> = Vec::new();
        let mut imports = Vec::new();
        let mut imported_ids: BTreeSet<String> = BTreeSet::new();
        for session in &sessions {
            let read = self.read_session(session);
            if !read.failed.is_empty() {
                // 读取失败的文件不推进水位，保留历史数据，下次同步重试。
                report.files_failed += read.failed.len();
                continue;
            }
            let Some(built) = read.built else {
                report.files_without_identity += session.paths().len();
                continue;
            };
            // 同一会话 id 只能有一条记录，避免同名会话互相覆盖。
            if !imported_ids.insert(built.import.session.client_session_id.clone()) {
                report.files_without_identity += session.paths().len();
                continue;
            }
            for path in session.paths() {
                let key = path.to_string_lossy().to_string();
                let Some(&(size, mtime)) = fingerprints.get(&key) else {
                    continue;
                };
                let unparsed = if session.messages.as_deref() == Some(path) {
                    built.unparsed as i64
                } else {
                    0
                };
                sources.push(SourceFile {
                    client: Client::Cline,
                    path: key,
                    kind: SourceFileKind::Jsonl,
                    size_bytes: Some(size),
                    mtime_ms: Some(mtime),
                    content_offset: size,
                    parser_version: PARSER_VERSION,
                    last_scan_ms: Some(now_ms()),
                    unparsed_records: unparsed,
                });
            }
            report.files_parsed += session.paths().len();
            imports.push(built.import);
        }
        // 解析期间源文件被改写时不能发布快照。
        before_commit();
        for source in &sources {
            let (size, mtime) = fingerprint(Path::new(&source.path))?;
            if source.size_bytes != Some(size) || source.mtime_ms != Some(mtime) {
                return Err(SyncError::ChangedDuringScan);
            }
        }
        sources.sort_by(|left, right| left.path.cmp(&right.path));
        // 有文件读取失败时只合并已解析的会话，失败来源的历史数据保持不变。
        let result = if report.files_failed == 0 {
            store.replace_client_snapshot(Client::Cline, &sources, &imports)?
        } else {
            store.merge_client_snapshot(Client::Cline, &sources, &imports)?
        };
        report.sessions_imported = imports.len();
        report.messages_inserted = result.messages_inserted;
        report.messages_pruned = result.messages_pruned;
        Ok(report)
    }

    /// 全部源文件未变化且文件集合未变时，可以跳过本次同步。
    fn unchanged(
        &self,
        store: &UsageStore,
        fingerprints: &BTreeMap<String, (i64, i64)>,
    ) -> Result<bool, SyncError> {
        for (path, (size, mtime)) in fingerprints {
            let old = store.source_file(Client::Cline, path)?;
            let known = old.as_ref().is_some_and(|old| {
                old.size_bytes == Some(*size)
                    && old.mtime_ms == Some(*mtime)
                    && old.parser_version == PARSER_VERSION
            });
            if !known {
                return Ok(false);
            }
        }
        let paths: Vec<String> = fingerprints.keys().cloned().collect();
        Ok(store.source_paths(Client::Cline)? == paths)
    }

    /// 读取一个会话的清单与明细；读失败的文件单独列出，由调用方保留历史数据。
    fn read_session(&self, session: &discovery::SessionFiles) -> SessionRead {
        let mut failed = Vec::new();
        let manifest = read_optional(
            session.manifest.as_ref(),
            &mut failed,
            parser::read_manifest,
        );
        let messages = read_optional(
            session.messages.as_ref(),
            &mut failed,
            parser::read_messages,
        );
        let built = rollup::build(
            &session.stem,
            manifest,
            messages,
            session
                .manifest
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            session
                .messages
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        );
        SessionRead { built, failed }
    }
}
