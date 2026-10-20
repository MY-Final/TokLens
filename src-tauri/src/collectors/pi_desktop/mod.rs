//! Read-only PiDesktop metadata collector.
mod ledger;
mod parser;
mod sync;
pub use sync::PiDesktopCollector;
pub const PARSER_VERSION: i64 = 2;
#[cfg(test)]
mod tests;
