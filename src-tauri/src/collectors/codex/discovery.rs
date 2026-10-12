//! Read-only rollout discovery and incremental scan decisions.

use super::{PARSER_VERSION, ScanReason, SyncError, SyncReport};
use crate::usage::store::SourceFileRow;
use std::path::{Path, PathBuf};

/// 枚举 `rollout-*.jsonl`（递归；子目录读取失败只计数，不中断）。
pub(super) fn discover_rollout_files(
    root: &Path,
    report: &mut SyncReport,
) -> Result<Vec<PathBuf>, SyncError> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                if directory == root {
                    return Err(SyncError::Directory(error));
                }
                report.directories_failed += 1;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    report.directories_failed += 1;
                    continue;
                }
            };
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => directories.push(path),
                Ok(file_type) if file_type.is_file() => {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                        files.push(path);
                    }
                }
                Err(_) => report.files_failed += 1,
                _ => {}
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(super) fn mtime_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis() as i64)
}

/// 增量决策：未变化跳过，其余情况整文件重新解析。
pub(super) fn decide_scan(
    watermark: Option<&SourceFileRow>,
    size_bytes: i64,
    mtime_ms: i64,
    force_rescan: bool,
) -> ScanReason {
    if force_rescan {
        return ScanReason::Forced;
    }
    let Some(watermark) = watermark else {
        return ScanReason::New;
    };
    if watermark.parser_version != PARSER_VERSION {
        return ScanReason::ParserVersionChanged;
    }
    if size_bytes < watermark.content_offset || size_bytes < watermark.size_bytes.unwrap_or(0) {
        return ScanReason::TruncatedOrReplaced;
    }
    if watermark.size_bytes == Some(size_bytes) && watermark.mtime_ms == Some(mtime_ms) {
        return ScanReason::Unchanged;
    }
    ScanReason::Appended
}
