//! Manual read-only reconciliation for the new collectors; outputs aggregate metadata only.

use toklens_lib::collectors::{CollectorRoots, SyncOptions, sync_clients};
use toklens_lib::usage::analytics::AnalyticsFilter;
use toklens_lib::usage::client::Client;
use toklens_lib::usage::store::UsageStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = UsageStore::open_in_memory()?;
    let defaults = [
        Client::ClaudeCode,
        Client::OpenCode,
        Client::Dsh,
        Client::KimiCode,
        Client::MimoCode,
        Client::PiDesktop,
    ];
    let requested = std::env::args()
        .skip(1)
        .map(|value| {
            Client::parse(&value).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "unknown client id")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let clients = if requested.is_empty() {
        defaults.as_slice()
    } else {
        requested.as_slice()
    };
    let batch = sync_clients(
        &store,
        &CollectorRoots::from_environment(),
        &SyncOptions::default(),
        clients,
    );
    for result in batch.clients {
        if let Some(error) = result.error {
            eprintln!("{}: {error}", result.client);
        }
        let totals = store.range_totals(&AnalyticsFilter {
            client: Some(result.client),
            ..Default::default()
        })?;
        let sums = totals.sums;
        println!(
            "{}",
            serde_json::json!({
                "client": result.client.as_str(), "sessions": totals.sessions, "messages": totals.messages,
                "unknownSessions": totals.sessions_without_usage, "partialSessions": totals.sessions_partial,
                "input": sums.input_tokens, "output": sums.output_tokens, "reasoning": sums.reasoning_tokens,
                "cacheRead": sums.cache_read_tokens, "cacheWrite": sums.cache_write_tokens, "total": sums.total_tokens,
                "report": result.report.map(|report| serde_json::json!({
                    "filesSeen": report.files_seen, "filesParsed": report.files_parsed,
                    "filesFailed": report.files_failed, "unparsed": report.unparsed_lines,
                })),
            })
        );
    }
    Ok(())
}
