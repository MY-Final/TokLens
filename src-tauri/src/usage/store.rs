//! 本地 SQLite 持久化：版本化迁移、幂等导入、增量同步水位。
//!
//! 设计要点：
//!
//! - `sessions` 是统计总量的**唯一事实来源**；`messages` 只用于时间序列与明细，
//!   两者绝不相加（Codex 累计值、OpenCode 会话聚合同理）。
//! - 所有写入都是"绝对值快照"而不是"增量累加"，因此重复导入天然幂等。
//! - `source_files` 保存文件大小 / mtime / 已消费字节偏移，支持增量同步与应用重启后的续读。
//! - 费用只保存客户端自报值，不做价格换算（费用与 Token 采集解耦）。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::usage::model::{
    MessageRecord, SessionRecord, SessionTotalBasis, decode_unknown_relations,
    encode_unknown_relations, message_id, now_ms, session_id,
};
use crate::usage::tokens::Client;

/// 存储层错误。
#[derive(Debug)]
pub enum UsageError {
    Database(rusqlite::Error),
    /// 应用数据目录创建/写入失败。
    Io(std::io::Error),
    InvalidInput(String),
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsageError::Database(error) => write!(f, "usage database error: {error}"),
            UsageError::Io(error) => write!(f, "usage storage io error: {error}"),
            UsageError::InvalidInput(message) => write!(f, "invalid usage input: {message}"),
        }
    }
}

impl std::error::Error for UsageError {}

impl From<rusqlite::Error> for UsageError {
    fn from(error: rusqlite::Error) -> Self {
        UsageError::Database(error)
    }
}

impl From<std::io::Error> for UsageError {
    fn from(error: std::io::Error) -> Self {
        UsageError::Io(error)
    }
}

/// 源文件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFileKind {
    Jsonl,
    Sqlite,
}

impl SourceFileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceFileKind::Jsonl => "jsonl",
            SourceFileKind::Sqlite => "sqlite",
        }
    }

    pub fn parse(value: &str) -> Option<SourceFileKind> {
        match value {
            "jsonl" => Some(SourceFileKind::Jsonl),
            "sqlite" => Some(SourceFileKind::Sqlite),
            _ => None,
        }
    }
}

/// 源文件水位（增量同步与重启恢复的依据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub client: Client,
    pub path: String,
    pub kind: SourceFileKind,
    pub size_bytes: Option<i64>,
    pub mtime_ms: Option<i64>,
    /// 已消费的字节偏移（JSONL 追加续读用）。
    pub content_offset: i64,
    /// 解析器版本；版本变化时应重新解析该文件。
    pub parser_version: i64,
    pub last_scan_ms: Option<i64>,
    /// 未识别/无法解析的记录数（供 UI 提示，不做静默丢弃）。
    pub unparsed_records: i64,
}

/// 读回的源文件行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileRow {
    pub id: i64,
    pub client: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub mtime_ms: Option<i64>,
    pub content_offset: i64,
    pub parser_version: i64,
    pub last_scan_ms: Option<i64>,
    pub unparsed_records: i64,
}

/// 写入结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    Updated,
    /// 输入比库中已有数据更旧（或累计值回退），已忽略。
    SkippedStale,
}

/// 一次导入的结果摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportSummary {
    pub session: Option<UpsertOutcome>,
    pub messages_inserted: usize,
    pub messages_updated: usize,
    pub messages_skipped_stale: usize,
    /// 覆盖式导入（`replace_file_import`）中删除的、该源文件此前写入的明细数。
    pub messages_pruned: usize,
}

/// 统计过滤条件。
#[derive(Debug, Clone, Copy, Default)]
pub struct TotalsFilter<'a> {
    pub client: Option<Client>,
    pub project_path: Option<&'a str>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}

/// 统计结果（从 `sessions` 表求和）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageTotals {
    pub sessions: i64,
    pub messages: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
}

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

/// 版本化迁移表。新增迁移只能追加，不得修改已发布的迁移。
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial_usage_schema",
    sql: INITIAL_SCHEMA_SQL,
}];

const INITIAL_SCHEMA_SQL: &str = r#"
CREATE TABLE source_files (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    client           TEXT    NOT NULL,
    path             TEXT    NOT NULL,
    kind             TEXT    NOT NULL CHECK (kind IN ('jsonl', 'sqlite')),
    size_bytes       INTEGER,
    mtime_ms         INTEGER,
    content_offset   INTEGER NOT NULL DEFAULT 0,
    parser_version   INTEGER NOT NULL,
    last_scan_ms     INTEGER,
    unparsed_records INTEGER NOT NULL DEFAULT 0,
    UNIQUE (client, path)
);

CREATE TABLE sessions (
    id                       TEXT PRIMARY KEY,
    client                   TEXT    NOT NULL,
    client_session_id        TEXT    NOT NULL,
    parent_client_session_id TEXT,
    project_path             TEXT,
    model                    TEXT,
    started_at_ms            INTEGER,
    ended_at_ms              INTEGER,
    semantics_id             TEXT    NOT NULL,
    total_basis              TEXT    NOT NULL CHECK (total_basis IN ('client_cumulative', 'client_reported', 'sum_of_messages')),
    message_coverage         TEXT    NOT NULL CHECK (message_coverage IN ('complete', 'partial', 'none')),
    source_kind              TEXT    NOT NULL CHECK (source_kind IN ('measured', 'derived', 'estimated')),
    input_tokens             INTEGER,
    output_tokens            INTEGER,
    reasoning_tokens         INTEGER,
    cache_read_tokens        INTEGER,
    cache_write_tokens       INTEGER,
    total_tokens             INTEGER,
    client_total_tokens      INTEGER,
    client_total_mismatch    INTEGER NOT NULL DEFAULT 0 CHECK (client_total_mismatch IN (0, 1)),
    unknown_relations        TEXT,
    cost_source              TEXT    NOT NULL CHECK (cost_source IN ('client_reported', 'none')),
    client_cost_usd          REAL,
    imported_at_ms           INTEGER NOT NULL,
    updated_at_ms            INTEGER NOT NULL,
    UNIQUE (client, client_session_id)
);

CREATE INDEX idx_sessions_client_started ON sessions (client, started_at_ms);
CREATE INDEX idx_sessions_project_started ON sessions (project_path, started_at_ms);

CREATE TABLE messages (
    id                    TEXT PRIMARY KEY,
    session_id            TEXT    NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    client                TEXT    NOT NULL,
    dedup_key             TEXT    NOT NULL,
    seq                   INTEGER,
    created_at_ms         INTEGER,
    completed_at_ms       INTEGER,
    model                 TEXT,
    agent                 TEXT,
    semantics_id          TEXT    NOT NULL,
    source_kind           TEXT    NOT NULL CHECK (source_kind IN ('measured', 'derived', 'estimated')),
    input_tokens          INTEGER,
    output_tokens         INTEGER,
    reasoning_tokens      INTEGER,
    cache_read_tokens     INTEGER,
    cache_write_tokens    INTEGER,
    total_tokens          INTEGER,
    client_total_tokens   INTEGER,
    client_total_mismatch INTEGER NOT NULL DEFAULT 0 CHECK (client_total_mismatch IN (0, 1)),
    unknown_relations     TEXT,
    is_sidechain          INTEGER NOT NULL DEFAULT 0 CHECK (is_sidechain IN (0, 1)),
    source_file_id        INTEGER REFERENCES source_files (id) ON DELETE SET NULL,
    source_ref            TEXT,
    cost_source           TEXT    NOT NULL CHECK (cost_source IN ('client_reported', 'none')),
    client_cost_usd       REAL,
    imported_at_ms        INTEGER NOT NULL,
    updated_at_ms         INTEGER NOT NULL,
    UNIQUE (client, session_id, dedup_key)
);

CREATE INDEX idx_messages_session_created ON messages (session_id, created_at_ms);
CREATE INDEX idx_messages_client_created ON messages (client, created_at_ms);
CREATE INDEX idx_messages_model ON messages (model);
"#;
/// TokLens 本地用量库。
pub struct UsageStore {
    pub(crate) conn: Connection,
}

impl UsageStore {
    /// 打开（必要时创建）本地用量库并执行迁移。
    pub fn open(path: &Path) -> Result<Self, UsageError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// 内存库（测试用）。
    pub fn open_in_memory() -> Result<Self, UsageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, UsageError> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    /// 已应用的迁移版本。
    pub fn schema_version(&self) -> Result<u32, UsageError> {
        let version: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        Ok(version as u32)
    }

    pub fn session_count(&self) -> Result<i64, UsageError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?)
    }

    pub fn message_count(&self) -> Result<i64, UsageError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?)
    }

    /// 读取源文件水位；应用重启后据此续读。
    pub fn source_file(
        &self,
        client: Client,
        path: &str,
    ) -> Result<Option<SourceFileRow>, UsageError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, client, path, kind, size_bytes, mtime_ms, content_offset, parser_version,
                        last_scan_ms, unparsed_records
                 FROM source_files WHERE client = ?1 AND path = ?2",
                params![client.as_str(), path],
                |row| {
                    Ok(SourceFileRow {
                        id: row.get(0)?,
                        client: row.get(1)?,
                        path: row.get(2)?,
                        kind: row.get(3)?,
                        size_bytes: row.get(4)?,
                        mtime_ms: row.get(5)?,
                        content_offset: row.get(6)?,
                        parser_version: row.get(7)?,
                        last_scan_ms: row.get(8)?,
                        unparsed_records: row.get(9)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// 覆盖式导入一个源文件的解析结果：先写水位，再写会话快照，然后**删除该源文件此前写入的明细**，
    /// 最后写入本次解析结果。
    ///
    /// 与 `import_session` 的差别：文件被截断/替换后，旧明细必须消失，否则会残留源文件中已不存在的记录。
    /// 单事务提交，中断时整体回滚、源文件水位不前进，因此下次运行会安全地重新解析。
    pub fn replace_file_import(
        &self,
        file: &SourceFile,
        session: &SessionRecord,
        messages: &[MessageRecord],
    ) -> Result<ImportSummary, UsageError> {
        if file.client != session.client {
            return Err(UsageError::InvalidInput(format!(
                "source file client {} does not match session client {}",
                file.client, session.client
            )));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let file_id = upsert_source_file_conn(&transaction, file)?;
        let outcome = upsert_session_conn(&transaction, session)?;
        let pruned = transaction.execute(
            "DELETE FROM messages WHERE source_file_id = ?1",
            params![file_id],
        )?;
        let mut summary = ImportSummary {
            session: Some(outcome),
            messages_pruned: pruned,
            ..ImportSummary::default()
        };
        for message in messages {
            if message.client_session_id != session.client_session_id {
                return Err(UsageError::InvalidInput(format!(
                    "message session {} does not match session {}",
                    message.client_session_id, session.client_session_id
                )));
            }
            match upsert_message_conn(&transaction, message, Some(file_id))? {
                UpsertOutcome::Inserted => summary.messages_inserted += 1,
                UpsertOutcome::Updated => summary.messages_updated += 1,
                UpsertOutcome::SkippedStale => summary.messages_skipped_stale += 1,
            }
        }
        transaction.commit()?;
        Ok(summary)
    }
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

    /// 一次性导入一个会话（源文件水位 + 会话 + 请求明细）。单事务保证原子性：
    /// 中断不会留下半截数据，重跑幂等。
    pub fn import_session(
        &self,
        file: &SourceFile,
        session: &SessionRecord,
        messages: &[MessageRecord],
    ) -> Result<ImportSummary, UsageError> {
        if file.client != session.client {
            return Err(UsageError::InvalidInput(format!(
                "source file client {} does not match session client {}",
                file.client, session.client
            )));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let file_id = upsert_source_file_conn(&transaction, file)?;
        let outcome = upsert_session_conn(&transaction, session)?;
        let mut summary = ImportSummary {
            session: Some(outcome),
            ..ImportSummary::default()
        };
        if outcome != UpsertOutcome::SkippedStale {
            for message in messages {
                if message.client_session_id != session.client_session_id {
                    return Err(UsageError::InvalidInput(format!(
                        "message session {} does not match session {}",
                        message.client_session_id, session.client_session_id
                    )));
                }
                match upsert_message_conn(&transaction, message, Some(file_id))? {
                    UpsertOutcome::Inserted => summary.messages_inserted += 1,
                    UpsertOutcome::Updated => summary.messages_updated += 1,
                    UpsertOutcome::SkippedStale => summary.messages_skipped_stale += 1,
                }
            }
        }
        transaction.commit()?;
        Ok(summary)
    }

    /// 按过滤条件汇总用量。只统计 `sessions` 表，避免与会话明细重复相加。
    pub fn totals(&self, filter: &TotalsFilter<'_>) -> Result<UsageTotals, UsageError> {
        let client = filter.client.map(|client| client.as_str());
        let totals = self.conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_write_tokens), 0),
                    COALESCE(SUM(total_tokens), 0)
             FROM sessions
             WHERE (?1 IS NULL OR client = ?1)
               AND (?2 IS NULL OR project_path = ?2)
               AND (?3 IS NULL OR started_at_ms >= ?3)
               AND (?4 IS NULL OR started_at_ms < ?4)",
            params![client, filter.project_path, filter.from_ms, filter.to_ms],
            |row| {
                Ok(UsageTotals {
                    sessions: row.get(0)?,
                    messages: 0,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    reasoning_tokens: row.get(3)?,
                    cache_read_tokens: row.get(4)?,
                    cache_write_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                })
            },
        )?;
        let messages: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM messages AS m
             JOIN sessions AS s ON s.id = m.session_id
             WHERE (?1 IS NULL OR s.client = ?1)
               AND (?2 IS NULL OR s.project_path = ?2)
               AND (?3 IS NULL OR s.started_at_ms >= ?3)
               AND (?4 IS NULL OR s.started_at_ms < ?4)",
            params![client, filter.project_path, filter.from_ms, filter.to_ms],
            |row| row.get(0),
        )?;
        Ok(UsageTotals { messages, ..totals })
    }

    /// 某个会话的明细求和（一致性校验：OpenCode / Claude Code 的会话列应等于明细求和）。
    pub fn messages_sum(
        &self,
        client: Client,
        client_session_id: &str,
    ) -> Result<UsageTotals, UsageError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_write_tokens), 0),
                    COALESCE(SUM(total_tokens), 0)
             FROM messages WHERE client = ?1 AND session_id = ?2",
            params![client.as_str(), session_id(client, client_session_id)],
            |row| {
                Ok(UsageTotals {
                    sessions: 0,
                    messages: row.get(0)?,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    reasoning_tokens: row.get(3)?,
                    cache_read_tokens: row.get(4)?,
                    cache_write_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                })
            },
        )?)
    }

    /// 读取某个会话行的关键字段（测试与诊断用）。
    pub fn session_tokens(
        &self,
        client: Client,
        client_session_id: &str,
    ) -> Result<Option<UsageTotals>, UsageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(input_tokens, 0), COALESCE(output_tokens, 0), COALESCE(reasoning_tokens, 0),
                        COALESCE(cache_read_tokens, 0), COALESCE(cache_write_tokens, 0), COALESCE(total_tokens, 0)
                 FROM sessions WHERE client = ?1 AND client_session_id = ?2",
                params![client.as_str(), client_session_id],
                |row| {
                    Ok(UsageTotals {
                        sessions: 1,
                        messages: 0,
                        input_tokens: row.get(0)?,
                        output_tokens: row.get(1)?,
                        reasoning_tokens: row.get(2)?,
                        cache_read_tokens: row.get(3)?,
                        cache_write_tokens: row.get(4)?,
                        total_tokens: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// 仅测试使用：直接访问连接以校验存储内容（不进入生产 API）。
    #[cfg(test)]
    pub(crate) fn connection_for_tests(&self) -> &Connection {
        &self.conn
    }
    /// 读取某个会话的未知项标注。
    pub fn session_unknown_relations(
        &self,
        client: Client,
        client_session_id: &str,
    ) -> Result<Vec<String>, UsageError> {
        let value: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT unknown_relations FROM sessions WHERE client = ?1 AND client_session_id = ?2",
                params![client.as_str(), client_session_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(decode_unknown_relations(value.flatten().as_deref()))
    }
}
/// Token 计数在库中以 i64 存储（SQLite 整数列），u64 不会出现在绑定参数里。
fn opt_i64(value: Option<u64>) -> Option<i64> {
    value.map(|value| value as i64)
}

/// 执行未应用的迁移（幂等）。
fn migrate(conn: &Connection) -> Result<u32, UsageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version       INTEGER PRIMARY KEY,
             name          TEXT    NOT NULL,
             applied_at_ms INTEGER NOT NULL
         );",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    for migration in MIGRATIONS.iter().filter(|m| i64::from(m.version) > current) {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, applied_at_ms) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, now_ms()],
        )?;
        transaction.commit()?;
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    // 便于外部工具直读；迁移表仍是事实来源。
    conn.execute_batch(&format!("PRAGMA user_version = {version};"))?;
    Ok(version as u32)
}

fn upsert_source_file_conn(conn: &Connection, file: &SourceFile) -> Result<i64, UsageError> {
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

fn upsert_session_conn(
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
fn upsert_message_conn(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::normalize::{
        ClaudeMessage, ClaudeSessionRollup, CodexThreadSnapshot, CodexUsageRecord,
        OpenCodeMessageRow, OpenCodeSessionRow, claude_code_message, claude_code_session,
        codex_message, codex_session, opencode_message, opencode_session,
    };
    use crate::usage::tokens::{RawTokenCounts, UNKNOWN_CACHE_WRITE_RELATION};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    const CODEX_THREAD: &str = "01a129d9-6720-7020-9402-a0a5930c37e6";
    const CLAUDE_SESSION: &str = "cfbc9166-dbf9-46d9-bbda-e2ef59b32a3c";

    fn temp_db_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "toklens-usage-{}-{}-{}.sqlite3",
            tag,
            std::process::id(),
            counter
        ))
    }

    fn remove_db(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn source_file(client: Client, path: &str) -> SourceFile {
        SourceFile {
            client,
            path: path.to_string(),
            kind: SourceFileKind::Jsonl,
            size_bytes: Some(4_200_000),
            mtime_ms: Some(1_791_704_287_676),
            content_offset: 12_345,
            parser_version: 1,
            last_scan_ms: Some(1_791_704_287_900),
            unparsed_records: 0,
        }
    }

    fn codex_cumulative() -> RawTokenCounts {
        RawTokenCounts {
            input: Some(4_941_884),
            output: Some(0),
            cache_read: Some(4_091_776),
            cache_write: Some(0),
            reasoning: Some(0),
            client_total: Some(4_941_884),
        }
    }

    fn codex_snapshot() -> CodexThreadSnapshot<'static> {
        CodexThreadSnapshot {
            thread_id: CODEX_THREAD,
            parent_thread_id: None,
            cwd: Some("D:\\WorkSpace\\TokLens"),
            model: Some("gpt-5.6-luna"),
            started_at: Some("2026-10-11T07:24:36.000Z"),
            ended_at: Some("2026-10-11T07:37:14.576Z"),
            first_ordinal: Some(0),
            message_count: 61,
            cumulative: codex_cumulative(),
        }
    }

    fn claude_raw() -> RawTokenCounts {
        RawTokenCounts {
            input: Some(1_608),
            output: Some(113),
            cache_read: Some(35_200),
            cache_write: Some(0),
            reasoning: None,
            client_total: None,
        }
    }

    fn claude_message(id: &str, is_sidechain: bool) -> MessageRecord {
        claude_code_message(&ClaudeMessage {
            session_id: CLAUDE_SESSION,
            message_id: id,
            timestamp: Some("2026-08-09T03:07:04.001Z"),
            model: Some("claude-sonnet-5"),
            cwd: Some("D:\\Repos\\example"),
            is_sidechain,
            usage: claude_raw(),
        })
    }

    fn claude_rollup(message_count: usize) -> SessionRecord {
        let raw = claude_raw();
        claude_code_session(&ClaudeSessionRollup {
            session_id: CLAUDE_SESSION,
            cwd: Some("D:\\Repos\\example"),
            model: Some("claude-sonnet-5"),
            started_at: Some("2026-08-09T03:07:04.000Z"),
            ended_at: Some("2026-08-09T03:10:00.000Z"),
            message_count,
            usage: RawTokenCounts::sum([&raw].into_iter().cycle().take(message_count)),
        })
    }

    #[test]
    fn migration_is_applied_once_and_versioned() {
        let store = UsageStore::open_in_memory().expect("open store");
        assert_eq!(store.schema_version().expect("version"), 1);
        let applied: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");
        assert_eq!(applied, MIGRATIONS.len() as i64);
        let version = migrate(&store.conn).expect("re-run migrations");
        assert_eq!(version, 1);
        let applied_again: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");
        assert_eq!(applied_again, applied);
    }

    #[test]
    fn schema_rejects_unknown_enum_values() {
        let store = UsageStore::open_in_memory().expect("open store");
        let result = store.conn.execute(
            "INSERT INTO sessions (id, client, client_session_id, semantics_id, total_basis,
                 message_coverage, source_kind, cost_source, imported_at_ms, updated_at_ms)
             VALUES ('x', 'codex', 'x', 's', 'nonsense', 'complete', 'measured', 'none', 0, 0)",
            [],
        );
        assert!(
            result.is_err(),
            "CHECK constraint should reject unknown enum"
        );
    }

    #[test]
    fn import_is_idempotent_and_watermarks_persist_across_restart() {
        let path = temp_db_path("restart");
        let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
        let session = codex_session(&codex_snapshot());
        let messages = [codex_message(&CodexUsageRecord {
            thread_id: CODEX_THREAD,
            response_id: Some("resp_1"),
            ordinal: 0,
            timestamp: Some("2026-10-11T07:24:36.001Z"),
            usage: RawTokenCounts {
                input: Some(90_065),
                output: Some(161),
                cache_read: Some(13_568),
                cache_write: Some(0),
                reasoning: Some(56),
                client_total: Some(90_226),
            },
        })];
        {
            let store = UsageStore::open(&path).expect("open store");
            let summary = store
                .import_session(&file, &session, &messages)
                .expect("import");
            assert_eq!(summary.session, Some(UpsertOutcome::Inserted));
            assert_eq!(summary.messages_inserted, 1);
            assert_eq!(store.session_count().expect("sessions"), 1);
            assert_eq!(store.message_count().expect("messages"), 1);
        }
        // 重启后：水位仍在，重复导入不新增行。
        {
            let store = UsageStore::open(&path).expect("reopen store");
            assert_eq!(store.schema_version().expect("version"), 1);
            let watermark = store
                .source_file(Client::Codex, &file.path)
                .expect("read watermark")
                .expect("watermark exists");
            assert_eq!(watermark.content_offset, 12_345);
            assert_eq!(watermark.parser_version, 1);
            let second = store
                .import_session(&file, &session, &messages)
                .expect("re-import");
            assert_eq!(second.session, Some(UpsertOutcome::Updated));
            assert_eq!(second.messages_inserted, 0);
            assert_eq!(second.messages_updated, 1);
            assert_eq!(store.session_count().expect("sessions"), 1);
            assert_eq!(store.message_count().expect("messages"), 1);
            let totals = store.totals(&TotalsFilter::default()).expect("totals");
            assert_eq!(totals.sessions, 1);
            assert_eq!(totals.messages, 1);
            assert_eq!(totals.total_tokens, 4_941_884);
        }
        remove_db(&path);
    }

    #[test]
    fn stale_cumulative_snapshot_never_overwrites_newer_total() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
        store
            .import_session(&file, &codex_session(&codex_snapshot()), &[])
            .expect("first import");

        let mut stale = codex_snapshot();
        stale.cumulative = RawTokenCounts {
            input: Some(1_000),
            client_total: Some(1_000),
            ..RawTokenCounts::default()
        };
        stale.ended_at = Some("2026-10-10T00:00:00.000Z");
        let summary = store
            .import_session(&file, &codex_session(&stale), &[])
            .expect("stale import");
        assert_eq!(summary.session, Some(UpsertOutcome::SkippedStale));
        let totals = store.totals(&TotalsFilter::default()).expect("totals");
        assert_eq!(totals.total_tokens, 4_941_884, "累计值不可回退");
    }

    #[test]
    fn claude_duplicate_message_ids_are_written_once() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(
            Client::ClaudeCode,
            "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
        );
        let first = claude_message("msg_01", false);
        let duplicate = claude_message("msg_01", false);
        let sidechain = claude_message("msg_02", true);
        let session = claude_rollup(2);
        let summary = store
            .import_session(&file, &session, &[first, duplicate, sidechain])
            .expect("import");
        assert_eq!(summary.messages_inserted, 2);
        assert_eq!(summary.messages_updated, 1);
        assert_eq!(store.message_count().expect("messages"), 2);

        let session_row = store
            .session_tokens(Client::ClaudeCode, CLAUDE_SESSION)
            .expect("session row")
            .expect("session exists");
        let messages_row = store
            .messages_sum(Client::ClaudeCode, CLAUDE_SESSION)
            .expect("messages sum");
        assert_eq!(messages_row.total_tokens, session_row.total_tokens);
        assert_eq!(session_row.total_tokens, 2 * (1_608 + 113 + 35_200));
        let sidechain_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE is_sidechain = 1",
                [],
                |row| row.get(0),
            )
            .expect("sidechain count");
        assert_eq!(sidechain_count, 1);
    }

    #[test]
    fn opencode_session_total_equals_deduped_message_sum() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(
            Client::OpenCode,
            "C:\\Users\\demo\\.local\\share\\opencode\\opencode.db",
        );
        let raw = RawTokenCounts {
            input: Some(288_249),
            output: Some(13_418),
            cache_read: Some(756_224),
            cache_write: Some(0),
            reasoning: Some(1_006),
            client_total: Some(1_058_897),
        };
        let row = OpenCodeSessionRow {
            id: "ses_ee3e1b96",
            parent_id: None,
            directory: Some("D:\\Repos\\example"),
            model: Some("deepseek-flash"),
            created_ms: Some(1_786_371_183_576),
            updated_ms: Some(1_791_473_749_139),
            cost_usd: Some(0.0123),
            usage: raw,
        };
        let message = opencode_message(&OpenCodeMessageRow {
            id: "msg_1",
            session_id: "ses_ee3e1b96",
            model_id: Some("deepseek-flash"),
            provider_id: Some("kuncode"),
            agent: Some("build"),
            created_ms: Some(1_786_371_183_576),
            completed_ms: Some(1_786_371_183_900),
            cost_usd: Some(0.0123),
            usage: raw,
        });
        store
            .import_session(&file, &opencode_session(&row), &[message])
            .expect("import");
        let session_row = store
            .session_tokens(Client::OpenCode, "ses_ee3e1b96")
            .expect("session row")
            .expect("session exists");
        let messages_row = store
            .messages_sum(Client::OpenCode, "ses_ee3e1b96")
            .expect("messages sum");
        assert_eq!(session_row.total_tokens, messages_row.total_tokens);
        assert_eq!(session_row.cache_read_tokens, 756_224);
        assert_eq!(session_row.reasoning_tokens, 1_006);
    }

    #[test]
    fn null_is_distinct_from_zero_and_missing_total_stays_unknown() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(
            Client::ClaudeCode,
            "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
        );
        let session = claude_code_session(&ClaudeSessionRollup {
            session_id: "s-missing",
            cwd: None,
            model: None,
            started_at: None,
            ended_at: None,
            message_count: 0,
            usage: RawTokenCounts {
                input: Some(10),
                output: None,
                cache_read: None,
                cache_write: None,
                reasoning: None,
                client_total: None,
            },
        });
        store
            .import_session(&file, &session, &[])
            .expect("import session");
        let (cache_read, total): (Option<i64>, Option<i64>) = store
            .conn
            .query_row(
                "SELECT cache_read_tokens, total_tokens FROM sessions WHERE client_session_id = 's-missing'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read row");
        assert_eq!(cache_read, None, "缺失字段必须写 NULL，不能写 0");
        assert_eq!(total, None, "缺失字段导致总量未知");
        assert_eq!(
            store
                .session_unknown_relations(Client::ClaudeCode, "s-missing")
                .expect("unknown relations"),
            vec!["total.unavailable_due_to_missing_fields".to_string()]
        );
    }

    #[test]
    fn unknown_relations_are_persisted_for_review() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
        store
            .import_session(&file, &codex_session(&codex_snapshot()), &[])
            .expect("import");
        let relations = store
            .session_unknown_relations(Client::Codex, CODEX_THREAD)
            .expect("relations");
        assert_eq!(relations, vec![UNKNOWN_CACHE_WRITE_RELATION.to_string()]);
    }

    #[test]
    fn deleting_a_session_cascades_to_its_messages() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(
            Client::ClaudeCode,
            "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
        );
        store
            .import_session(&file, &claude_rollup(1), &[claude_message("msg_01", false)])
            .expect("import");
        assert_eq!(store.message_count().expect("messages"), 1);
        store
            .conn
            .execute("DELETE FROM sessions WHERE client = 'claude_code'", [])
            .expect("delete session");
        assert_eq!(store.message_count().expect("messages"), 0);
    }

    #[test]
    fn totals_filter_by_client_and_time_range() {
        let store = UsageStore::open_in_memory().expect("open store");
        let codex_file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
        store
            .import_session(&codex_file, &codex_session(&codex_snapshot()), &[])
            .expect("import codex");
        let claude_file = source_file(
            Client::ClaudeCode,
            "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
        );
        store
            .import_session(&claude_file, &claude_rollup(2), &[])
            .expect("import claude");

        let all = store.totals(&TotalsFilter::default()).expect("totals");
        assert_eq!(all.sessions, 2);
        assert_eq!(all.total_tokens, 4_941_884 + 2 * (1_608 + 113 + 35_200));

        let codex_only = store
            .totals(&TotalsFilter {
                client: Some(Client::Codex),
                ..TotalsFilter::default()
            })
            .expect("totals");
        assert_eq!(codex_only.sessions, 1);
        assert_eq!(codex_only.total_tokens, 4_941_884);

        let codex_window = store
            .totals(&TotalsFilter {
                client: Some(Client::Codex),
                from_ms: Some(1_791_700_000_000),
                to_ms: Some(1_791_705_000_000),
                ..TotalsFilter::default()
            })
            .expect("totals");
        assert_eq!(codex_window.sessions, 1);

        let outside_window = store
            .totals(&TotalsFilter {
                from_ms: Some(1_800_000_000_000),
                ..TotalsFilter::default()
            })
            .expect("totals");
        assert_eq!(outside_window.sessions, 0);
        assert_eq!(outside_window.total_tokens, 0);
    }

    #[test]
    fn unknown_snapshot_does_not_erase_known_total() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
        store
            .import_session(&file, &codex_session(&codex_snapshot()), &[])
            .expect("first import");

        // 文件刚追加、还没写入任何 token 记录：不得把已知累计值抹成 NULL。
        let mut empty = codex_snapshot();
        empty.cumulative = RawTokenCounts::default();
        empty.message_count = 0;
        empty.ended_at = Some("2026-10-11T09:00:00.000Z");
        let outcome = store
            .upsert_session(&codex_session(&empty))
            .expect("empty snapshot");
        assert_eq!(outcome, UpsertOutcome::SkippedStale);
        let totals = store.totals(&TotalsFilter::default()).expect("totals");
        assert_eq!(totals.total_tokens, 4_941_884);
    }

    #[test]
    fn replace_file_import_prunes_messages_of_that_file() {
        let store = UsageStore::open_in_memory().expect("open store");
        let file = source_file(
            Client::ClaudeCode,
            "C:\\Users\\demo\\.claude\\projects\\p\\s.jsonl",
        );
        let session = claude_rollup(2);
        let first = store
            .replace_file_import(
                &file,
                &session,
                &[
                    claude_message("msg_01", false),
                    claude_message("msg_02", false),
                ],
            )
            .expect("first import");
        assert_eq!(first.messages_inserted, 2);
        assert_eq!(first.messages_pruned, 0);

        // 文件被截断后重新解析：只剩一条消息，旧明细必须被清理。
        let second = store
            .replace_file_import(&file, &session, &[claude_message("msg_01", false)])
            .expect("second import");
        assert_eq!(second.messages_pruned, 2);
        assert_eq!(second.messages_inserted, 1);
        assert_eq!(store.message_count().expect("messages"), 1);
    }
    #[test]
    fn import_rejects_mismatched_session_and_file() {
        let store = UsageStore::open_in_memory().expect("open store");
        let codex_file = source_file(Client::Codex, "C:\\Users\\demo\\.codex\\sessions\\a.jsonl");
        let claude = claude_rollup(1);
        let error = store
            .import_session(&codex_file, &claude, &[])
            .expect_err("mismatched client must fail");
        assert!(matches!(error, UsageError::InvalidInput(_)));
    }
}
