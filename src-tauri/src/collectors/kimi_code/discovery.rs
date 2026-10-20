//! Only session root metadata and agent wire files are eligible, never plugin state or mirrors.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 会话目录。两种落盘布局都属于同一个会话：
///
/// - `<session>/state.json` 与 `<session>/agents/<agent>/wire.jsonl`（旧版）；
/// - `<workdir>/<session>/wire.jsonl`（当前版本，工作目录名是路径摘要）。
pub(super) fn session_dir(path: &Path) -> Option<&Path> {
    let parent = path.parent()?;
    match path.file_name()? {
        // 会话元数据只认旧版布局的 `session_*` 目录，避免把插件自己的
        // state.json 当成会话。
        name if name == "state.json" => parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("session_"))
            .then_some(parent),
        name if name == "wire.jsonl" => {
            // 旧版布局把每个 agent 的 wire 放在 <session>/agents/<agent>/ 下。
            let agents = parent.parent();
            if agents
                .and_then(Path::file_name)
                .is_some_and(|name| name == "agents")
            {
                agents.and_then(Path::parent)
            } else {
                Some(parent)
            }
        }
        _ => None,
    }
}

pub(super) fn accepts(path: &Path) -> bool {
    session_dir(path).is_some()
}

pub(super) struct SessionFiles {
    /// 旧版布局的会话元数据；当前版本没有这个文件。
    pub state: Option<PathBuf>,
    pub wires: Vec<PathBuf>,
}

pub(super) fn group(paths: &[PathBuf]) -> BTreeMap<PathBuf, SessionFiles> {
    let mut groups = BTreeMap::new();
    for path in paths {
        let Some(dir) = session_dir(path) else {
            continue;
        };
        let group = groups
            .entry(dir.to_path_buf())
            .or_insert_with(|| SessionFiles {
                state: None,
                wires: Vec::new(),
            });
        match path.file_name().and_then(|name| name.to_str()) {
            Some("state.json") => group.state = Some(path.clone()),
            Some("wire.jsonl") => group.wires.push(path.clone()),
            _ => {}
        }
    }
    groups
}
