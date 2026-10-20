//! OpenCode SQLite collector. No content tables or credential tables are read.

mod queries;
mod sync;

pub use sync::OpenCodeCollector;
/// v2：新增 `session_message` 表来源（客户端 2026-10-18 起的写入目标）。
pub const PARSER_VERSION: i64 = 2;

#[cfg(test)]
mod tests;
