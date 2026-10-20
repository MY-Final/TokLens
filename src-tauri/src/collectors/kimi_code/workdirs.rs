//! 把 `kimi.json` 登记的工作目录映射回会话目录名。
//!
//! Kimi 用工作目录路径的 MD5 摘要作为会话的父目录名，因此反查摘要就能得到
//! 项目路径；`kimi.json` 缺失或摘要对不上时项目路径保持未知。

use std::collections::BTreeMap;
use std::path::Path;

/// 工作目录摘要 → 项目路径。
pub(super) type WorkDirs = BTreeMap<String, String>;

/// 读取 `kimi.json` 的工作目录登记；文件缺失或结构不符时返回空表。
pub(super) fn read(home: &Path) -> WorkDirs {
    let Ok(text) = std::fs::read_to_string(home.join("kimi.json")) else {
        return WorkDirs::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return WorkDirs::new();
    };
    let Some(entries) = value.get("work_dirs").and_then(|value| value.as_array()) else {
        return WorkDirs::new();
    };
    let mut dirs = WorkDirs::new();
    for entry in entries {
        let Some(path) = entry.get("path").and_then(|value| value.as_str()) else {
            continue;
        };
        dirs.insert(digest(path), path.to_string());
    }
    dirs
}

/// 会话目录名就是工作目录路径的 MD5 摘要。
pub(super) fn digest(path: &str) -> String {
    format!("{:x}", md5::compute(path.as_bytes()))
}
