//! Typed statistics-only projection. No chat history, ciphertext, cost or titles are retained.
use crate::collectors::SyncError;
use crate::usage::model::MessageCoverage;
use crate::usage::normalize::grok::{self, GrokSession};
use crate::usage::store::SessionImport;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub model_calls: Option<u64>,
    pub turn_count: Option<u64>,
    pub primary_model_id: Option<String>,
    pub turn_number: Option<i64>,
    pub ended_at: Option<String>,
    #[serde(default)]
    pub usage_is_incomplete: bool,
    #[serde(default)]
    pub model_usage: BTreeMap<String, Usage>,
}

impl Usage {
    fn counts(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cached_read_tokens,
            cache_write: self.cache_creation_tokens,
            reasoning: self.reasoning_tokens,
            client_total: self.total_tokens,
        }
    }
    fn model(&self) -> Option<&str> {
        self.primary_model_id
            .as_deref()
            .filter(|model| !model.trim().is_empty())
            .or_else(|| {
                (self.model_usage.len() == 1)
                    .then(|| self.model_usage.keys().next().map(String::as_str))
                    .flatten()
            })
    }
    fn valid(&self) -> bool {
        [
            self.input_tokens,
            self.output_tokens,
            self.cached_read_tokens,
            self.cache_creation_tokens,
            self.reasoning_tokens,
            self.total_tokens,
        ]
        .into_iter()
        .flatten()
        .all(|count| count <= i64::MAX as u64)
            && self
                .model_calls
                .is_none_or(|count| count <= i64::MAX as u64)
            && self.turn_count.is_none_or(|count| count <= i64::MAX as u64)
            && self.model_usage.values().all(Usage::valid)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    pub session_id: String,
    pub updated_at: Option<String>,
    pub session: Usage,
    #[serde(default)]
    pub turns: Vec<Usage>,
}

#[derive(Default, Deserialize)]
pub(super) struct Summary {
    pub info: Option<Info>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_active_at: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Info {
    pub id: Option<String>,
    pub cwd: Option<String>,
}

fn invalid() -> SyncError {
    std::io::Error::other("unsupported Grok statistics snapshot").into()
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SyncError> {
    let file = std::fs::File::open(path)?;
    serde_json::from_reader(std::io::BufReader::new(file)).map_err(|_| invalid())
}

pub(super) fn read_snapshot(path: &Path) -> Result<Snapshot, SyncError> {
    let snapshot: Snapshot = read(path)?;
    if snapshot.session_id.trim().is_empty() || !snapshot.session.valid() {
        return Err(invalid());
    }
    Ok(snapshot)
}

pub(super) fn read_summary(path: &Path) -> Result<Summary, SyncError> {
    read(path)
}

fn time(value: Option<&str>) -> Option<i64> {
    value.and_then(grok::timestamp_ms)
}

pub(super) fn import(
    snapshot: Snapshot,
    summary: Summary,
    source_path: String,
) -> Result<SessionImport, SyncError> {
    if summary
        .info
        .as_ref()
        .and_then(|info| info.id.as_deref())
        .is_some_and(|id| id != snapshot.session_id)
    {
        return Err(invalid());
    }
    let mut turns: BTreeMap<i64, Usage> = BTreeMap::new();
    for turn in snapshot.turns {
        let number = turn
            .turn_number
            .filter(|number| *number >= 0)
            .ok_or_else(invalid)?;
        if !turn.valid() {
            return Err(invalid());
        }
        if turns.get(&number).is_some_and(|old| old != &turn) {
            // An ambiguous duplicate cannot overwrite the last confirmed ledger.
            return Err(invalid());
        }
        turns.insert(number, turn);
    }
    let raw_turns: Vec<_> = turns.values().map(Usage::counts).collect();
    let raw_session = snapshot.session.counts();
    let incomplete =
        snapshot.session.usage_is_incomplete || turns.values().any(|turn| turn.usage_is_incomplete);
    let has_complete_counts = |raw: &RawTokenCounts| {
        [
            raw.input,
            raw.output,
            raw.cache_read,
            raw.cache_write,
            raw.reasoning,
            raw.client_total,
        ]
        .iter()
        .all(Option::is_some)
    };
    let sum_calls = turns
        .values()
        .map(|turn| turn.model_calls)
        .try_fold(0u64, |sum, calls| sum.checked_add(calls?));
    let coverage = if turns.is_empty() {
        MessageCoverage::NoMessages
    } else if !incomplete
        && has_complete_counts(&raw_session)
        && raw_turns.iter().all(has_complete_counts)
        && RawTokenCounts::sum(&raw_turns) == raw_session
        && snapshot.session.turn_count == Some(turns.len() as u64)
        && snapshot.session.model_calls.is_some()
        && snapshot.session.model_calls == sum_calls
    {
        MessageCoverage::Complete
    } else {
        MessageCoverage::Partial
    };
    let project = summary.info.as_ref().and_then(|info| info.cwd.as_deref());
    let started_at = time(summary.created_at.as_deref());
    let ended_at = time(snapshot.updated_at.as_deref())
        .or_else(|| time(summary.last_active_at.as_deref()))
        .or_else(|| time(summary.updated_at.as_deref()));
    let session = grok::session(&GrokSession {
        id: &snapshot.session_id,
        project,
        model: snapshot.session.model(),
        started_at_ms: started_at,
        ended_at_ms: ended_at,
        raw: raw_session,
        coverage,
        incomplete,
    });
    let messages = turns
        .into_iter()
        .map(|(number, turn)| {
            grok::turn(
                &snapshot.session_id,
                number,
                time(turn.ended_at.as_deref()),
                turn.model(),
                &turn.counts(),
                turn.usage_is_incomplete,
            )
        })
        .collect();
    Ok(SessionImport {
        session,
        messages,
        source_path,
        message_source_paths: BTreeMap::new(),
    })
}
