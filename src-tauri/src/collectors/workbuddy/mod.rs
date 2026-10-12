//! Read-only WorkBuddy project transcripts; no content or account data is retained.
mod parser;
mod rollup;
mod sync;

pub use sync::WorkBuddyCollector;
const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
