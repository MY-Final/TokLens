//! Codex 采集器：扫描 `%USERPROFILE%\.codex\sessions\` 下的 `rollout-*.jsonl`。
//!
//! 解析与口径要点（依据见 `docs/data-source-research.md`）：
//!
//! - 兼容两套 Token 记录：新版 `token_usage_record`（`usage` 为每请求增量、
//!   `thread_token_usage` 为线程累计）与旧版 `event_msg.token_count`
//!   （`info.last_token_usage` 为每请求增量、`info.total_token_usage` 为线程累计）。
//!   混合格式保留旧格式历史，丢弃与新版配对的镜像事件，避免重复计数。
//! - 线程累计值按 `thread_id` 取最大一份，绝不相加多个文件的累计值；分页/续接文件
//!   通过 `first_ordinal != 0` 标记为明细不完整。
//! - 增量同步按 `(size_bytes, mtime_ms, parser_version)` 判断文件是否变化：未变化直接跳过，
//!   变化则整文件重新解析并以单事务覆盖写入（幂等，且能正确处理追加、截断与替换）。
//!
//! 已知限制：既不改变大小也不改变 mtime 的"同尺寸内容替换"无法被察觉（rollout 文件按追加写入，
//! 这种情况视为异常）；`SyncOptions::force_rescan` 提供显式全量重扫的兜底路径。

mod discovery;
mod parsed;
mod parser;
mod sync;
mod thread;
mod types;

pub use parser::FIELD_WHITELIST;
pub use sync::CodexCollector;
pub use types::{PARSER_VERSION, ScanReason, SyncError, SyncOptions, SyncReport};

#[cfg(test)]
mod tests;
