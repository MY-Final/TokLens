//! Manual read-only reconciliation for the new collectors; outputs aggregate metadata only.

use toklens_lib::collectors::{CollectorRoots, SyncOptions, sync_clients};
use toklens_lib::usage::analytics::AnalyticsFilter;
use toklens_lib::usage::client::Client;
use toklens_lib::usage::store::UsageStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = UsageStore::open_in_memory()?;
    let clients = [
        Client::ClaudeCode,
        Client::OpenCode,
        Client::Dsh,
        Client::KimiCode,
        Client::MimoCode,
        Client::PiDesktop,
    ];
    let batch = sync_clients(
        &store,
        &CollectorRoots::from_environment(),
        &SyncOptions::default(),
        &clients,
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
                "unparsed": result.report.map(|report| report.unparsed_lines),
            })
        );
    }
    Ok(())
}
