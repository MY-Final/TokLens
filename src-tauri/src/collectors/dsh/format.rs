//! Canonical generations are alternative versions of one log, never additive sources.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn generation(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let name = name.strip_suffix(".zstd").unwrap_or(name);
    if name == "session.jsonl" {
        return Some(0);
    }
    let version = name.strip_prefix("session.v")?.strip_suffix(".jsonl")?;
    if version.starts_with('0') || !version.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    version.parse().ok()
}

pub(super) fn accepts(path: &Path) -> bool {
    generation(path).is_some()
}

pub(super) fn select(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut selected: BTreeMap<PathBuf, (u64, PathBuf)> = BTreeMap::new();
    for path in paths {
        let Some(version) = generation(&path) else {
            continue;
        };
        let Some(parent) = path.parent() else {
            continue;
        };
        // At equal generations prefer the plain live log over its compressed copy.
        let replace = selected.get(parent).is_none_or(|(old_version, old)| {
            version > *old_version || (version == *old_version && path < *old)
        });
        if replace {
            selected.insert(parent.to_path_buf(), (version, path));
        }
    }
    let mut paths = selected
        .into_values()
        .map(|(_, path)| path)
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names_and_generation_priority() {
        for (name, version) in [
            ("session.jsonl", 0),
            ("session.jsonl.zstd", 0),
            ("session.v4.jsonl.zstd", 4),
            ("session.v99.jsonl", 99),
        ] {
            assert_eq!(generation(Path::new(name)), Some(version));
        }
        for name in [
            "session.v0.jsonl",
            "session.v04.jsonl",
            "session.v.jsonl",
            "session.v4.jsonl.bak",
            "session.v4.jsonl.zstd.tmp",
            "Session.jsonl",
        ] {
            assert!(!accepts(Path::new(name)));
        }
        assert_eq!(
            select(
                [
                    "a/session.jsonl",
                    "a/session.v4.jsonl.zstd",
                    "a/session.v4.jsonl",
                    "b/session.v99.jsonl",
                    "b/session.v1.jsonl"
                ]
                .map(PathBuf::from)
                .to_vec()
            ),
            vec![
                PathBuf::from("a/session.v4.jsonl"),
                PathBuf::from("b/session.v99.jsonl")
            ]
        );
    }
}
