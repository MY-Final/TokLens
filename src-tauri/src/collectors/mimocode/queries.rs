//! Project only statistics in SQL; bodies, tool parts, memories and account tables stay in the source.
use crate::collectors::files::valid_counts;
use crate::usage::{
    client::Client,
    model::MessageRecord,
    normalize::request_ledger::{self, RequestMeta, SessionMeta},
    store::SessionImport,
    tokens::RawTokenCounts,
};
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
struct Row {
    id: String,
    session_id: String,
    role: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    agent: Option<String>,
    created: Option<i64>,
    completed: Option<i64>,
    tokens: Counts,
}
pub(super) struct Snapshot {
    pub sessions: Vec<SessionImport>,
    pub unparsed: usize,
}
fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}
struct Imported {
    sessions: BTreeSet<String>,
    messages: BTreeSet<(String, String)>,
}
fn imports(conn: &Connection) -> rusqlite::Result<Imported> {
    let mut imported = Imported {
        sessions: BTreeSet::new(),
        messages: BTreeSet::new(),
    };
    for name in ["claude_import", "external_import"] {
        if table_exists(conn, name)? {
            // Static allowlisted identifiers; only imported message/session identities are read.
            let mut query = conn.prepare(&format!("SELECT session_id, message_ids FROM {name}"))?;
            for row in
                query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            {
                let (sid, json) = row?;
                let ids: Vec<String> =
                    serde_json::from_str(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
                imported.sessions.insert(sid.clone());
                imported
                    .messages
                    .extend(ids.into_iter().map(|id| (sid.clone(), id)));
            }
        }
    }
    Ok(imported)
}
pub(super) fn snapshot(conn: &Connection, source_path: &str) -> rusqlite::Result<Snapshot> {
    let excluded = imports(conn)?;
    let mut sessions = BTreeMap::new();
    let mut unparsed = 0;
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, directory, time_created, time_updated FROM session ORDER BY id",
    )?;
    for row in stmt.query_map([], |row| {
        Ok(SessionMeta {
            id: row.get(0)?,
            parent: row.get(1)?,
            project: row.get(2)?,
            start_ms: row.get(3)?,
            end_ms: row.get(4)?,
        })
    })? {
        let meta = row?;
        if meta.id.is_empty() {
            unparsed += 1;
            continue;
        }
        sessions.insert(meta.id.clone(), meta);
    }
    let mut details: BTreeMap<String, Vec<(MessageRecord, RawTokenCounts)>> = BTreeMap::new();
    let mut partial = BTreeSet::new();
    let mut stmt = conn.prepare("SELECT json_valid(data), session_id, json_object('id',id,'session_id',session_id,
        'role',json_extract(safe,'$.role'),'model',json_extract(safe,'$.modelID'),'provider',json_extract(safe,'$.providerID'),
        'agent',agent_id,'created',COALESCE(json_extract(safe,'$.time.created'),time_created),'completed',json_extract(safe,'$.time.completed'),
        'tokens',json_object('input',json_extract(safe,'$.tokens.input'),'output',json_extract(safe,'$.tokens.output'),
        'reasoning',json_extract(safe,'$.tokens.reasoning'),'cache_read',json_extract(safe,'$.tokens.cache.read'),
        'cache_write',json_extract(safe,'$.tokens.cache.write'),'total',json_extract(safe,'$.tokens.total')))
        FROM (SELECT id,session_id,agent_id,time_created,data,CASE WHEN json_valid(data) THEN data ELSE '{}' END AS safe FROM message) ORDER BY id")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, bool>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (valid, sid, json) = row?;
        let row = if valid {
            serde_json::from_str::<Row>(&json).ok()
        } else {
            None
        };
        let Some(row) = row else {
            unparsed += 1;
            partial.insert(sid);
            continue;
        };
        if excluded.messages.contains(&(sid.clone(), row.id.clone())) {
            continue;
        }
        match row.role.as_deref() {
            Some("assistant") => {}
            Some("user" | "system") => continue,
            _ => {
                unparsed += 1;
                partial.insert(sid);
                continue;
            }
        }
        let raw = row.tokens.raw();
        if row.id.is_empty() || !sessions.contains_key(&sid) || !valid_counts(&raw) {
            unparsed += 1;
            partial.insert(sid);
            continue;
        }
        let model = row.model.map(|model| {
            row.provider
                .map_or_else(|| model.clone(), |provider| format!("{provider}/{model}"))
        });
        let sidechain = row.agent.as_deref().is_some_and(|agent| agent != "main");
        let message = request_ledger::request(
            Client::MimoCode,
            &row.session_id,
            RequestMeta {
                key: row.id.clone(),
                model,
                agent: row.agent,
                created_ms: row.created,
                completed_ms: row.completed,
                sidechain,
                source_ref: format!("message_id={}", row.id),
            },
            &raw,
        );
        details.entry(sid).or_default().push((message, raw));
    }
    let mut result = Vec::new();
    for (id, meta) in sessions {
        // Imported sessions stay visible only if MiMo has made its own requests afterwards.
        if excluded.sessions.contains(&id) && !details.contains_key(&id) {
            continue;
        }
        let (messages, counts): (Vec<_>, Vec<_>) =
            details.remove(&id).unwrap_or_default().into_iter().unzip();
        result.push(SessionImport {
            session: request_ledger::session(
                Client::MimoCode,
                meta,
                &messages,
                &counts,
                partial.contains(&id),
            ),
            messages,
            source_path: source_path.to_string(),
            message_source_paths: BTreeMap::new(),
        });
    }
    Ok(Snapshot {
        sessions: result,
        unparsed,
    })
}
