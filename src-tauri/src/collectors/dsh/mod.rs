//! DSH / deepseek-hermes v0 session artifacts (plain or concatenated Zstandard frames).

mod parser;
mod sync;

pub use sync::DshCollector;
pub const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
