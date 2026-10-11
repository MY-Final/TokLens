//! OpenCode SQLite collector. No content tables or credential tables are read.

mod queries;
mod sync;

pub use sync::OpenCodeCollector;
pub const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
