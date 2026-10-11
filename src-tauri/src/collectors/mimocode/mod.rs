//! Read-only MiMoCode metadata collector.
mod queries;
mod sync;

pub use sync::MiMoCodeCollector;
pub const PARSER_VERSION: i64 = 1;
#[cfg(test)]
mod tests;
