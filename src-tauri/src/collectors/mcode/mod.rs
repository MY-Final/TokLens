//! MCode's SQLite request ledger; never read versioned message history or raw JSON.
mod discovery;
mod parser;
mod sync;

pub use sync::MCodeCollector;
const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
