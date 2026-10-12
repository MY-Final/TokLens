//! Read-only Gemini CLI conversation journal metadata; excludes Antigravity.
mod discovery;
mod parser;
mod sync;

pub use sync::GeminiCliCollector;
const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
