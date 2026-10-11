//! Shared read-only discovery helpers for transcript collectors.

use super::{SyncError, SyncReport};
use std::path::{Path, PathBuf};

pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub(super) fn jsonl_files(root: &Path, report: &mut SyncReport) -> Result<Vec<PathBuf>, SyncError> {
    discover_files(root, report, |path| {
        path.extension().is_some_and(|ext| ext == "jsonl")
    })
}

pub(super) fn discover_files(
    root: &Path,
    report: &mut SyncReport,
    accept: fn(&Path) -> bool,
) -> Result<Vec<PathBuf>, SyncError> {
    let mut directories = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).map_err(SyncError::Directory)? {
            let entry = entry.map_err(SyncError::Directory)?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file() && accept(&entry.path()) {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    report.files_seen = files.len();
    Ok(files)
}

pub(super) fn fingerprint(path: &Path) -> std::io::Result<(i64, i64)> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?;
    Ok((metadata.len() as i64, modified.as_millis() as i64))
}

/// SQLite bindings are signed; reject invalid source counts before normalization.
pub(super) fn valid_counts(raw: &crate::usage::tokens::RawTokenCounts) -> bool {
    let fields = [
        raw.input,
        raw.output,
        raw.cache_read,
        raw.cache_write,
        raw.reasoning,
    ];
    let total = fields
        .into_iter()
        .flatten()
        .try_fold(0u64, u64::checked_add);
    total.is_some_and(|total| total <= i64::MAX as u64)
        && raw
            .client_total
            .is_none_or(|total| total <= i64::MAX as u64)
}
