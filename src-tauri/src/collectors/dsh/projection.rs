//! Fold owned usage metadata into deduplicated request attempts.

use super::Parsed;
use super::metadata::Header;
use crate::usage::normalize::dsh::{DshMessage, DshSession, dsh_message, dsh_session};
use crate::usage::store::SessionImport;
use crate::usage::tokens::RawTokenCounts;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) struct Event {
    pub seq: i64,
    pub time: Option<i64>,
    pub kind: EventKind,
}

pub(super) enum EventKind {
    Model(Option<String>),
    Retry(i64, i64),
    Sample {
        turn: i64,
        step: i64,
        usage: Option<RawTokenCounts>,
        complete: bool,
    },
}

pub(super) fn fold(
    header: Header,
    events: Vec<Event>,
    cut: i64,
    path: &Path,
    unparsed: usize,
) -> Parsed {
    let mut model = None;
    let mut end = header.created_at;
    let mut messages = BTreeMap::new();
    let mut usages = BTreeMap::new();
    let mut attempts: BTreeMap<(i64, i64), u64> = BTreeMap::new();
    let mut retries = BTreeSet::new();
    let mut last_sample = None;
    for event in events {
        // Inherited request config is safe metadata and may apply to the first owned request.
        if let EventKind::Model(next) = event.kind {
            model = next;
            continue;
        }
        if event.seq < cut {
            continue;
        }
        match event.kind {
            EventKind::Retry(turn, step) => {
                if retries.insert(event.seq) && last_sample == Some((turn, step)) {
                    *attempts.entry((turn, step)).or_default() += 1;
                    last_sample = None;
                }
            }
            EventKind::Sample {
                turn,
                step,
                usage,
                complete,
            } => {
                let attempt = *attempts.get(&(turn, step)).unwrap_or(&0);
                let key = (turn, step, attempt);
                let has_sample = usage.is_some();
                let usage = usage
                    .or_else(|| usages.get(&key).copied())
                    .unwrap_or_default();
                end = end.max(event.time);
                let mut message = dsh_message(&DshMessage {
                    session_id: &header.id,
                    seq: event.seq,
                    turn,
                    step,
                    attempt,
                    time_ms: event.time,
                    model: model.as_deref(),
                    usage,
                    is_sidechain: header.origin.as_deref() == Some("subagent"),
                });
                if complete {
                    message.completed_at_ms = event.time;
                }
                messages.insert(key, message);
                usages.insert(key, usage);
                if has_sample {
                    last_sample = Some((turn, step));
                }
            }
            EventKind::Model(_) => {}
        }
    }
    let counts =
        crate::usage::normalize::sum_complete_counts(&usages.into_values().collect::<Vec<_>>());
    let messages = messages.into_values().collect::<Vec<_>>();
    let mut session = dsh_session(&DshSession {
        id: &header.id,
        parent: header.parent_session.as_deref(),
        cwd: header.cwd.as_deref(),
        model: model.as_deref(),
        start_ms: header.created_at,
        end_ms: end,
        message_count: messages.len(),
        usage: counts,
    });
    if unparsed > 0 {
        session.coverage = crate::usage::model::MessageCoverage::Partial;
    }
    Parsed {
        import: SessionImport {
            session,
            messages,
            source_path: path.to_string_lossy().to_string(),
            message_source_paths: BTreeMap::new(),
        },
        unparsed,
    }
}
