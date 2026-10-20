//! Deserialize only identity, project and timestamps; titles and lastPrompt are skipped.

use super::workdirs::WorkDirs;
use crate::usage::normalize::kimi_code::KimiSessionMeta;
use crate::usage::time::parse_iso8601_utc_ms;
use serde::Deserialize;
use std::io::BufReader;
use std::path::Path;

#[derive(Deserialize)]
#[serde(untagged)]
enum Timestamp {
    Millis(i64),
    Iso(String),
}

impl Timestamp {
    fn millis(&self) -> Option<i64> {
        match self {
            Self::Millis(value) => Some(*value),
            Self::Iso(value) => parse_iso8601_utc_ms(value),
        }
        .filter(|value| *value >= 0)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    id: Option<String>,
    version: Option<u64>,
    cwd: Option<String>,
    work_dir: Option<String>,
    created_at: Option<Timestamp>,
    updated_at: Option<Timestamp>,
}

pub(super) fn invalid() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unsupported or incomplete Kimi Code session",
    )
}

/// 读取会话元数据。
///
/// 有 `state.json` 时按旧版布局解析；没有时用会话目录名作为标识，
/// 项目路径由工作目录摘要反查得到，时间由 wire 记录补齐。
pub(super) fn read(
    session_dir: &Path,
    state: Option<&Path>,
    work_dirs: &WorkDirs,
) -> std::io::Result<KimiSessionMeta> {
    let Some(state) = state else {
        // 旧版布局的会话一定有 state.json；缺了说明会话不完整。
        if session_dir.join("agents").is_dir() {
            return Err(invalid());
        }
        let id = session_dir
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(invalid)?;
        let project = session_dir
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .and_then(|name| work_dirs.get(name).cloned());
        return Ok(KimiSessionMeta {
            id: id.to_string(),
            project,
            start_ms: None,
            end_ms: None,
        });
    };
    let state: State = serde_json::from_reader(BufReader::new(std::fs::File::open(state)?))
        .map_err(|_| invalid())?;
    if state.version.is_some_and(|version| version != 2) {
        return Err(invalid());
    }
    let id = state
        .id
        .or_else(|| session_dir.file_name()?.to_str().map(str::to_string))
        .filter(|id| !id.is_empty())
        .ok_or_else(invalid)?;
    let start = state.created_at.as_ref().and_then(Timestamp::millis);
    if start.is_none() {
        return Err(invalid());
    }
    Ok(KimiSessionMeta {
        id,
        project: state.cwd.or(state.work_dir),
        start_ms: start,
        end_ms: state.updated_at.as_ref().and_then(Timestamp::millis),
    })
}
