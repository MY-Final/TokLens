//! Idempotent absolute snapshot writes and stale-record guards.

use super::{SourceFile, UpsertOutcome, UsageError, UsageStore};
use crate::usage::model::{
    MessageRecord, SessionRecord, SessionTotalBasis, encode_unknown_relations, message_id,
    session_id,
};
use crate::usage::time::now_ms;
use rusqlite::{Connection, OptionalExtension, params};

impl UsageStore {
    /// 写入/更新源文件水位，返回行 id。
    pub fn upsert_source_file(&self, file: &SourceFile) -> Result<i64, UsageError> {
        upsert_source_file_conn(&self.conn, file)
    }

    pub fn upsert_session(&self, session: &SessionRecord) -> Result<UpsertOutcome, UsageError> {
        upsert_session_conn(&self.conn, session)
    }

    pub fn upsert_message(
        &self,
        message: &MessageRecord,
        source_file_id: Option<i64>,
    ) -> Result<UpsertOutcome, UsageError> {
        upsert_message_conn(&self.conn, message, source_file_id)
    }
}

/// Token 计数在库中以 i64 存储（SQLite 整数列），u64 不会出现在绑定参数里。
fn opt_i64(value: Option<u64>) -> Option<i64> {
    value.map(|value| value as i64)
}

pub(super) fn upsert_source_file_conn(
    conn: &Connection,
    file: &SourceFile,
) -> Result<i64, UsageError> {
    if file.path.is_empty() {
        return Err(UsageError::InvalidInput("source file path is empty".into()));
    }
    conn.execute(
        "INSERT INTO source_files
             (client, path, kind, size_bytes, mtime_ms, content_offset, parser_version, last_scan_ms, unparsed_records)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (client, path) DO UPDATE SET
             kind = excluded.kind,
             size_bytes = excluded.size_bytes,
             mtime_ms = excluded.mtime_ms,
             content_offset = excluded.content_offset,
             parser_version = excluded.parser_version,
             last_scan_ms = excluded.last_scan_ms,
             unparsed_records = excluded.unparsed_records",
        params![
            file.client.as_str(),
            file.path,
            file.kind.as_str(),
            file.size_bytes,
            file.mtime_ms,
            file.content_offset,
            file.parser_version,
            file.last_scan_ms,
            file.unparsed_records,
        ],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM source_files WHERE client = ?1 AND path = ?2",
        params![file.client.as_str(), file.path],
        |row| row.get(0),
    )?)
}

/// 会话写入的陈旧判定：只看"绝对值"与时间，不做任何累加。
fn should_replace_session(
    existing_total: Option<i64>,
    existing_basis: Option<&str>,
    existing_updated_at: i64,
    incoming_total: Option<i64>,
    incoming_updated_at: i64,
) -> bool {
    if existing_basis.is_none() {
        return true;
    }
    // 新快照完全没有 token 数据（例如文件刚追加、尚未写入用量）时，不得抹掉已知总量。
    if incoming_total.is_none() && existing_total.is_some() {
        return false;
    }
    // 累计值口径：累计值不得回退（防止用旧文件覆盖新快照）。
    if existing_basis == Some(SessionTotalBasis::ClientCumulative.as_str())
        && let (Some(existing), Some(incoming)) = (existing_total, incoming_total)
        && incoming < existing
    {
        return false;
    }
    incoming_updated_at >= existing_updated_at
}

pub(super) fn upsert_session_conn(
    conn: &Connection,
    session: &SessionRecord,
) -> Result<UpsertOutcome, UsageError> {
    if session.client_session_id.is_empty() {
        return Err(UsageError::InvalidInput("session id is empty".into()));
    }
    let existing = conn
        .query_row(
            "SELECT total_tokens, total_basis, updated_at_ms FROM sessions
             WHERE client = ?1 AND client_session_id = ?2",
            params![session.client.as_str(), session.client_session_id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let is_new = existing.is_none();
    let now = now_ms();
    let incoming_updated_at = session.ended_at_ms.unwrap_or(now);
    let (existing_total, existing_basis, existing_updated_at) = match &existing {
        Some((total, basis, updated_at)) => (*total, basis.as_deref(), *updated_at),
        None => (None, None, i64::MIN),
    };
    if !should_replace_session(
        existing_total,
        existing_basis,
        existing_updated_at,
        opt_i64(session.usage.total_tokens),
        incoming_updated_at,
    ) {
        return Ok(UpsertOutcome::SkippedStale);
    }
    let buckets = session.usage.buckets;
    conn.execute(
        "INSERT INTO sessions
             (id, client, client_session_id, parent_client_session_id, project_path, model,
              started_at_ms, ended_at_ms, semantics_id, total_basis, message_coverage, source_kind,
              input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_write_tokens,
              total_tokens, client_total_tokens, client_total_mismatch, unknown_relations,
              cost_source, client_cost_usd, imported_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
         ON CONFLICT (client, client_session_id) DO UPDATE SET
             parent_client_session_id = excluded.parent_client_session_id,
             project_path = excluded.project_path,
             model = excluded.model,
             started_at_ms = excluded.started_at_ms,
             ended_at_ms = excluded.ended_at_ms,
             semantics_id = excluded.semantics_id,
             total_basis = excluded.total_basis,
             message_coverage = excluded.message_coverage,
             source_kind = excluded.source_kind,
             input_tokens = excluded.input_tokens,
             output_tokens = excluded.output_tokens,
             reasoning_tokens = excluded.reasoning_tokens,
             cache_read_tokens = excluded.cache_read_tokens,
             cache_write_tokens = excluded.cache_write_tokens,
             total_tokens = excluded.total_tokens,
             client_total_tokens = excluded.client_total_tokens,
             client_total_mismatch = excluded.client_total_mismatch,
             unknown_relations = excluded.unknown_relations,
             cost_source = excluded.cost_source,
             client_cost_usd = excluded.client_cost_usd,
             updated_at_ms = excluded.updated_at_ms",
        params![
            session_id(session.client, &session.client_session_id),
            session.client.as_str(),
            session.client_session_id,
            session.parent_client_session_id,
            session.project_path,
            session.model,
            session.started_at_ms,
            session.ended_at_ms,
            session.semantics_id,
            session.total_basis.as_str(),
            session.coverage.as_str(),
            session.source_kind.as_str(),
            opt_i64(buckets.input),
            opt_i64(buckets.output),
            opt_i64(buckets.reasoning),
            opt_i64(buckets.cache_read),
            opt_i64(buckets.cache_write),
            opt_i64(session.usage.total_tokens),
            opt_i64(session.usage.total_tokens),
            i64::from(session.usage.client_total_mismatch),
            encode_unknown_relations(&session.usage.unknown_relations),
            session.cost_source.as_str(),
            session.client_cost_usd,
            now,
            incoming_updated_at,
        ],
    )?;
    Ok(if is_new {
        UpsertOutcome::Inserted
    } else {
        UpsertOutcome::Updated
    })
}
pub(super) fn upsert_message_conn(
    conn: &Connection,
    message: &MessageRecord,
    source_file_id: Option<i64>,
) -> Result<UpsertOutcome, UsageError> {
    if message.dedup_key.is_empty() {
        return Err(UsageError::InvalidInput(
            "message dedup key is empty".into(),
        ));
    }
    let session_key = session_id(message.client, &message.client_session_id);
    let existing_created: Option<Option<i64>> = conn
        .query_row(
            "SELECT created_at_ms FROM messages
             WHERE client = ?1 AND session_id = ?2 AND dedup_key = ?3",
            params![message.client.as_str(), session_key, message.dedup_key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(Some(existing_created)) = existing_created
        && let Some(incoming_created) = message.created_at_ms
        && incoming_created < existing_created
    {
        return Ok(UpsertOutcome::SkippedStale);
    }
    let is_new = existing_created.is_none();
    let now = now_ms();
    let buckets = message.usage.buckets;
    conn.execute(
        "INSERT INTO messages
             (id, session_id, client, dedup_key, seq, created_at_ms, completed_at_ms, model, agent,
              semantics_id, source_kind, input_tokens, output_tokens, reasoning_tokens,
              cache_read_tokens, cache_write_tokens, total_tokens, client_total_tokens,
              client_total_mismatch, unknown_relations, is_sidechain, source_file_id, source_ref,
              cost_source, client_cost_usd, imported_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
         ON CONFLICT (client, session_id, dedup_key) DO UPDATE SET
             seq = excluded.seq,
             created_at_ms = excluded.created_at_ms,
             completed_at_ms = excluded.completed_at_ms,
             model = excluded.model,
             agent = excluded.agent,
             semantics_id = excluded.semantics_id,
             source_kind = excluded.source_kind,
             input_tokens = excluded.input_tokens,
             output_tokens = excluded.output_tokens,
             reasoning_tokens = excluded.reasoning_tokens,
             cache_read_tokens = excluded.cache_read_tokens,
             cache_write_tokens = excluded.cache_write_tokens,
             total_tokens = excluded.total_tokens,
             client_total_tokens = excluded.client_total_tokens,
             client_total_mismatch = excluded.client_total_mismatch,
             unknown_relations = excluded.unknown_relations,
             is_sidechain = excluded.is_sidechain,
             source_file_id = excluded.source_file_id,
             source_ref = excluded.source_ref,
             cost_source = excluded.cost_source,
             client_cost_usd = excluded.client_cost_usd,
             updated_at_ms = excluded.updated_at_ms",
        params![
            message_id(
                message.client,
                &message.client_session_id,
                &message.dedup_key
            ),
            session_key,
            message.client.as_str(),
            message.dedup_key,
            message.seq,
            message.created_at_ms,
            message.completed_at_ms,
            message.model,
            message.agent,
            message.semantics_id,
            message.source_kind.as_str(),
            opt_i64(buckets.input),
            opt_i64(buckets.output),
            opt_i64(buckets.reasoning),
            opt_i64(buckets.cache_read),
            opt_i64(buckets.cache_write),
            opt_i64(message.usage.total_tokens),
            opt_i64(message.usage.total_tokens),
            i64::from(message.usage.client_total_mismatch),
            encode_unknown_relations(&message.usage.unknown_relations),
            i64::from(message.is_sidechain),
            source_file_id,
            message.source_ref,
            message.cost_source.as_str(),
            message.client_cost_usd,
            now,
            message.created_at_ms.unwrap_or(now),
        ],
    )?;
    Ok(if is_new {
        UpsertOutcome::Inserted
    } else {
        UpsertOutcome::Updated
    })
}
