//! Dispatch independent client scans; one failure never stops the remaining clients.

use super::copilot::CopilotCollector;
use super::workbuddy::WorkBuddyCollector;
use super::{
    SyncOptions, SyncReport, claude_code::ClaudeCodeCollector, codex::CodexCollector,
    dsh::DshCollector, kimi_code::KimiCodeCollector, opencode::OpenCodeCollector,
};
use super::{mimocode::MiMoCodeCollector, pi_desktop::PiDesktopCollector};
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
    pub mimocode: Option<PathBuf>,
    pub pi_desktop: Option<PathBuf>,
    pub workbuddy: Option<PathBuf>,
    pub copilot: Option<PathBuf>,
    pub copilot_agent_host: Option<PathBuf>,
}

impl CollectorRoots {
    pub fn path(&self, client: Client) -> Option<&std::path::Path> {
        match client {
            Client::Codex => &self.codex,
            Client::ClaudeCode => &self.claude_code,
            Client::OpenCode => &self.opencode,
            Client::Dsh => &self.dsh,
            Client::KimiCode => &self.kimi_code,
            Client::MimoCode => &self.mimocode,
            Client::PiDesktop => &self.pi_desktop,
            Client::WorkBuddy => &self.workbuddy,
            Client::Copilot => &self.copilot,
        }
        .as_deref()
    }

    pub fn from_environment() -> Self {
        Self {
            codex: CodexCollector::default_root(),
            claude_code: ClaudeCodeCollector::default_root(),
            opencode: OpenCodeCollector::default_database(),
            dsh: DshCollector::default_root(),
            kimi_code: KimiCodeCollector::default_root(),
            mimocode: MiMoCodeCollector::default_database(),
            pi_desktop: PiDesktopCollector::default_root(),
            workbuddy: WorkBuddyCollector::default_root(),
            copilot: CopilotCollector::default_database(),
            copilot_agent_host: CopilotCollector::default_agent_host(),
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
            Client::MimoCode => roots
                .mimocode
                .as_ref()
                .map(|root| MiMoCodeCollector::new(root).sync(store, options)),
            Client::PiDesktop => roots
                .pi_desktop
                .as_ref()
                .map(|root| PiDesktopCollector::new(root).sync(store, options)),
            Client::Dsh => roots
                .dsh
                .as_ref()
                .map(|root| DshCollector::new(root).sync(store, options)),
            Client::Copilot => roots.copilot.as_ref().map(|root| {
                CopilotCollector::new(root)
                    .with_agent_host(roots.copilot_agent_host.clone())
                    .sync(store, options)
            }),
            Client::WorkBuddy => roots
                .workbuddy
                .as_ref()
                .map(|root| WorkBuddyCollector::new(root).sync(store, options)),
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
