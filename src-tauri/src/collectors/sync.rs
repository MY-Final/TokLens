//! Dispatch independent client scans; one failure never stops the remaining clients.

use super::{
    SyncOptions, SyncReport, claude_code::ClaudeCodeCollector, codex::CodexCollector,
    dsh::DshCollector, kimi_code::KimiCodeCollector, opencode::OpenCodeCollector,
};
use crate::usage::client::Client;
use crate::usage::store::UsageStore;
use std::path::PathBuf;

#[derive(Default)]
pub struct CollectorRoots {
    pub codex: Option<PathBuf>,
    pub claude_code: Option<PathBuf>,
    pub opencode: Option<PathBuf>,
    pub dsh: Option<PathBuf>,
    pub kimi_code: Option<PathBuf>,
}

impl CollectorRoots {
    pub fn from_environment() -> Self {
        Self {
            codex: CodexCollector::default_root(),
            claude_code: ClaudeCodeCollector::default_root(),
            opencode: OpenCodeCollector::default_database(),
            dsh: DshCollector::default_root(),
            kimi_code: KimiCodeCollector::default_root(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientSyncResult {
    pub client: Client,
    pub report: Option<SyncReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SyncBatch {
    pub clients: Vec<ClientSyncResult>,
}

impl SyncBatch {
    pub fn has_errors(&self) -> bool {
        self.clients.iter().any(|client| {
            client.error.is_some()
                || client
                    .report
                    .as_ref()
                    .is_some_and(|report| report.files_failed > 0 || report.directories_failed > 0)
        })
    }
}

pub fn sync_clients(
    store: &UsageStore,
    roots: &CollectorRoots,
    options: &SyncOptions,
    clients: &[Client],
) -> SyncBatch {
    let mut batch = SyncBatch::default();
    for &client in clients {
        let result = match client {
            Client::Codex => roots
                .codex
                .as_ref()
                .map(|root| CodexCollector::new(root).sync(store, options)),
            Client::ClaudeCode => roots
                .claude_code
                .as_ref()
                .map(|root| ClaudeCodeCollector::new(root).sync(store, options)),
            Client::OpenCode => roots
                .opencode
                .as_ref()
                .map(|root| OpenCodeCollector::new(root).sync(store, options)),
            Client::KimiCode => roots
                .kimi_code
                .as_ref()
                .map(|root| KimiCodeCollector::new(root).sync(store, options)),
            Client::Dsh => roots
                .dsh
                .as_ref()
                .map(|root| DshCollector::new(root).sync(store, options)),
        }
        .unwrap_or_else(|| {
            Ok(SyncReport {
                root_missing: true,
                ..Default::default()
            })
        });
        batch.clients.push(match result {
            Ok(report) => ClientSyncResult {
                client,
                report: Some(report),
                error: None,
            },
            Err(error) => ClientSyncResult {
                client,
                report: None,
                error: Some(error.to_string()),
            },
        });
    }
    batch
}
