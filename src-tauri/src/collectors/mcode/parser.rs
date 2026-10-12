//! SQL explicitly projects metadata columns; no record_json, raw, cost or history.
use crate::usage::{
    model::MessageRecord,
    normalize::{
        mcode,
        request_ledger::{RequestMeta, SessionMeta},
    },
    store::SessionImport,
    tokens::RawTokenCounts,
};
use rusqlite::{Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Snapshot {
    pub sessions: Vec<SessionImport>,
    pub unparsed: usize,
}

fn columns(conn: &Connection, table: &str) -> rusqlite::Result<BTreeSet<String>> {
    conn.prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect()
}

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

fn number(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(index)?
        .map(|value| {
            u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
        })
        .transpose()
}

fn timestamp(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<i64>> {
    let value: Option<i64> = row.get(index)?;
    if let Some(value) = value.filter(|value| *value < 0) {
        return Err(rusqlite::Error::IntegralValueOutOfRange(index, value));
    }
    Ok(value)
}

pub(super) fn snapshot(conn: &Connection, path: &str) -> rusqlite::Result<Snapshot> {
    let session_columns = columns(conn, "local_runtime_sessions")?;
    if !session_columns.contains("session_id") {
        return Err(rusqlite::Error::InvalidColumnName("session_id".into()));
    }
    let sql = format!(
        "SELECT {} FROM local_runtime_sessions ORDER BY session_id",
        project(
            &session_columns,
            &[
                "session_id",
                "parent_session_id",
                "workspace_dir",
                "created_at_ms",
                "updated_at_ms",
            ]
        )
    );
    let mut sessions = BTreeMap::new();
    let mut unparsed = 0;
    let mut statement = conn.prepare(&sql)?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let parsed = (|| -> rusqlite::Result<_> {
            Ok(SessionMeta {
                id: row.get(0)?,
                parent: row.get(1)?,
                project: row.get(2)?,
                start_ms: timestamp(row, 3)?,
                end_ms: timestamp(row, 4)?,
            })
        })();
        let Ok(meta) = parsed else {
            unparsed += 1;
            continue;
        };
        if meta.id.is_empty() {
            unparsed += 1;
            continue;
        }
        if sessions.insert(meta.id.clone(), meta).is_some() {
            return Err(rusqlite::Error::InvalidQuery);
        }
    }
    let has_usage = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='local_runtime_token_usage'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let mut details = BTreeMap::<String, Vec<(MessageRecord, RawTokenCounts)>>::new();
    let mut partial = BTreeSet::new();
    if has_usage {
        let usage_columns = columns(conn, "local_runtime_token_usage")?;
        for required in ["id", "session_id"] {
            if !usage_columns.contains(required) {
                return Err(rusqlite::Error::InvalidColumnName(required.into()));
            }
        }
        let sql = format!(
            "SELECT {} FROM local_runtime_token_usage ORDER BY id",
            project(
                &usage_columns,
                &[
                    "id",
                    "session_id",
                    "agent_name",
                    "model",
                    "ts",
                    "input_tokens",
                    "output_tokens",
                    "reasoning_tokens",
                    "cache_read_tokens",
                    "cache_write_tokens",
                ]
            )
        );
        let mut statement = conn.prepare(&sql)?;
        let mut rows = statement.query([])?;
        let mut ids = BTreeSet::new();
        while let Some(row) = rows.next()? {
            // Without a usable session reference, abort rather than replace an
            // existing complete snapshot with silently unassigned usage.
            let sid: String = row.get(1)?;
            let parsed = (|| -> rusqlite::Result<_> {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    timestamp(row, 4)?,
                    RawTokenCounts {
                        input: number(row, 5)?,
                        output: number(row, 6)?,
                        reasoning: number(row, 7)?,
                        cache_read: number(row, 8)?,
                        cache_write: number(row, 9)?,
                        client_total: None,
                    },
                ))
            })();
            let Ok((id, agent, model, created_ms, raw)) = parsed else {
                unparsed += 1;
                partial.insert(sid);
                continue;
            };
            if id < 0
                || !crate::collectors::files::valid_counts(&raw)
                || !sessions.contains_key(&sid)
            {
                unparsed += 1;
                partial.insert(sid);
                continue;
            }
            if !ids.insert(id) {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let meta = RequestMeta {
                key: format!("usage:{id}"),
                model: model.filter(|model| !model.trim().is_empty()),
                agent,
                created_ms,
                completed_ms: None,
                sidechain: false,
                source_ref: format!("local_runtime_token_usage.id={id}"),
            };
            details
                .entry(sid.clone())
                .or_default()
                .push((mcode::request(&sid, meta, raw), raw));
        }
    }
    let sessions = sessions
        .into_iter()
        .map(|(id, meta)| {
            let (messages, counts): (Vec<_>, Vec<_>) =
                details.remove(&id).unwrap_or_default().into_iter().unzip();
            SessionImport {
                session: mcode::session(
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
