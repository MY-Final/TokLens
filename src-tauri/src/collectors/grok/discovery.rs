//! Only sessions/<encoded workspace>/<session>/usage.json and summary.json.
use crate::collectors::{SyncError, SyncReport};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(super) fn resolve_root(config: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    config
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".grok")))
        .map(|root| root.join("sessions"))
}

pub(super) fn files(root: &Path, report: &mut SyncReport) -> Result<Vec<PathBuf>, SyncError> {
    let mut paths = Vec::new();
    for workspace in std::fs::read_dir(root).map_err(SyncError::Directory)? {
        let workspace = workspace.map_err(SyncError::Directory)?;
        if !workspace.file_type()?.is_dir() {
            continue;
        }
        for session in std::fs::read_dir(workspace.path()).map_err(SyncError::Directory)? {
            let session = session.map_err(SyncError::Directory)?;
            if !session.file_type()?.is_dir() {
                continue;
            }
            let usage = session.path().join("usage.json");
            // An absent usage ledger has no counters; summary-only sessions are not invented.
            if !usage.try_exists()? {
                continue;
            }
            if !std::fs::symlink_metadata(&usage)?.file_type().is_file() {
                return Err(std::io::Error::other("unsupported usage source").into());
            }
            paths.push(usage);
            let summary = session.path().join("summary.json");
            if summary.try_exists()? {
                if !std::fs::symlink_metadata(&summary)?.file_type().is_file() {
                    return Err(std::io::Error::other("unsupported summary source").into());
                }
                paths.push(summary);
            }
        }
    }
    paths.sort();
    report.files_seen = paths.len();
    Ok(paths)
}
