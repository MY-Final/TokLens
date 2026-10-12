//! SDK event whitelist: serde skips prompt/response, tools and system content.
use crate::collectors::{SyncError, files::fingerprint};
use crate::usage::{
    normalize::{
        copilot,
        request_ledger::{RequestMeta, SessionMeta},
    },
    store::SessionImport,
    time::parse_iso8601_utc_ms,
    tokens::RawTokenCounts,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    timestamp: Option<String>,
    #[serde(default)]
    data: Data,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    session_id: Option<String>,
    start_time: Option<String>,
    selected_model: Option<String>,
    new_model: Option<String>,
    model: Option<String>,
    api_call_id: Option<String>,
    context: Option<Context>,
}
#[derive(Deserialize)]
struct Context {
    cwd: Option<String>,
}

pub(super) struct SdkSession {
    pub import: SessionImport,
    pub user_events: BTreeSet<String>,
}
fn invalid() -> SyncError {
    SyncError::Source(std::io::Error::from(std::io::ErrorKind::InvalidData))
}
pub(super) fn read(path: &Path) -> Result<SdkSession, SyncError> {
    let before = fingerprint(path)?;
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut line = Vec::new();
    let mut meta = None;
    let mut end = None;
    let mut model = None;
    let mut requests = BTreeMap::new();
    let mut users = BTreeSet::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            return Err(SyncError::ChangedDuringScan);
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let event: Event = serde_json::from_slice(&line).map_err(|_| invalid())?;
        let time = event.timestamp.as_deref().and_then(parse_iso8601_utc_ms);
        end = end.max(time);
        match event.kind.as_str() {
            "session.start" => {
                let id = event
                    .data
                    .session_id
                    .filter(|id| !id.is_empty())
                    .ok_or_else(invalid)?;
                if meta.is_some() {
                    return Err(invalid());
                }
                model = event.data.selected_model;
                meta = Some(SessionMeta {
                    id,
                    parent: None,
                    project: event.data.context.and_then(|c| c.cwd),
                    start_ms: event
                        .data
                        .start_time
                        .as_deref()
                        .and_then(parse_iso8601_utc_ms)
                        .or(time),
                    end_ms: None,
                });
            }
            "user.message" => {
                users.insert(event.id.filter(|id| !id.is_empty()).ok_or_else(invalid)?);
            }
            "session.model_change" => {
                model = event.data.new_model.or(model);
            }
            "assistant.message" => {
                let api = event.data.api_call_id.filter(|id| !id.is_empty());
                if let Some(api) = api {
                    requests.insert(
                        api.clone(),
                        RequestMeta {
                            key: format!("api:{api}"),
                            model: event.data.model.or_else(|| model.clone()),
                            agent: None,
                            created_ms: time,
                            completed_ms: None,
                            sidechain: false,
                            source_ref: format!("assistant.message.apiCallId={api}"),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    if fingerprint(path)? != before {
        return Err(SyncError::ChangedDuringScan);
    }
    let mut meta = meta.ok_or_else(invalid)?;
    meta.end_ms = end;
    let messages: Vec<_> = requests
        .into_values()
        .map(|request| copilot::agent_host_request(&meta.id, request, RawTokenCounts::default()))
        .collect();
    let mut session = copilot::session(meta, &messages, &[], true);
    session.model = model.or(session.model);
    session.semantics_id = copilot::AGENT_HOST_SEMANTICS_ID.into();
    Ok(SdkSession {
        import: SessionImport {
            session,
            messages,
            source_path: path.to_string_lossy().into(),
            message_source_paths: BTreeMap::new(),
        },
        user_events: users,
    })
}
