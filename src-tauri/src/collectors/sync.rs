//! Dispatch independent client scans; one failure never stops the remaining clients.

use super::copilot::CopilotCollector;
use super::workbuddy::WorkBuddyCollector;
use super::{
    SyncOptions, SyncReport, claude_code::ClaudeCodeCollector, codex::CodexCollector,
    dsh::DshCollector, kimi_code::KimiCodeCollector, opencode::OpenCodeCollector,
};
use super::{gemini_cli::GeminiCliCollector, grok::GrokCollector, mcode::MCodeCollector};
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
    pub gemini_cli: Option<PathBuf>,
    pub grok: Option<PathBuf>,
    pub mcode: Option<PathBuf>,
}

/// Root readability only; checking a file never reads its contents.
pub(crate) struct SourceLocation {
    pub(crate) path: PathBuf,
    pub(crate) state: &'static str,
}

impl SourceLocation {
    fn inspect(path: PathBuf) -> Self {
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map(|directory| directory.join(&path))
                .unwrap_or(path)
        };
        let state = match std::fs::metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
            Err(_) => "inaccessible",
            Ok(metadata) if metadata.is_dir() => {
                if std::fs::read_dir(&path).is_ok() {
                    "available"
                } else {
                    "inaccessible"
                }
            }
            Ok(metadata) if metadata.is_file() => {
                if std::fs::File::open(&path).is_ok() {
                    "available"
                } else {
                    "inaccessible"
                }
            }
            Ok(_) => "inaccessible",
        };
        Self { path, state }
    }
}

impl CollectorRoots {
    pub(crate) fn source_locations(&self, client: Client) -> Vec<SourceLocation> {
        self.paths(client)
            .into_iter()
            .map(SourceLocation::inspect)
            .collect()
    }
    /// All configured scan locations, including Copilot's independent metadata sources.
    fn paths(&self, client: Client) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self
            .path(client)
            .map(|path| path.to_path_buf())
            .into_iter()
            .collect();
        if client == Client::Copilot {
            if let Some(root) = self.copilot.as_ref().and_then(|path| path.parent()) {
                paths.push(root.join("session-state"));
            }
            if let Some(root) = &self.copilot_agent_host {
                paths.push(root.clone());
            }
        }
        paths
    }

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
            Client::GeminiCli => &self.gemini_cli,
            Client::Grok => &self.grok,
            Client::Mcode => &self.mcode,
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
            gemini_cli: GeminiCliCollector::default_root(),
            grok: GrokCollector::default_root(),
            mcode: MCodeCollector::default_database(),
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

/// Actual client stages processed, including missing or failed sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProgress {
    pub completed_clients: usize,
    pub total_clients: usize,
    pub current_client: Option<Client>,
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
    sync_clients_with_progress(store, roots, options, clients, |_| {})
}

pub(crate) fn sync_clients_with_progress(
    store: &UsageStore,
    roots: &CollectorRoots,
    options: &SyncOptions,
    clients: &[Client],
    mut on_progress: impl FnMut(SyncProgress),
) -> SyncBatch {
    let mut batch = SyncBatch::default();
    on_progress(SyncProgress {
        completed_clients: 0,
        total_clients: clients.len(),
        current_client: clients.first().copied(),
    });
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
            Client::GeminiCli => roots
                .gemini_cli
                .as_ref()
                .map(|root| GeminiCliCollector::new(root).sync(store, options)),
            Client::Grok => roots
                .grok
                .as_ref()
                .map(|root| GrokCollector::new(root).sync(store, options)),
            Client::Mcode => roots
                .mcode
                .as_ref()
                .map(|root| MCodeCollector::new(root).sync(store, options)),
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
        on_progress(SyncProgress {
            completed_clients: batch.clients.len(),
            total_clients: clients.len(),
            current_client: clients.get(batch.clients.len()).copied(),
        });
    }
    batch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::TempDir;

    #[test]
    fn single_client_rescan_leaves_other_sources_unprocessed() {
        let dir = TempDir::new();
        let invalid = dir.0.join("invalid.sqlite");
        std::fs::write(&invalid, b"invalid").expect("synthetic source");
        let store = UsageStore::open_in_memory().expect("store");
        let roots = CollectorRoots {
            mcode: Some(invalid),
            grok: Some(dir.0.join("missing-grok")),
            ..Default::default()
        };
        let batch = sync_clients(
            &store,
            &roots,
            &SyncOptions { force_rescan: true },
            &[Client::Grok],
        );
        assert_eq!(batch.clients.len(), 1);
        assert_eq!(batch.clients[0].client, Client::Grok);
        assert!(
            batch.clients[0]
                .report
                .as_ref()
                .expect("report")
                .root_missing
        );
        assert!(
            !batch.has_errors(),
            "the invalid non-selected database is not scanned"
        );
    }

    #[test]
    fn copilot_diagnostics_include_all_scan_roots() {
        let roots = CollectorRoots {
            copilot: Some(PathBuf::from("fixture/copilot/session-store.db")),
            copilot_agent_host: Some(PathBuf::from("fixture/code/agentSessionData")),
            ..Default::default()
        };
        assert_eq!(
            roots.paths(Client::Copilot),
            vec![
                PathBuf::from("fixture/copilot/session-store.db"),
                PathBuf::from("fixture/copilot/session-state"),
                PathBuf::from("fixture/code/agentSessionData"),
            ]
        );
        assert!(roots.paths(Client::Codex).is_empty());
    }

    #[test]
    fn source_location_checks_only_readability_and_distinguishes_missing_roots() {
        let dir = TempDir::new();
        let file = dir.0.join("metadata.sqlite");
        std::fs::write(&file, b"synthetic").expect("source fixture");
        assert_eq!(SourceLocation::inspect(dir.0.clone()).state, "available");
        assert_eq!(SourceLocation::inspect(file.clone()).state, "available");
        assert_eq!(
            SourceLocation::inspect(dir.0.join("missing")).state,
            "missing"
        );
        assert_eq!(std::fs::read(file).expect("unchanged source"), b"synthetic");
    }

    #[test]
    fn new_clients_report_independently_after_an_invalid_mcode_database() {
        let dir = TempDir::new();
        let database = dir.0.join("invalid.sqlite");
        std::fs::write(&database, b"not a SQLite database").expect("synthetic source");
        let store = UsageStore::open_in_memory().expect("store");
        let roots = CollectorRoots {
            mcode: Some(database),
            gemini_cli: Some(dir.0.join("missing-gemini")),
            grok: Some(dir.0.join("missing-grok")),
            ..Default::default()
        };
        let mut progress = Vec::new();
        let batch = sync_clients_with_progress(
            &store,
            &roots,
            &SyncOptions::default(),
            &[Client::Mcode, Client::GeminiCli, Client::Grok],
            |update| progress.push(update),
        );
        assert_eq!(
            progress,
            vec![
                SyncProgress {
                    completed_clients: 0,
                    total_clients: 3,
                    current_client: Some(Client::Mcode)
                },
                SyncProgress {
                    completed_clients: 1,
                    total_clients: 3,
                    current_client: Some(Client::GeminiCli)
                },
                SyncProgress {
                    completed_clients: 2,
                    total_clients: 3,
                    current_client: Some(Client::Grok)
                },
                SyncProgress {
                    completed_clients: 3,
                    total_clients: 3,
                    current_client: None
                },
            ]
        );
        assert_eq!(batch.clients.len(), 3);
        assert!(batch.has_errors());
        for entry in &batch.clients[1..] {
            assert!(entry.error.is_none());
            assert!(
                entry
                    .report
                    .as_ref()
                    .expect("missing source report")
                    .root_missing
            );
        }
        assert_eq!(store.session_count().expect("no imported data"), 0);
    }

    #[test]
    fn no_enabled_clients_reports_an_empty_completed_scan() {
        let store = UsageStore::open_in_memory().expect("store");
        let mut progress = Vec::new();
        let batch = sync_clients_with_progress(
            &store,
            &CollectorRoots::default(),
            &SyncOptions::default(),
            &[],
            |update| progress.push(update),
        );
        assert!(batch.clients.is_empty());
        assert_eq!(
            progress,
            vec![SyncProgress {
                completed_clients: 0,
                total_clients: 0,
                current_client: None
            }]
        );
    }
}
