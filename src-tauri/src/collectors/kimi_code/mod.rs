//! Read-only Kimi Code session metadata and agent usage ledgers.

mod discovery;
mod metadata;
mod parser;
mod sync;
mod workdirs;

pub use sync::KimiCodeCollector;
pub const PARSER_VERSION: i64 = 2;

#[cfg(test)]
mod tests;
