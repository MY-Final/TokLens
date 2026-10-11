//! Whitelisted SQL projections inside one source read transaction.

use crate::collectors::files::valid_counts;
use crate::usage::model::{MessageCoverage, MessageRecord, SessionRecord};
use crate::usage::normalize::{
    OpenCodeMessageRow, OpenCodeSessionRow, opencode_message, opencode_session,
    pick_opencode_session,
};
use crate::usage::store::SessionImport;
use crate::usage::tokens::RawTokenCounts;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Deserialize)]
struct Counts {
    input: Option<u64>,
    output: Option<u64>,
    reasoning: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    total: Option<u64>,
}

impl Counts {
    fn raw(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input,
            output: self.output,
            reasoning: self.reasoning,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            client_total: self.total,
        }
    }
}

#[derive(Deserialize)]
struct Session {
    id: String,
    parent_id: Option<String>,
    directory: Option<String>,
    model: Option<Model>,
    created: Option<i64>,
    updated: Option<i64>,
    cost: Option<f64>,
    tokens: Counts,
}

#[derive(Default, Deserialize)]
struct Model {
    id: Option<String>,
    #[serde(rename = "modelID")]
    model_id: Option<String>,
    #[serde(rename = "providerID")]
    provider_id: Option<String>,
}

fn model_label(model: Option<&Model>) -> Option<String> {
    let model = model?;
    let id = model.id.as_ref().or(model.model_id.as_ref())?;
    Some(
        model
            .provider_id
            .as_ref()
            .map_or_else(|| id.clone(), |provider| format!("{provider}/{id}")),
    )
}

impl Session {
    fn row<'a>(&'a self, model: Option<&'a str>) -> OpenCodeSessionRow<'a> {
        OpenCodeSessionRow {
            id: &self.id,
            parent_id: self.parent_id.as_deref(),
            directory: self.directory.as_deref(),
            model,
            created_ms: self.created,
            updated_ms: self.updated,
            cost_usd: self.cost.filter(|cost| cost.is_finite() && *cost >= 0.0),
            usage: self.tokens.raw(),
        }
    }
}

pub(super) struct Snapshot {
    pub sessions: Vec<SessionImport>,
    pub unparsed: usize,
}

fn columns(conn: &Connection, table: &str) -> rusqlite::Result<BTreeSet<String>> {
    // Callers only pass static allowlisted table names.
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    statement.query_map([], |row| row.get(1))?.collect()
}

fn table_exists(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn sessions(
    conn: &Connection,
    table: &str,
    unparsed: &mut usize,
) -> rusqlite::Result<BTreeMap<String, Session>> {
    let columns = columns(conn, table)?;
    let col = |name: &str| {
        if columns.contains(name) {
            name.to_string()
        } else {
            "NULL".to_string()
        }
    };
    if !columns.contains("id") {
        return Err(rusqlite::Error::InvalidQuery);
    }
    // Only these identifiers can enter the query. Titles, summaries, permissions and raw model JSON stay in the source.
    let sql = format!("SELECT json_object('id', id, 'parent_id', {}, 'directory', {},
        'model', CASE WHEN json_valid({model}) THEN json_object('id', json_extract({model}, '$.id'),
            'modelID', json_extract({model}, '$.modelID'), 'providerID', json_extract({model}, '$.providerID')) ELSE NULL END,
        'created', {}, 'updated', {}, 'cost', {},
        'tokens', json_object('input', {}, 'output', {}, 'reasoning', {}, 'cache_read', {}, 'cache_write', {}, 'total', {})) FROM {table}",
        col("parent_id"), col("directory"), col("time_created"), col("time_updated"), col("cost"),
        col("tokens_input"), col("tokens_output"), col("tokens_reasoning"), col("tokens_cache_read"), col("tokens_cache_write"), col("tokens_total"), model=col("model"));
    let mut statement = conn.prepare(&sql)?;
    let mut result = BTreeMap::new();
    for row in statement.query_map([], |row| row.get::<_, String>(0))? {
        match serde_json::from_str::<Session>(&row?) {
            Ok(session) if !session.id.is_empty() && valid_counts(&session.tokens.raw()) => {
                result.insert(session.id.clone(), session);
            }
            _ => *unparsed += 1,
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
struct Message {
    id: String,
    session_id: String,
    role: Option<String>,
    model_id: Option<String>,
    provider_id: Option<String>,
    agent: Option<String>,
    created: Option<i64>,
    completed: Option<i64>,
    cost: Option<f64>,
    tokens: Counts,
}

fn messages(
    conn: &Connection,
    unparsed: &mut usize,
) -> rusqlite::Result<BTreeMap<String, Vec<MessageRecord>>> {
    let mut result: BTreeMap<String, Vec<MessageRecord>> = BTreeMap::new();
    if !table_exists(conn, "message")? {
        return Ok(result);
    }
    let columns = columns(conn, "message")?;
    if !["id", "session_id", "data"]
        .iter()
        .all(|name| columns.contains(*name))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let mut statement = conn.prepare("SELECT json_valid(data), json_object('id', id, 'session_id', session_id,
        'role', json_extract(safe, '$.role'),
        'model_id', COALESCE(json_extract(safe, '$.modelID'), json_extract(safe, '$.model.modelID'), json_extract(safe, '$.model.id')),
        'provider_id', COALESCE(json_extract(safe, '$.providerID'), json_extract(safe, '$.model.providerID')),
        'agent', COALESCE(json_extract(safe, '$.agent'), json_extract(safe, '$.mode')),
        'created', json_extract(safe, '$.time.created'), 'completed', json_extract(safe, '$.time.completed'),
        'cost', json_extract(safe, '$.cost'),
        'tokens', json_object('input', json_extract(safe, '$.tokens.input'), 'output', json_extract(safe, '$.tokens.output'),
            'reasoning', json_extract(safe, '$.tokens.reasoning'), 'cache_read', json_extract(safe, '$.tokens.cache.read'),
            'cache_write', json_extract(safe, '$.tokens.cache.write'), 'total', json_extract(safe, '$.tokens.total')))
        FROM (SELECT id, session_id, data, CASE WHEN json_valid(data) THEN data ELSE '{}' END AS safe FROM message)
        ORDER BY id")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, bool>(0)?, row.get::<_, String>(1)?))
    })? {
        let (valid, json) = row?;
        if !valid {
            *unparsed += 1;
            continue;
        }
        let message = match serde_json::from_str::<Message>(&json) {
            Ok(message) => message,
            Err(_) => {
                *unparsed += 1;
                continue;
            }
        };
        match message.role.as_deref() {
            Some("assistant") => {}
            Some("user" | "system") => continue,
            _ => {
                *unparsed += 1;
                continue;
            }
        }
        if message.id.is_empty()
            || message.session_id.is_empty()
            || !valid_counts(&message.tokens.raw())
        {
            *unparsed += 1;
            continue;
        }
        let normalized = opencode_message(&OpenCodeMessageRow {
            id: &message.id,
            session_id: &message.session_id,
            model_id: message.model_id.as_deref(),
            provider_id: message.provider_id.as_deref(),
            agent: message.agent.as_deref(),
            created_ms: message.created,
            completed_ms: message.completed,
            cost_usd: message.cost.filter(|cost| cost.is_finite() && *cost >= 0.0),
            usage: message.tokens.raw(),
        });
        result
            .entry(message.session_id)
            .or_default()
            .push(normalized);
    }
    Ok(result)
}

pub(super) fn snapshot(conn: &Connection, source_path: &str) -> rusqlite::Result<Snapshot> {
    let mut unparsed = 0;
    let has_legacy = table_exists(conn, "session")?;
    let has_current = table_exists(conn, "session_v2")?;
    if !has_legacy && !has_current {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let legacy = if has_legacy {
        sessions(conn, "session", &mut unparsed)?
    } else {
        BTreeMap::new()
    };
    let current = if has_current {
        sessions(conn, "session_v2", &mut unparsed)?
    } else {
        BTreeMap::new()
    };
    let ids: BTreeSet<_> = legacy.keys().chain(current.keys()).collect();
    let mut messages = messages(conn, &mut unparsed)?;
    let mut imports = Vec::new();
    for id in ids {
        let legacy_model = legacy
            .get(id)
            .and_then(|session| model_label(session.model.as_ref()));
        let current_model = current
            .get(id)
            .and_then(|session| model_label(session.model.as_ref()));
        let legacy_row = legacy
            .get(id)
            .map(|session| session.row(legacy_model.as_deref()));
        let current_row = current
            .get(id)
            .map(|session| session.row(current_model.as_deref()));
        let Some(row) = pick_opencode_session(legacy_row.as_ref(), current_row.as_ref()) else {
            continue;
        };
        let mut session: SessionRecord = opencode_session(row);
        let details = messages.remove(id).unwrap_or_default();
        if session.model.is_none() {
            session.model = details
                .iter()
                .max_by_key(|message| message.created_at_ms)
                .and_then(|message| message.model.clone());
        }
        session.coverage = if details.is_empty() {
            MessageCoverage::NoMessages
        } else {
            let total = details.iter().try_fold(0u64, |sum, message| {
                sum.checked_add(message.usage.total_tokens?)
            });
            if total.is_some() && total == session.usage.total_tokens {
                MessageCoverage::Complete
            } else {
                MessageCoverage::Partial
            }
        };
        imports.push(SessionImport {
            session,
            messages: details,
            source_path: source_path.to_string(),
            message_source_paths: BTreeMap::new(),
        });
    }
    unparsed += messages.values().map(Vec::len).sum::<usize>();
    Ok(Snapshot {
        sessions: imports,
        unparsed,
    })
}
