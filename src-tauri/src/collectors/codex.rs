//! Codex 采集器：扫描 `%USERPROFILE%\.codex\sessions\` 下的 `rollout-*.jsonl`。
//!
//! 解析与口径要点（依据见 `docs/data-source-research.md`）：
//!
//! - 兼容两套 Token 记录：新版 `token_usage_record`（`usage` 为每请求增量、
//!   `thread_token_usage` 为线程累计）与旧版 `event_msg.token_count`
//!   （`info.last_token_usage` 为每请求增量、`info.total_token_usage` 为线程累计）。
//!   同一文件同时存在两套记录时**只使用新版**，避免重复计数。
//! - 线程累计值按 `thread_id` 取最新一份，绝不相加多个文件的累计值；分页/续接文件
//!   通过 `first_ordinal != 0` 标记为明细不完整。
//! - 增量同步按 `(size_bytes, mtime_ms, parser_version)` 判断文件是否变化：未变化直接跳过，
//!   变化则整文件重新解析并以单事务覆盖写入（幂等，且能正确处理追加、截断与替换）。
//!
//! 已知限制：既不改变大小也不改变 mtime 的"同尺寸内容替换"无法被察觉（rollout 文件按追加写入，
//! 这种情况视为异常）；`SyncOptions::force_rescan` 提供显式全量重扫的兜底路径。

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::usage::model::{MessageRecord, SessionRecord, now_ms};
use crate::usage::normalize::{
    CodexThreadSnapshot, CodexUsageRecord, codex_message, codex_session,
};
use crate::usage::store::{SourceFile, SourceFileKind, SourceFileRow, UsageError, UsageStore};
use crate::usage::tokens::{Client, RawTokenCounts};

/// 解析器版本：白名单字段或归一化规则变化时 +1，水位中的旧版本会触发重新解析。
pub const PARSER_VERSION: i64 = 1;

/// 采集白名单：只提取这些字段，其余内容一律不读、不存、不记录。
pub const FIELD_WHITELIST: &[&str] = &[
    "type",
    "ordinal",
    "timestamp",
    "payload.id",
    "payload.session_id",
    "payload.cwd",
    "payload.model",
    "payload.info.last_token_usage",
    "payload.info.total_token_usage",
    "payload.response_id",
    "payload.thread_id",
    "payload.usage",
    "payload.thread_token_usage",
    "payload.type",
    "payload.history_base.thread_id",
    "payload.base_instructions.provenance.model",
];

/// 采集器错误。
#[derive(Debug)]
pub enum SyncError {
    /// 根目录无法枚举。
    Directory(std::io::Error),
    /// 本地库写入失败。
    Store(UsageError),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Directory(error) => write!(f, "cannot read collector root: {error}"),
            SyncError::Store(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<UsageError> for SyncError {
    fn from(error: UsageError) -> Self {
        SyncError::Store(error)
    }
}

/// 同步选项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncOptions {
    /// 忽略水位，全量重新解析所有文件（用于强制修复或用户主动重扫）。
    pub force_rescan: bool,
}

/// 单个文件的处理原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScanReason {
    New,
    Appended,
    TruncatedOrReplaced,
    ParserVersionChanged,
    Forced,
    Unchanged,
}

/// 一次同步的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// 根目录不存在（视为"无数据"而不是错误）。
    pub root_missing: bool,
    pub files_seen: usize,
    pub files_parsed: usize,
    pub files_skipped_unchanged: usize,
    /// 解析成功但没有会话标识（无法归属，未写库、未推进水位）。
    pub files_without_identity: usize,
    /// 读取失败的文件数（保留水位，下次重试）。
    pub files_failed: usize,
    /// 无法枚举的子目录数。
    pub directories_failed: usize,
    /// 解析失败的行数（已计入对应文件水位）。
    pub unparsed_lines: usize,
    pub sessions_imported: usize,
    pub sessions_skipped_stale: usize,
    pub messages_inserted: usize,
    pub messages_updated: usize,
    pub messages_skipped_stale: usize,
    pub messages_pruned: usize,
    /// 各处理原因的文件数，便于诊断增量行为。
    pub reasons: BTreeMap<ScanReason, usize>,
}

/// Codex 采集器。
pub struct CodexCollector {
    root: PathBuf,
}

impl CodexCollector {
    /// 默认根目录：`%USERPROFILE%\.codex\sessions`（取不到环境变量时返回 `None`）。
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("USERPROFILE")
            .map(|home| PathBuf::from(home).join(".codex").join("sessions"))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 扫描并增量导入。单个文件失败不会中断整次同步。
    pub fn sync(&self, store: &UsageStore, options: &SyncOptions) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.root.is_dir() {
            report.root_missing = true;
            return Ok(report);
        }
        let files = discover_rollout_files(&self.root, &mut report)?;
        report.files_seen = files.len();

        let mut parsed_files = Vec::new();
        for path in files {
            let (size_bytes, mtime_ms) = match std::fs::metadata(&path) {
                Ok(metadata) => (metadata.len() as i64, mtime_ms(&metadata)),
                Err(_) => {
                    report.files_failed += 1;
                    continue;
                }
            };
            let path_string = path.to_string_lossy().to_string();
            let watermark = store.source_file(Client::Codex, &path_string)?;
            let reason = decide_scan(
                watermark.as_ref(),
                size_bytes,
                mtime_ms,
                options.force_rescan,
            );
            *report.reasons.entry(reason).or_default() += 1;
            if reason == ScanReason::Unchanged {
                report.files_skipped_unchanged += 1;
                continue;
            }
            match parse_file(&path, size_bytes, mtime_ms) {
                Ok(parsed) => {
                    report.files_parsed += 1;
                    report.unparsed_lines += parsed.unparsed_lines;
                    parsed_files.push(parsed);
                }
                Err(_) => report.files_failed += 1,
            }
        }

        let mut threads: BTreeMap<String, ThreadAggregate> = BTreeMap::new();
        for parsed in parsed_files {
            match parsed.thread_id.clone() {
                Some(thread_id) => threads.entry(thread_id).or_default().push(parsed),
                None => report.files_without_identity += 1,
            }
        }

        for (thread_id, aggregate) in threads {
            let session = aggregate.session_record(&thread_id);
            let mut files = aggregate.files;
            files.sort_by(|left, right| {
                (left.mtime_ms, left.path.clone()).cmp(&(right.mtime_ms, right.path.clone()))
            });
            let mut session_counted = false;
            for file in files {
                let source = SourceFile {
                    client: Client::Codex,
                    path: file.path.to_string_lossy().to_string(),
                    kind: SourceFileKind::Jsonl,
                    size_bytes: Some(file.size_bytes),
                    mtime_ms: Some(file.mtime_ms),
                    content_offset: file.size_bytes,
                    parser_version: PARSER_VERSION,
                    last_scan_ms: Some(now_ms()),
                    unparsed_records: file.unparsed_lines as i64,
                };
                let summary = store.replace_file_import(&source, &session, &file.messages)?;
                if !session_counted {
                    session_counted = true;
                    match summary.session {
                        Some(crate::usage::store::UpsertOutcome::SkippedStale) => {
                            report.sessions_skipped_stale += 1;
                        }
                        Some(_) => report.sessions_imported += 1,
                        None => {}
                    }
                }
                report.messages_inserted += summary.messages_inserted;
                report.messages_updated += summary.messages_updated;
                report.messages_skipped_stale += summary.messages_skipped_stale;
                report.messages_pruned += summary.messages_pruned;
            }
        }

        Ok(report)
    }
}

/// 枚举 `rollout-*.jsonl`（递归；子目录读取失败只计数，不中断）。
fn discover_rollout_files(root: &Path, report: &mut SyncReport) -> Result<Vec<PathBuf>, SyncError> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                if directory == root {
                    return Err(SyncError::Directory(error));
                }
                report.directories_failed += 1;
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => directories.push(path),
                Ok(file_type) if file_type.is_file() => {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                        files.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    files.sort();
    Ok(files)
}

fn mtime_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis() as i64)
}

/// 增量决策：未变化跳过，其余情况整文件重新解析。
fn decide_scan(
    watermark: Option<&SourceFileRow>,
    size_bytes: i64,
    mtime_ms: i64,
    force_rescan: bool,
) -> ScanReason {
    if force_rescan {
        return ScanReason::Forced;
    }
    let Some(watermark) = watermark else {
        return ScanReason::New;
    };
    if watermark.parser_version != PARSER_VERSION {
        return ScanReason::ParserVersionChanged;
    }
    if size_bytes < watermark.content_offset || size_bytes < watermark.size_bytes.unwrap_or(0) {
        return ScanReason::TruncatedOrReplaced;
    }
    if watermark.size_bytes == Some(size_bytes) && watermark.mtime_ms == Some(mtime_ms) {
        return ScanReason::Unchanged;
    }
    ScanReason::Appended
}

/// 单文件解析结果（只含白名单字段）。
#[derive(Debug, Clone)]
struct ParsedFile {
    path: PathBuf,
    size_bytes: i64,
    mtime_ms: i64,
    thread_id: Option<String>,
    session_id: Option<String>,
    parent_thread_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    first_ordinal: Option<i64>,
    /// 新版记录里的线程累计（`thread_token_usage`），可能只覆盖新格式窗口。
    new_cumulative: Option<RawTokenCounts>,
    /// 旧版记录里的线程累计（`info.total_token_usage`），可能覆盖更早的历史。
    legacy_cumulative: Option<RawTokenCounts>,
    unparsed_lines: usize,
    messages: Vec<MessageRecord>,
}

/// 一条旧格式 Token 事件（`event_msg.token_count`）。
#[derive(Debug, Clone)]
struct LegacyEvent {
    ordinal: i64,
    timestamp: Option<String>,
    thread_id: String,
    /// `info.last_token_usage`：本次请求的增量。
    usage: RawTokenCounts,
}

impl ParsedFile {
    fn new(path: &Path, size_bytes: i64, mtime_ms: i64) -> Self {
        Self {
            path: path.to_path_buf(),
            size_bytes,
            mtime_ms,
            thread_id: None,
            session_id: None,
            parent_thread_id: None,
            cwd: None,
            model: None,
            started_at: None,
            ended_at: None,
            first_ordinal: None,
            new_cumulative: None,
            legacy_cumulative: None,
            unparsed_lines: 0,
            messages: Vec::new(),
        }
    }

    fn mark_time(&mut self, timestamp: Option<&str>) {
        let Some(timestamp) = timestamp else {
            return;
        };
        if self
            .started_at
            .as_deref()
            .is_none_or(|start| timestamp < start)
        {
            self.started_at = Some(timestamp.to_string());
        }
        if self.ended_at.as_deref().is_none_or(|end| timestamp > end) {
            self.ended_at = Some(timestamp.to_string());
        }
    }

    fn mark_ordinal(&mut self, ordinal: i64) {
        self.first_ordinal = Some(
            self.first_ordinal
                .map_or(ordinal, |first| first.min(ordinal)),
        );
    }
}

/// 汇总明细：新版记录全部保留；旧格式事件只保留"新格式尚未开始"的前缀，
/// 与新格式逐条配对的镜像事件（值相同、按顺序一一对应）必须丢弃，避免同一请求被统计两次。
fn resolve_messages(
    parsed: &mut ParsedFile,
    new_records: &[MessageRecord],
    legacy_events: &[LegacyEvent],
) {
    let mut mirror_index = 0usize;
    for event in legacy_events {
        let is_mirror = mirror_index < new_records.len()
            && event.usage.client_total.is_some()
            && new_records[mirror_index].usage.total_tokens == event.usage.client_total;
        if is_mirror {
            mirror_index += 1;
            continue;
        }
        parsed.messages.push(codex_message(&CodexUsageRecord {
            thread_id: &event.thread_id,
            // 旧格式没有 response_id：去重键回退为 `token_count:<ordinal>`。
            response_id: None,
            ordinal: event.ordinal,
            timestamp: event.timestamp.as_deref(),
            usage: event.usage,
        }));
    }
    parsed.messages.extend(new_records.iter().cloned());
}

/// 按行解析一个 rollout 文件。只访问白名单字段，正文不会被取出。
fn parse_file(path: &Path, size_bytes: i64, mtime_ms: i64) -> std::io::Result<ParsedFile> {
    let reader = BufReader::new(File::open(path)?);
    let mut parsed = ParsedFile::new(path, size_bytes, mtime_ms);
    let mut new_records: Vec<MessageRecord> = Vec::new();
    let mut legacy_events: Vec<LegacyEvent> = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                // 读取中断（例如文件正在被截断）：保留已解析部分，交由水位机制下次重试。
                parsed.unparsed_lines += 1;
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                parsed.unparsed_lines += 1;
                continue;
            }
        };
        // 明细完整度只看文件自身的 ordinal 起点（调研口径：起点不为 0 说明是分页/续接文件）。
        if let Some(ordinal) = value.get("ordinal").and_then(Value::as_i64) {
            parsed.mark_ordinal(ordinal);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => parse_session_meta(&mut parsed, &value),
            Some("turn_context") => parse_turn_context(&mut parsed, &value),
            Some("token_usage_record") => {
                parse_token_usage_record(&mut parsed, &value, &mut new_records)
            }
            Some("event_msg") => parse_legacy_token_count(&mut parsed, &value, &mut legacy_events),
            // 其余行类型（正文、工具输出、状态等）一律忽略，不读取任何字段。
            _ => {}
        }
    }

    resolve_messages(&mut parsed, &new_records, &legacy_events);
    Ok(parsed)
}

fn parse_session_meta(parsed: &mut ParsedFile, value: &Value) {
    let payload = value.get("payload");
    if parsed.thread_id.is_none() {
        parsed.thread_id = payload
            .and_then(|payload| payload.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.session_id.is_none() {
        parsed.session_id = payload
            .and_then(|payload| payload.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.parent_thread_id.is_none() {
        parsed.parent_thread_id = payload
            .and_then(|payload| payload.get("history_base"))
            .and_then(|history| history.get("thread_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.cwd.is_none() {
        parsed.cwd = payload
            .and_then(|payload| payload.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if parsed.model.is_none() {
        parsed.model = payload
            .and_then(|payload| payload.get("base_instructions"))
            .and_then(|instructions| instructions.get("provenance"))
            .and_then(|provenance| provenance.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    parsed.mark_time(line_timestamp(value));
}

fn parse_turn_context(parsed: &mut ParsedFile, value: &Value) {
    let payload = value.get("payload");
    // 实际运行的模型以 `turn_context` 为准（会话元数据里的可能只是默认值）。
    if let Some(model) = payload
        .and_then(|payload| payload.get("model"))
        .and_then(Value::as_str)
    {
        parsed.model = Some(model.to_string());
    }
    if parsed.cwd.is_none() {
        parsed.cwd = payload
            .and_then(|payload| payload.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    parsed.mark_time(line_timestamp(value));
}

fn parse_token_usage_record(
    parsed: &mut ParsedFile,
    value: &Value,
    records: &mut Vec<MessageRecord>,
) {
    let payload = value.get("payload");
    let timestamp = line_timestamp(value);
    let Some(ordinal) = value.get("ordinal").and_then(Value::as_i64) else {
        parsed.unparsed_lines += 1;
        return;
    };
    let thread_id = payload
        .and_then(|payload| payload.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| parsed.thread_id.clone());
    let Some(thread_id) = thread_id else {
        parsed.unparsed_lines += 1;
        return;
    };
    if parsed.session_id.is_none() {
        parsed.session_id = payload
            .and_then(|payload| payload.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    parsed.mark_time(timestamp);
    records.push(codex_message(&CodexUsageRecord {
        thread_id: &thread_id,
        response_id: payload
            .and_then(|payload| payload.get("response_id"))
            .and_then(Value::as_str),
        ordinal,
        timestamp,
        usage: raw_counts(payload.and_then(|payload| payload.get("usage"))),
    }));
    // 文件按追加顺序写入：最后一条即为文件末尾的线程累计。
    if let Some(cumulative) = payload
        .and_then(|payload| payload.get("thread_token_usage"))
        .map(|usage| raw_counts(Some(usage)))
    {
        parsed.new_cumulative = Some(cumulative);
    }
}

fn parse_legacy_token_count(parsed: &mut ParsedFile, value: &Value, events: &mut Vec<LegacyEvent>) {
    let payload = value.get("payload");
    if payload
        .and_then(|payload| payload.get("type"))
        .and_then(Value::as_str)
        != Some("token_count")
    {
        return;
    }
    let Some(info) = payload.and_then(|payload| payload.get("info")) else {
        return;
    };
    let Some(last_usage) = info.get("last_token_usage") else {
        return;
    };
    let timestamp = line_timestamp(value);
    let Some(ordinal) = value.get("ordinal").and_then(Value::as_i64) else {
        parsed.unparsed_lines += 1;
        return;
    };
    let thread_id = payload
        .and_then(|payload| payload.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| parsed.thread_id.clone());
    let Some(thread_id) = thread_id else {
        parsed.unparsed_lines += 1;
        return;
    };
    parsed.mark_time(timestamp);
    events.push(LegacyEvent {
        ordinal,
        timestamp: timestamp.map(str::to_string),
        thread_id,
        usage: raw_counts(Some(last_usage)),
    });
    if let Some(total_usage) = info.get("total_token_usage") {
        parsed.legacy_cumulative = Some(raw_counts(Some(total_usage)));
    }
}

fn line_timestamp(value: &Value) -> Option<&str> {
    value.get("timestamp").and_then(Value::as_str).or_else(|| {
        value
            .get("payload")
            .and_then(|payload| payload.get("timestamp"))
            .and_then(Value::as_str)
    })
}

/// 从客户端字段映射到统一计数（Codex 字段名 → `RawTokenCounts`）。
fn raw_counts(usage: Option<&Value>) -> RawTokenCounts {
    let Some(usage) = usage else {
        return RawTokenCounts::default();
    };
    RawTokenCounts {
        input: usage.get("input_tokens").and_then(Value::as_u64),
        output: usage.get("output_tokens").and_then(Value::as_u64),
        cache_read: usage.get("cached_input_tokens").and_then(Value::as_u64),
        cache_write: usage
            .get("cache_write_input_tokens")
            .and_then(Value::as_u64),
        reasoning: usage.get("reasoning_output_tokens").and_then(Value::as_u64),
        client_total: usage.get("total_tokens").and_then(Value::as_u64),
    }
}

/// 同一线程的多个 rollout 文件聚合结果。
#[derive(Debug, Default)]
struct ThreadAggregate {
    session_id: Option<String>,
    parent_thread_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    first_ordinal: Option<i64>,
    message_count: usize,
    /// 线程累计值：在新旧两个计数器（以及同线程的多个文件）中取最大值。
    cumulative: Option<RawTokenCounts>,
    cumulative_key: Option<(u64, u64, u64)>,
    files: Vec<ParsedFile>,
}

/// 累计值排序键：先比总量，再比输入与缓存读取（仅用于在多个候选中确定性地取最大值）。
fn cumulative_key(counts: &RawTokenCounts) -> (u64, u64, u64) {
    (
        counts.client_total.unwrap_or(0),
        counts.input.unwrap_or(0),
        counts.cache_read.unwrap_or(0),
    )
}

impl ThreadAggregate {
    fn push(&mut self, file: ParsedFile) {
        if self.session_id.is_none() {
            self.session_id = file.session_id.clone();
        }
        if self.parent_thread_id.is_none() {
            self.parent_thread_id = file.parent_thread_id.clone();
        }
        if self.cwd.is_none() {
            self.cwd = file.cwd.clone();
        }
        if self.model.is_none() {
            self.model = file.model.clone();
        }
        if let Some(start) = file.started_at.as_deref()
            && self
                .started_at
                .as_deref()
                .is_none_or(|current| start < current)
        {
            self.started_at = Some(start.to_string());
        }
        if let Some(end) = file.ended_at.as_deref()
            && self.ended_at.as_deref().is_none_or(|current| end > current)
        {
            self.ended_at = Some(end.to_string());
        }
        // 只要有一个文件的明细是从头开始的，整个线程的请求明细就是完整的。
        if let Some(first_ordinal) = file.first_ordinal {
            self.first_ordinal = Some(
                self.first_ordinal
                    .map_or(first_ordinal, |current| current.min(first_ordinal)),
            );
        }
        self.message_count += file.messages.len();
        // 新旧两个累计计数器的起点可能不同（旧格式链可能覆盖更早的历史），
        // 只能取最大值；绝不能把两个计数器或多份文件相加。
        for candidate in [file.new_cumulative, file.legacy_cumulative]
            .into_iter()
            .flatten()
        {
            self.observe_cumulative(candidate);
        }
        self.files.push(file);
    }

    fn observe_cumulative(&mut self, candidate: RawTokenCounts) {
        let key = cumulative_key(&candidate);
        if self.cumulative.is_none() || Some(key) > self.cumulative_key {
            self.cumulative = Some(candidate);
            self.cumulative_key = Some(key);
        }
    }

    /// 由线程聚合结果生成会话快照（累计值 + 明细完整度）。
    fn session_record(&self, thread_id: &str) -> SessionRecord {
        codex_session(&CodexThreadSnapshot {
            thread_id,
            parent_thread_id: self.parent_thread_id.as_deref(),
            cwd: self.cwd.as_deref(),
            model: self.model.as_deref(),
            started_at: self.started_at.as_deref(),
            ended_at: self.ended_at.as_deref(),
            first_ordinal: self.first_ordinal,
            message_count: self.message_count,
            cumulative: self.cumulative.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::store::TotalsFilter;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    const THREAD: &str = "01a129d9-6720-7020-9402-a0a5930c37e6";
    const PARENT: &str = "01a129d8-f6be-72b1-a243-c141d54e2791";
    /// 用于验证"正文绝不入库"的哨兵字符串。
    const SENTINEL: &str = "SENTINEL-PROMPT-TEXT-DO-NOT-STORE";

    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "toklens-codex-{}-{}-{}",
            tag,
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn lines_to_text(lines: &[Value]) -> String {
        let mut text = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        text.push('\n');
        text
    }

    fn write_file(dir: &Path, name: &str, lines: &[Value]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, lines_to_text(lines)).expect("write synthetic jsonl");
        path
    }

    fn append_lines(path: &Path, lines: &[Value]) {
        let mut text = std::fs::read_to_string(path).expect("read synthetic jsonl");
        text.push_str(&lines_to_text(lines));
        std::fs::write(path, text).expect("append synthetic jsonl");
    }

    /// Codex 的 `usage` / `thread_token_usage` 结构：`total = input + output`。
    fn counts(input: u64, output: u64, cached: u64, cache_write: u64, reasoning: u64) -> Value {
        json!({
            "input_tokens": input,
            "output_tokens": output,
            "cached_input_tokens": cached,
            "cache_write_input_tokens": cache_write,
            "reasoning_output_tokens": reasoning,
            "total_tokens": input + output,
        })
    }

    fn session_meta_at(thread: &str, at: &str, parent: Option<&str>, ordinal: i64) -> Value {
        let mut payload = json!({
            "id": thread,
            "session_id": thread,
            "timestamp": at,
            "cwd": "D:\\WorkSpace\\TokLens",
            "cli_version": "0.162.0-alpha.17.2",
            "base_instructions": { "provenance": { "model": "gpt-5.6-luna" } },
        });
        if let Some(parent) = parent {
            payload["history_base"] = json!({ "thread_id": parent, "end_ordinal_exclusive": 807 });
        }
        json!({ "timestamp": at, "ordinal": ordinal, "type": "session_meta", "payload": payload })
    }

    fn usage_record(
        thread: &str,
        at: &str,
        ordinal: i64,
        response_id: &str,
        usage: Value,
        thread_usage: Value,
    ) -> Value {
        json!({
            "timestamp": at,
            "ordinal": ordinal,
            "type": "token_usage_record",
            "payload": {
                "thread_id": thread,
                "session_id": thread,
                "turn_id": "turn-1",
                "response_id": response_id,
                "usage": usage,
                "turn_token_usage": usage.clone(),
                "thread_token_usage": thread_usage,
            },
        })
    }

    fn legacy_token_count(
        thread: Option<&str>,
        at: &str,
        ordinal: i64,
        last: Value,
        total: Value,
    ) -> Value {
        let mut payload = json!({
            "type": "token_count",
            "info": { "last_token_usage": last, "total_token_usage": total, "model_context_window": 200000 },
        });
        if let Some(thread) = thread {
            payload["thread_id"] = json!(thread);
        }
        json!({ "timestamp": at, "ordinal": ordinal, "type": "event_msg", "payload": payload })
    }

    fn sync(store: &UsageStore, root: &Path) -> SyncReport {
        CodexCollector::new(root)
            .sync(store, &SyncOptions::default())
            .expect("sync")
    }

    fn reason_count(report: &SyncReport, reason: ScanReason) -> usize {
        report.reasons.get(&reason).copied().unwrap_or(0)
    }

    fn codex_totals(store: &UsageStore) -> crate::usage::store::UsageTotals {
        store
            .totals(&TotalsFilter {
                client: Some(Client::Codex),
                ..TotalsFilter::default()
            })
            .expect("totals")
    }

    fn coverage(store: &UsageStore, client_session_id: &str) -> String {
        store
            .connection_for_tests()
            .query_row(
                "SELECT message_coverage FROM sessions WHERE client = 'codex' AND client_session_id = ?1",
                rusqlite::params![client_session_id],
                |row| row.get::<_, String>(0),
            )
            .expect("coverage")
    }

    #[test]
    fn imports_new_token_format_with_codex_semantics() {
        let dir = temp_dir("new-format");
        write_file(
            &dir,
            "rollout-2026-10-11T07-24-36-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T07:24:36.001Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T07:25:00.000Z",
                    1,
                    "resp_1",
                    counts(90_065, 161, 13_568, 0, 56),
                    counts(90_065, 161, 13_568, 0, 56),
                ),
                usage_record(
                    THREAD,
                    "2026-10-11T07:26:00.000Z",
                    2,
                    "resp_2",
                    counts(4_851_819, 0, 4_078_208, 0, 40),
                    counts(4_941_884, 161, 4_091_776, 0, 96),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.files_seen, 1);
        assert_eq!(report.files_parsed, 1);
        assert_eq!(report.sessions_imported, 1);
        assert_eq!(report.messages_inserted, 2);
        assert_eq!(report.unparsed_lines, 0);

        let totals = codex_totals(&store);
        assert_eq!(totals.sessions, 1);
        assert_eq!(totals.messages, 2);
        // 会话总量取线程累计值，而不是两条请求之和。
        assert_eq!(totals.total_tokens, 4_941_884 + 161);
        // 子集字段已扣除：input 不含 cached，output 不含 reasoning。
        let session = store
            .session_tokens(Client::Codex, THREAD)
            .expect("session")
            .expect("exists");
        assert_eq!(session.input_tokens, 4_941_884 - 4_091_776);
        assert_eq!(session.output_tokens, 161 - 96);
        assert_eq!(session.reasoning_tokens, 96);
        assert_eq!(session.cache_read_tokens, 4_091_776);
        assert_eq!(
            store
                .session_unknown_relations(Client::Codex, THREAD)
                .expect("unknown"),
            vec!["cache_write.relation_to_input".to_string()]
        );
        assert_eq!(coverage(&store, THREAD), "complete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn imports_legacy_token_count_only_files() {
        let dir = temp_dir("legacy");
        write_file(
            &dir,
            "rollout-2026-07-09T23-37-38-019f4786.jsonl",
            &[
                session_meta_at(THREAD, "2026-07-09T23:37:38.000Z", None, 0),
                legacy_token_count(
                    None,
                    "2026-07-09T23:38:00.000Z",
                    1,
                    counts(13_000, 100, 0, 0, 10),
                    counts(13_000, 100, 0, 0, 10),
                ),
                legacy_token_count(
                    None,
                    "2026-07-09T23:39:00.000Z",
                    2,
                    counts(528_000, 168, 512_000, 0, 68),
                    counts(541_000, 268, 512_000, 0, 78),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.messages_inserted, 2);
        let totals = codex_totals(&store);
        // 旧格式：last_token_usage 是每请求增量，total_token_usage 是线程累计。
        assert_eq!(totals.total_tokens, 541_000 + 268);
        let session = store
            .session_tokens(Client::Codex, THREAD)
            .expect("session")
            .expect("exists");
        assert_eq!(session.input_tokens, 541_000 - 512_000);
        assert_eq!(session.reasoning_tokens, 78);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mixed_format_keeps_legacy_prefix_and_takes_max_cumulative() {
        // 真实场景（rollout-2026-10-06T00-38-35-…）：文件前半段只有旧格式历史，
        // 后半段是新格式记录 + 每条旧格式镜像。镜像必须丢弃，但旧格式前缀要保留。
        let dir = temp_dir("mixed-prefix");
        write_file(
            &dir,
            "rollout-2026-10-06T00-38-35-01a10cee.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-06T00:38:35.000Z", None, 11),
                legacy_token_count(
                    None,
                    "2026-10-06T00:39:00.000Z",
                    12,
                    counts(12_000, 180, 0, 0, 0),
                    counts(12_000, 180, 0, 0, 0),
                ),
                legacy_token_count(
                    None,
                    "2026-10-06T00:40:00.000Z",
                    13,
                    counts(12_600, 206, 0, 0, 0),
                    counts(24_600, 386, 0, 0, 0),
                ),
                usage_record(
                    THREAD,
                    "2026-10-06T00:41:00.000Z",
                    20,
                    "resp_1",
                    counts(44_000, 698, 0, 0, 0),
                    counts(44_000, 698, 0, 0, 0),
                ),
                // 新格式记录的镜像：值相同，必须丢弃。
                legacy_token_count(
                    None,
                    "2026-10-06T00:41:00.000Z",
                    25,
                    counts(44_000, 698, 0, 0, 0),
                    counts(32_000_000, 253_468, 0, 0, 0),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        // 明细 = 2 条旧格式前缀 + 1 条新格式记录（镜像已丢弃）。
        assert_eq!(report.messages_inserted, 3);
        assert_eq!(store.message_count().expect("messages"), 3);
        // 线程累计 = 两个计数器的最大值（旧格式链覆盖更早的历史），绝不是相加。
        assert_eq!(codex_totals(&store).total_tokens, 32_000_000 + 253_468);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mixed_format_files_use_new_records_only() {
        let dir = temp_dir("mixed");
        let usage = counts(1_000, 100, 400, 0, 20);
        write_file(
            &dir,
            "rollout-2026-10-11T08-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T08:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T08:01:00.000Z",
                    1,
                    "resp_1",
                    usage.clone(),
                    usage.clone(),
                ),
                // 同一条请求的旧格式镜像记录：必须被忽略，不能重复计数。
                legacy_token_count(None, "2026-10-11T08:01:00.000Z", 1, usage.clone(), usage),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.messages_inserted, 1, "旧格式镜像记录必须被忽略");
        assert_eq!(codex_totals(&store).total_tokens, 1_100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn thread_cumulative_is_not_summed_across_continuation_files() {
        let dir = temp_dir("continuation");
        write_file(
            &dir,
            "rollout-2026-10-11T07-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T07:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T07:01:00.000Z",
                    1,
                    "resp_1",
                    counts(1_000, 100, 400, 0, 20),
                    counts(1_000, 100, 400, 0, 20),
                ),
            ],
        );
        write_file(
            &dir,
            "rollout-2026-10-11T07-30-00-01a129d9_01a129d8.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T07:30:00.000Z", Some(PARENT), 807),
                usage_record(
                    THREAD,
                    "2026-10-11T07:31:00.000Z",
                    807,
                    "resp_2",
                    counts(900, 50, 0, 0, 10),
                    counts(1_900, 150, 400, 0, 30),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.files_seen, 2);
        assert_eq!(report.messages_inserted, 2);
        let totals = codex_totals(&store);
        assert_eq!(totals.sessions, 1, "同一 thread 只应有一个会话");
        // 取最新累计值 1_950，而不是把两份累计值相加（2_950）。
        assert_eq!(totals.total_tokens, 1_900 + 150);
        let parent: Option<String> = store
            .connection_for_tests()
            .query_row(
                "SELECT parent_client_session_id FROM sessions WHERE client_session_id = ?1",
                rusqlite::params![THREAD],
                |row| row.get(0),
            )
            .expect("parent");
        assert_eq!(parent.as_deref(), Some(PARENT));
        // 该线程有一个文件从 ordinal 0 开始（另一个是它的续接），合起来覆盖完整历史。
        assert_eq!(coverage(&store, THREAD), "complete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_without_token_records_still_imports_a_session() {
        let dir = temp_dir("no-usage");
        write_file(
            &dir,
            "rollout-2026-10-11T09-00-00-01a1aaaa.jsonl",
            &[session_meta_at(
                "01a1aaaa-0000-0000-0000-000000000000",
                "2026-10-11T09:00:00.000Z",
                None,
                0,
            )],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.sessions_imported, 1);
        assert_eq!(report.messages_inserted, 0);
        let totals = codex_totals(&store);
        assert_eq!(totals.sessions, 1);
        assert_eq!(totals.total_tokens, 0, "没有用量时总量为 NULL，聚合为 0");
        assert_eq!(
            coverage(&store, "01a1aaaa-0000-0000-0000-000000000000"),
            "none"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paginated_only_thread_is_marked_partial() {
        let dir = temp_dir("paginated");
        write_file(
            &dir,
            "rollout-2026-09-12T01-28-46-01a09172.jsonl",
            &[
                // 分页文件：历史被裁掉，ordinal 从 807 开始。
                session_meta_at(THREAD, "2026-09-12T01:28:46.000Z", None, 807),
                usage_record(
                    THREAD,
                    "2026-09-12T01:29:00.000Z",
                    808,
                    "resp_1",
                    counts(100, 10, 0, 0, 0),
                    counts(1_907_000, 268, 0, 0, 0),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.messages_inserted, 1);
        assert_eq!(coverage(&store, THREAD), "partial");
        // 明细不完整，但会话总量仍取客户端累计值。
        assert_eq!(codex_totals(&store).total_tokens, 1_907_000 + 268);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rescan_skips_unchanged_files_and_is_idempotent() {
        let dir = temp_dir("rescan");
        write_file(
            &dir,
            "rollout-2026-10-11T10-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T10:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T10:01:00.000Z",
                    1,
                    "resp_1",
                    counts(1_000, 100, 400, 0, 20),
                    counts(1_000, 100, 400, 0, 20),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let first = sync(&store, &dir);
        assert_eq!(first.files_parsed, 1);
        assert_eq!(first.messages_inserted, 1);

        let second = sync(&store, &dir);
        assert_eq!(second.files_parsed, 0, "未变化的文件不应重新解析");
        assert_eq!(second.files_skipped_unchanged, 1);
        assert_eq!(reason_count(&second, ScanReason::Unchanged), 1);
        assert_eq!(store.message_count().expect("messages"), 1);
        assert_eq!(codex_totals(&store).total_tokens, 1_100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn appended_file_imports_new_messages_without_duplicating() {
        let dir = temp_dir("append");
        let path = write_file(
            &dir,
            "rollout-2026-10-11T11-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T11:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T11:01:00.000Z",
                    1,
                    "resp_1",
                    counts(1_000, 100, 400, 0, 20),
                    counts(1_000, 100, 400, 0, 20),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        sync(&store, &dir);

        append_lines(
            &path,
            &[usage_record(
                THREAD,
                "2026-10-11T11:05:00.000Z",
                2,
                "resp_2",
                counts(900, 50, 0, 0, 10),
                counts(1_900, 150, 400, 0, 30),
            )],
        );
        let report = sync(&store, &dir);

        assert_eq!(report.files_parsed, 1);
        assert_eq!(reason_count(&report, ScanReason::Appended), 1);
        assert_eq!(store.message_count().expect("messages"), 2);
        assert_eq!(report.messages_pruned, 1, "覆盖式导入先清理该文件旧明细");
        assert_eq!(codex_totals(&store).total_tokens, 1_900 + 150);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncated_file_is_reparsed_and_detail_is_rebuilt() {
        let dir = temp_dir("truncate");
        let path = write_file(
            &dir,
            "rollout-2026-10-11T12-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T12:01:00.000Z",
                    1,
                    "resp_1",
                    counts(1_000, 100, 0, 0, 0),
                    counts(1_000, 100, 0, 0, 0),
                ),
                usage_record(
                    THREAD,
                    "2026-10-11T12:02:00.000Z",
                    2,
                    "resp_2",
                    counts(2_000, 0, 0, 0, 0),
                    counts(3_000, 100, 0, 0, 0),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        sync(&store, &dir);
        assert_eq!(store.message_count().expect("messages"), 2);

        // 文件被截断/替换：内容变短、ordinal 起点改变。
        std::fs::write(
            &path,
            lines_to_text(&[
                session_meta_at(THREAD, "2026-10-11T12:00:00.000Z", None, 807),
                usage_record(
                    THREAD,
                    "2026-10-11T12:01:00.000Z",
                    808,
                    "resp_1",
                    counts(1_000, 100, 0, 0, 0),
                    counts(1_000, 100, 0, 0, 0),
                ),
            ]),
        )
        .expect("truncate synthetic file");
        let report = sync(&store, &dir);

        assert_eq!(reason_count(&report, ScanReason::TruncatedOrReplaced), 1);
        // 明细按新内容重建（旧的两条已不存在）。
        assert_eq!(report.messages_pruned, 2);
        assert_eq!(store.message_count().expect("messages"), 1);
        // 累计值不回退：曾观测到的 3_100 仍是该线程的已知累计值，且陈旧快照整体被忽略
        // （会话行的口径、完整度、时间与总量一并保持上一次的值，不做部分更新）。
        assert_eq!(codex_totals(&store).total_tokens, 3_100);
        assert_eq!(coverage(&store, THREAD), "complete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_response_ids_across_files_land_once() {
        let dir = temp_dir("duplicate-response");
        let usage = counts(1_000, 100, 400, 0, 20);
        write_file(
            &dir,
            "rollout-2026-10-11T13-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T13:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T13:01:00.000Z",
                    1,
                    "resp_X",
                    usage.clone(),
                    usage.clone(),
                ),
            ],
        );
        write_file(
            &dir,
            "rollout-2026-10-11T13-10-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T13:10:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T13:01:00.000Z",
                    1,
                    "resp_X",
                    usage.clone(),
                    usage,
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.files_seen, 2);
        assert_eq!(store.message_count().expect("messages"), 1);
        assert_eq!(codex_totals(&store).total_tokens, 1_100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn force_rescan_reparses_unchanged_files() {
        let dir = temp_dir("force");
        write_file(
            &dir,
            "rollout-2026-10-11T14-00-00-01a129d9.jsonl",
            &[
                session_meta_at(THREAD, "2026-10-11T14:00:00.000Z", None, 0),
                usage_record(
                    THREAD,
                    "2026-10-11T14:01:00.000Z",
                    1,
                    "resp_1",
                    counts(1_000, 100, 0, 0, 0),
                    counts(1_000, 100, 0, 0, 0),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        sync(&store, &dir);

        let report = CodexCollector::new(&dir)
            .sync(&store, &SyncOptions { force_rescan: true })
            .expect("forced sync");
        assert_eq!(report.files_parsed, 1);
        assert_eq!(reason_count(&report, ScanReason::Forced), 1);
        assert_eq!(store.message_count().expect("messages"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_root_is_not_an_error() {
        let dir = temp_dir("missing-root");
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir.join("does-not-exist"));

        assert!(report.root_missing);
        assert_eq!(report.files_seen, 0);
        assert_eq!(store.session_count().expect("sessions"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_lines_are_counted_and_do_not_abort_the_file() {
        let dir = temp_dir("invalid");
        let path = dir.join("rollout-2026-10-11T15-00-00-01a129d9.jsonl");
        let mut text = lines_to_text(&[
            session_meta_at(THREAD, "2026-10-11T15:00:00.000Z", None, 0),
            usage_record(
                THREAD,
                "2026-10-11T15:01:00.000Z",
                1,
                "resp_1",
                counts(1_000, 100, 0, 0, 0),
                counts(1_000, 100, 0, 0, 0),
            ),
        ]);
        text.push_str("not-json-at-all\n{\"type\": \"token_usage_record\", \"payload\": {\n");
        std::fs::write(&path, text).expect("write file with invalid lines");

        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.unparsed_lines, 2);
        assert_eq!(report.messages_inserted, 1, "合法记录仍应导入");
        let watermark = store
            .source_file(Client::Codex, &path.to_string_lossy())
            .expect("watermark")
            .expect("exists");
        assert_eq!(watermark.unparsed_records, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_without_identity_is_not_imported_and_keeps_no_watermark() {
        let dir = temp_dir("no-identity");
        // 既没有 session_meta，记录里也没有 thread_id：无法归属，不写库、不推进水位。
        let path = write_file(
            &dir,
            "rollout-2026-10-11T16-00-00-unknown.jsonl",
            &[legacy_token_count(
                None,
                "2026-10-11T16:00:00.000Z",
                1,
                counts(100, 10, 0, 0, 0),
                counts(100, 10, 0, 0, 0),
            )],
        );
        let store = UsageStore::open_in_memory().expect("store");
        let report = sync(&store, &dir);

        assert_eq!(report.files_without_identity, 1);
        assert_eq!(report.sessions_imported, 0);
        assert_eq!(store.session_count().expect("sessions"), 0);
        assert!(
            store
                .source_file(Client::Codex, &path.to_string_lossy())
                .expect("watermark")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn privacy_sentinel_content_never_reaches_the_database() {
        let dir = temp_dir("privacy");
        let meta = session_meta_at(THREAD, "2026-10-11T17:00:00.000Z", None, 0);
        let mut meta = meta;
        meta["payload"]["base_instructions"]["text"] = json!(SENTINEL);
        write_file(
            &dir,
            "rollout-2026-10-11T17-00-00-01a129d9.jsonl",
            &[
                meta,
                // 正文/提示词/工具输出行：解析器不读取这些字段。
                json!({ "timestamp": "2026-10-11T17:00:30.000Z", "ordinal": 1, "type": "response_item",
                        "payload": { "type": "message", "role": "user",
                                     "content": [{ "type": "input_text", "text": SENTINEL }] } }),
                json!({ "timestamp": "2026-10-11T17:01:00.000Z", "ordinal": 2, "type": "event_msg",
                        "payload": { "type": "item_completed",
                                     "item": { "type": "CommandExecution", "aggregated_output": SENTINEL } } }),
                usage_record(
                    THREAD,
                    "2026-10-11T17:02:00.000Z",
                    3,
                    "resp_1",
                    counts(1_000, 100, 400, 0, 20),
                    counts(1_000, 100, 400, 0, 20),
                ),
            ],
        );
        let store = UsageStore::open_in_memory().expect("store");
        sync(&store, &dir);

        let connection = store.connection_for_tests();
        let text_columns = [
            ("sessions", "id"),
            ("sessions", "client_session_id"),
            ("sessions", "parent_client_session_id"),
            ("sessions", "project_path"),
            ("sessions", "model"),
            ("sessions", "semantics_id"),
            ("sessions", "unknown_relations"),
            ("messages", "id"),
            ("messages", "dedup_key"),
            ("messages", "model"),
            ("messages", "agent"),
            ("messages", "source_ref"),
            ("source_files", "path"),
        ];
        let mut leaked = Vec::new();
        for (table, column) in text_columns {
            let sql = format!("SELECT {column} FROM {table}");
            let mut statement = connection.prepare(&sql).expect("prepare");
            let rows = statement
                .query_map([], |row| row.get::<_, Option<String>>(0))
                .expect("query");
            for value in rows.flatten().flatten() {
                if value.contains(SENTINEL) {
                    leaked.push(format!("{table}.{column}"));
                }
            }
        }
        assert!(leaked.is_empty(), "敏感内容进入数据库：{leaked:?}");
        assert_eq!(store.message_count().expect("messages"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
