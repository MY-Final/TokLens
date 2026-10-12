//! Link persisted user turns to SDK identities, never read drafts or edit payloads.
use super::database;
use crate::collectors::SyncError;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Provider {
    sdk_session_id: String,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    #[serde(rename = "_meta", default)]
    meta: Meta,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    turn_token_totals: Option<Vec<Total>>,
    #[serde(rename = "vscode.modelCall")]
    call: Option<Call>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Total {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_tokens: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Call {
    schema_version: u64,
    sdk_session_id: String,
    api_call_id: String,
}

pub(super) struct Turn {
    pub sdk_id: String,
    pub event_id: String,
    pub api_id: String,
    pub last_call: RawTokenCounts,
    pub totals: Vec<RawTokenCounts>,
    pub source_path: String,
    pub primary: bool,
}
fn invalid() -> SyncError {
    SyncError::Source(std::io::Error::from(std::io::ErrorKind::InvalidData))
}
fn counts(
    input: Option<u64>,
    output: Option<u64>,
    cache: Option<u64>,
) -> Result<RawTokenCounts, SyncError> {
    if [input, output, cache]
        .into_iter()
        .flatten()
        .any(|n| n > i64::MAX as u64)
    {
        return Err(invalid());
    }
    Ok(RawTokenCounts {
        input,
        output,
        cache_read: cache,
        ..RawTokenCounts::default()
    })
}

pub(super) fn read(path: &Path) -> Result<Vec<Turn>, SyncError> {
    let conn = database::open(path)?;
    // Only databases carrying this Copilot SDK contract are considered. Empty
    // backing databases and Claude Agent Host sessions are not usage sources.
    let mut stmt = conn.prepare("SELECT key,value FROM session_metadata WHERE key IN ('defaultChatProviderData','agentHost.chatProviderData') ORDER BY key DESC")?;
    let mut provider = None;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (key, value) = row?;
        if let Ok(p) = serde_json::from_str::<Provider>(&value)
            && !p.sdk_session_id.is_empty()
        {
            provider = Some((p.sdk_session_id, key == "defaultChatProviderData"));
            break;
        }
    }
    let Some((sdk, primary)) = provider else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT t.event_id,u.usage FROM turn_usage u JOIN turns t ON t.id=u.turn_id ORDER BY t.id",
    )?;
    let mut result = Vec::new();
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
    })? {
        let (event, value) = row?;
        let event = event.filter(|s| !s.is_empty()).ok_or_else(invalid)?;
        let usage: Usage = serde_json::from_str(&value).map_err(|_| invalid())?;
        let call = usage.meta.call.ok_or_else(invalid)?;
        if call.schema_version != 1 || call.sdk_session_id != sdk || call.api_call_id.is_empty() {
            return Err(invalid());
        }
        let totals = usage.meta.turn_token_totals.ok_or_else(invalid)?;
        if totals.is_empty() {
            return Err(invalid());
        }
        result.push(Turn {
            sdk_id: sdk.clone(),
            event_id: event,
            api_id: call.api_call_id,
            last_call: counts(
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens,
            )?,
            totals: totals
                .into_iter()
                .map(|t| counts(t.input_tokens, t.output_tokens, t.cached_tokens))
                .collect::<Result<_, _>>()?,
            source_path: path.to_string_lossy().into(),
            primary,
        });
    }
    Ok(result)
}

pub(super) fn dedup(turns: impl IntoIterator<Item = Turn>) -> Result<Vec<Turn>, SyncError> {
    let mut chosen: BTreeMap<(String, String), Turn> = BTreeMap::new();
    for turn in turns {
        let key = (turn.sdk_id.clone(), turn.event_id.clone());
        if let Some(old) = chosen.get(&key) {
            if old.primary && !turn.primary {
                continue;
            }
            if old.primary == turn.primary {
                if old.api_id != turn.api_id
                    || old.last_call != turn.last_call
                    || old.totals != turn.totals
                {
                    return Err(SyncError::ChangedDuringScan);
                }
                continue;
            }
        }
        chosen.insert(key, turn);
    }
    Ok(chosen.into_values().collect())
}
