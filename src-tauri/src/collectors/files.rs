//! Shared read-only discovery helpers for transcript collectors.

use super::{SyncError, SyncReport};
use std::path::{Path, PathBuf};

pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("HOME").filter(|value| !value.is_empty()))
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
            // Never follow symlinks/junctions: a reparse point can escape the source
            // tree or form a directory cycle. Reads stay inside the scanned root.
            if kind.is_symlink() {
                continue;
            }
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

#[cfg(test)]
mod tests {
    use super::{SyncReport, jsonl_files};
    use crate::collectors::test_support::TempDir;

    #[cfg(unix)]
    #[test]
    fn discover_files_does_not_follow_symlinked_directories() {
        use std::os::unix::fs::symlink;
        let dir = TempDir::new();
        dir.write("real/nested/a.jsonl", "{}");
        symlink(dir.0.join("real"), dir.0.join("link")).expect("synthetic symlink");
        let mut report = SyncReport::default();
        let files = jsonl_files(&dir.0, &mut report).expect("discover");
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.jsonl"));
        assert_eq!(report.files_seen, 1);
    }

    #[cfg(windows)]
    #[test]
    fn discover_files_does_not_follow_reparse_directories() {
        let dir = TempDir::new();
        dir.write("real/nested/a.jsonl", "{}");
        // Directory symlinks need Developer Mode or admin; skip when unavailable.
        if std::os::windows::fs::symlink_dir(dir.0.join("real"), dir.0.join("link")).is_err() {
            return;
        }
        let mut report = SyncReport::default();
        let files = jsonl_files(&dir.0, &mut report).expect("discover");
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.jsonl"));
        assert_eq!(report.files_seen, 1);
    }
}
