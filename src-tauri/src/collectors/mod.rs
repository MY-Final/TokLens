//! Read-only client collectors and shared synchronization contracts.

pub mod claude_code;
pub mod codex;
pub mod copilot;
pub mod dsh;
mod files;
pub mod gemini_cli;
pub mod grok;
pub mod kimi_code;
pub mod mcode;
pub mod mimocode;
pub mod opencode;
pub mod pi_desktop;
pub(crate) mod sqlite_snapshot;
mod sync;
mod types;
pub mod workbuddy;

pub use sync::{ClientSyncResult, CollectorRoots, SyncBatch, SyncProgress, sync_clients};
pub(crate) use sync::{SourceLocation, sync_clients_with_progress};
pub use types::{ScanReason, SyncError, SyncOptions, SyncReport};

#[cfg(test)]
mod test_support;
