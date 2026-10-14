//! SQL projects only identities, times, models and usage counters.
use crate::usage::{
    model::MessageRecord,
    normalize::{
        copilot,
        request_ledger::{RequestMeta, SessionMeta},
    },
    store::SessionImport,
    time::parse_iso8601_utc_ms,
    tokens::RawTokenCounts,
};
use rusqlite::{Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};

fn count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(index)?
        .map(|value| {
            u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
        })
        .transpose()
}

pub(super) struct Snapshot {
    pub sessions: Vec<SessionImport>,
    pub unparsed: usize,
}

fn columns(conn: &Connection, table: &str) -> rusqlite::Result<BTreeSet<String>> {
    // Static allowlisted identifiers only; never interpolate user input here.
    conn.prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect()
}

/// Emit the whitelisted columns in a fixed order; absent columns become `NULL AS col`
/// so older schema revisions stay readable instead of failing the whole source.
fn project(columns: &BTreeSet<String>, whitelist: &[&str]) -> String {
    whitelist
        .iter()
        .map(|column| {
            if columns.contains(*column) {
                (*column).to_string()
            } else {
                format!("NULL AS {column}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn time(value: Option<String>) -> Option<i64> {
    let value = value?;
    parse_iso8601_utc_ms(&value).or_else(|| {
        // SQLite datetime('now') defaults are explicitly UTC.
        if value.len() == 19 && value.as_bytes()[10] == b' ' {
            parse_iso8601_utc_ms(&format!("{}T{}Z", &value[..10], &value[11..]))
        } else {
            None
        }
    })
}

pub(super) fn snapshot(conn: &Connection, path: &str) -> rusqlite::Result<Snapshot> {
    let mut sessions = BTreeMap::new();
    let mut unparsed = 0;
    let session_columns = columns(conn, "sessions")?;
    if !session_columns.contains("id") {
        return Err(rusqlite::Error::InvalidColumnName("id".into()));
    }
    let session_sql = format!(
        "SELECT {} FROM sessions ORDER BY id",
        project(&session_columns, &["id", "cwd", "created_at", "updated_at"])
    );
    let mut stmt = conn.prepare(&session_sql)?;
    for row in stmt.query_map([], |row| {
        Ok(SessionMeta {
            id: row.get(0)?,
            parent: None,
            project: row.get(1)?,
            start_ms: time(row.get(2)?),
            end_ms: time(row.get(3)?),
        })
    })? {
        let meta = row?;
        if meta.id.is_empty() {
            unparsed += 1;
            continue;
        }
        sessions.insert(meta.id.clone(), meta);
    }
    let has_usage = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='assistant_usage_events'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let mut details: BTreeMap<String, Vec<(MessageRecord, RawTokenCounts)>> = BTreeMap::new();
    let mut partial = BTreeSet::new();
    if has_usage {
        let usage_columns = columns(conn, "assistant_usage_events")?;
        for required in ["id", "session_id"] {
            if !usage_columns.contains(required) {
                return Err(rusqlite::Error::InvalidColumnName(required.into()));
            }
        }
        let usage_sql = format!(
            "SELECT {} FROM assistant_usage_events ORDER BY id",
            project(
                &usage_columns,
                &[
                    "id",
                    "session_id",
                    "agent_id",
                    "parent_tool_call_id",
                    "model",
                    "input_tokens",
                    "output_tokens",
                    "cache_read_tokens",
                    "cache_write_tokens",
                    "reasoning_tokens",
                    "created_at",
                ]
            )
        );
        let mut stmt = conn.prepare(&usage_sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let sid: String = row.get(1)?;
            let metadata = (|| -> rusqlite::Result<_> {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    RawTokenCounts {
                        input: count(row, 5)?,
                        output: count(row, 6)?,
                        cache_read: count(row, 7)?,
                        cache_write: count(row, 8)?,
                        reasoning: count(row, 9)?,
                        client_total: None,
                    },
                    time(row.get(10)?),
                ))
            })();
            let Ok((id, agent, parent_tool, model, raw, created_ms)) = metadata else {
                unparsed += 1;
                partial.insert(sid);
                continue;
            };
            if !sessions.contains_key(&sid) {
                unparsed += 1;
                partial.insert(sid);
                continue;
            }
            let sidechain = agent.is_some() || parent_tool.is_some();
            let record = copilot::request(
                &sid,
                RequestMeta {
                    key: format!("usage:{id}"),
                    model,
                    agent,
                    created_ms,
                    completed_ms: None,
                    sidechain,
                    source_ref: format!("assistant_usage_events.id={id}"),
                },
                raw,
            );
            details.entry(sid).or_default().push(record);
        }
    }
    let sessions = sessions
        .into_iter()
        .map(|(id, meta)| {
            let (messages, counts): (Vec<_>, Vec<_>) =
                details.remove(&id).unwrap_or_default().into_iter().unzip();
            SessionImport {
                session: copilot::session(
                    meta,
                    &messages,
                    &counts,
                    !has_usage || partial.contains(&id),
                ),
                messages,
                source_path: path.to_string(),
                message_source_paths: BTreeMap::new(),
            }
        })
        .collect();
    Ok(Snapshot { sessions, unparsed })
}
