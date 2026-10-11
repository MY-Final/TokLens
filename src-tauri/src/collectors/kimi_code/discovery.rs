//! Only session root metadata and agent wire files are eligible, never plugin state or mirrors.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn session_dir(path: &Path) -> Option<&Path> {
    let parent = path.parent()?;
    let session = if path.file_name()? == "state.json" {
        parent
    } else if path.file_name()? == "wire.jsonl" && parent.parent()?.file_name()? == "agents" {
        parent.parent()?.parent()?
    } else {
        return None;
    };
    session
        .file_name()?
        .to_str()?
        .starts_with("session_")
        .then_some(session)
}

pub(super) fn accepts(path: &Path) -> bool {
    session_dir(path).is_some()
}

pub(super) struct SessionFiles {
    pub state: PathBuf,
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
                state: dir.join("state.json"),
                wires: Vec::new(),
            });
        if path.file_name().is_some_and(|name| name == "wire.jsonl") {
            group.wires.push(path.clone());
        }
    }
    groups
}
