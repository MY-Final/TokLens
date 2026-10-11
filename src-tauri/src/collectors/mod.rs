//! Read-only client collectors and shared synchronization contracts.

pub mod claude_code;
pub mod codex;
pub mod dsh;
mod files;
pub mod kimi_code;
pub mod opencode;
mod sync;
mod types;

pub use sync::{ClientSyncResult, CollectorRoots, SyncBatch, sync_clients};
pub use types::{ScanReason, SyncError, SyncOptions, SyncReport};

#[cfg(test)]
mod test_support;
