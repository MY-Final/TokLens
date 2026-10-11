//! DSH / deepseek-hermes v0–v4 session artifacts (plain or concatenated Zstandard frames).

mod format;
mod metadata;
mod parser;
mod projection;
mod sync;

pub use sync::DshCollector;
pub(super) struct Parsed {
    pub import: crate::usage::store::SessionImport,
    pub unparsed: usize,
}

pub const PARSER_VERSION: i64 = 2;

#[cfg(test)]
mod tests;
