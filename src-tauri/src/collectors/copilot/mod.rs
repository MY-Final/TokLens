//! Read-only Copilot CLI and VS Code Agent Host metadata.
mod agent_host;
use super::sqlite_snapshot as database;
mod events;
mod queries;
mod sync;
pub use sync::CopilotCollector;
const PARSER_VERSION: i64 = 2;
#[cfg(test)]
mod agent_host_tests;
#[cfg(test)]
mod tests;
