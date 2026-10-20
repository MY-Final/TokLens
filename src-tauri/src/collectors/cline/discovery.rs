//! 发现会话目录与其中的清单 / 消息文件；不读取文件内容。

use crate::collectors::{SyncError, SyncReport};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// 一个会话目录里的清单与消息文件。
#[derive(Debug, Clone)]
pub(super) struct SessionFiles {
    /// 文件名主干，等于客户端会话 id（清单缺失时的兜底来源）。
    pub stem: String,
    pub manifest: Option<PathBuf>,
    pub messages: Option<PathBuf>,
}

impl SessionFiles {
    /// 该会话涉及的全部源文件（清单与消息文件）。
    pub(super) fn paths(&self) -> Vec<&Path> {
        let mut paths = Vec::new();
        if let Some(path) = &self.manifest {
            paths.push(path.as_path());
        }
        if let Some(path) = &self.messages {
            paths.push(path.as_path());
        }
        paths
    }
}

/// 解析 Cline 的会话根目录。
///
/// 与 Cline 自身的目录解析保持一致：`CLINE_SESSION_DATA_DIR` 优先，
/// 其次是 `CLINE_DATA_DIR/sessions`、`CLINE_DIR/data/sessions`，最后是 `~/.cline/data/sessions`。
pub(super) fn resolve_root(
    session_dir: Option<OsString>,
    data_dir: Option<OsString>,
    cline_dir: Option<OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(dir) = non_empty(session_dir) {
        return Some(PathBuf::from(dir));
    }
    let data = non_empty(data_dir)
        .map(PathBuf::from)
        .or_else(|| non_empty(cline_dir).map(|dir| PathBuf::from(dir).join("data")))
        .or_else(|| home.map(|home| home.join(".cline").join("data")))?;
    Some(data.join("sessions"))
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.to_string_lossy().trim().is_empty())
}

/// 列出根目录下的全部会话文件。
pub(super) fn sessions(
    root: &Path,
    report: &mut SyncReport,
) -> Result<Vec<SessionFiles>, SyncError> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).map_err(SyncError::Directory)? {
        let entry = entry.map_err(SyncError::Directory)?;
        let kind = entry.file_type()?;
        // 不跟随符号链接 / 联接点，避免读出扫描根目录之外的内容。
        if kind.is_symlink() || !kind.is_dir() {
            continue;
        }
        found.extend(read_session_dir(&entry.path())?);
    }
    found.sort_by(|left, right| left.stem.cmp(&right.stem));
    report.files_seen = found.iter().map(|session| session.paths().len()).sum();
    Ok(found)
}

/// 读取一个会话目录：`<id>.json` 是清单，`<id>.messages.json` 是逐轮明细。
fn read_session_dir(dir: &Path) -> Result<Vec<SessionFiles>, SyncError> {
    let mut manifests: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut messages: BTreeMap<String, PathBuf> = BTreeMap::new();
    for entry in std::fs::read_dir(dir).map_err(SyncError::Directory)? {
        let entry = entry.map_err(SyncError::Directory)?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(stem) = name.strip_suffix(".messages.json") {
            messages.insert(stem.to_string(), entry.path());
        } else if let Some(stem) = name.strip_suffix(".json") {
            manifests.insert(stem.to_string(), entry.path());
        }
    }
    let mut found = Vec::new();
    for (stem, manifest) in manifests {
        let messages = messages.remove(&stem);
        found.push(SessionFiles {
            stem,
            manifest: Some(manifest),
            messages,
        });
    }
    // 只有明细、没有清单的目录也要采集：明细求和仍然可用。
    for (stem, messages) in messages {
        found.push(SessionFiles {
            stem,
            manifest: None,
            messages: Some(messages),
        });
    }
    Ok(found)
}
