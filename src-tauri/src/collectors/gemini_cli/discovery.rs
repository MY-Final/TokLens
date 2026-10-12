use crate::collectors::files::{discover_files, home_dir};
use crate::collectors::{SyncError, SyncReport};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(super) fn resolve_root(config: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    // Installed Storage.homedir treats GEMINI_CLI_HOME as the user home,
    // then getGlobalGeminiDir appends .gemini. GEMINI_DIR is only a constant.
    config
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or(home)
        .map(|home| home.join(".gemini"))
}

pub(super) fn default_root() -> Option<PathBuf> {
    resolve_root(std::env::var_os("GEMINI_CLI_HOME"), home_dir())
}

pub(super) fn discover(root: &Path, report: &mut SyncReport) -> Result<Vec<PathBuf>, SyncError> {
    let tmp = root.join("tmp");
    if !tmp.try_exists()? {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(tmp).map_err(SyncError::Directory)? {
        let entry = entry.map_err(SyncError::Directory)?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let chats = entry.path().join("chats");
        if !chats.try_exists()? || std::fs::symlink_metadata(&chats)?.file_type().is_symlink() {
            continue;
        }
        // Includes the installed CLI's chats/<parent-id>/<subagent-id>.jsonl.
        paths.extend(discover_files(
            &chats,
            &mut SyncReport::default(),
            |path| path.extension().is_some_and(|ext| ext == "jsonl"),
        )?);
    }
    paths.sort();
    report.files_seen = paths.len();
    Ok(paths)
}

pub(super) fn project_marker(root: &Path, journal: &Path) -> std::io::Result<Option<PathBuf>> {
    let Some(project) = journal
        .strip_prefix(root.join("tmp"))
        .ok()
        .and_then(|path| path.components().next())
    else {
        return Ok(None);
    };
    for base in ["tmp", "history"] {
        let marker = root
            .join(base)
            .join(project.as_os_str())
            .join(".project_root");
        if marker.try_exists()? && std::fs::symlink_metadata(&marker)?.file_type().is_file() {
            return Ok(Some(marker));
        }
    }
    Ok(None)
}
