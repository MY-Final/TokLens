//! Grok's authoritative usage snapshots; chat and event logs are never read.
mod discovery;
mod parser;
mod sync;

pub use sync::GrokCollector;
const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
