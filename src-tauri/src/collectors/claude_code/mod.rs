//! Claude Code transcripts, including subagents; metadata only.

mod parser;
mod rollup;
mod sync;

pub use sync::ClaudeCodeCollector;
pub const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
