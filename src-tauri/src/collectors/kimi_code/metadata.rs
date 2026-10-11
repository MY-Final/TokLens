//! Deserialize only identity, project and timestamps; titles and lastPrompt are skipped.

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

pub(super) fn read(path: &Path) -> std::io::Result<KimiSessionMeta> {
    let state: State = serde_json::from_reader(BufReader::new(std::fs::File::open(path)?))
        .map_err(|_| invalid())?;
    if state.version.is_some_and(|version| version != 2) {
        return Err(invalid());
    }
    let id = state
        .id
        .or_else(|| path.parent()?.file_name()?.to_str().map(str::to_string))
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
