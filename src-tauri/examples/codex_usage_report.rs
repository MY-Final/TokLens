//! Codex 用量对账工具（手动运行，不属于产品功能）。
//!
//! 只读扫描本机 `%USERPROFILE%\.codex\sessions`，把结果写入临时库并导出每个线程的累计值，
//! 便于与 Codex 自身的 `state_5.sqlite`（表 `threads.tokens_used`）抽样对账。
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example codex_usage_report > sessions.tsv
//! ```
//!
//! 输出：`<thread_id>\t<total_tokens 或 -1（无用量数据）>\t<message_coverage>`。

use toklens_lib::collectors::codex::{CodexCollector, SyncOptions};
use toklens_lib::usage::store::{TotalsFilter, UsageStore};
use toklens_lib::usage::tokens::Client;

fn main() {
    let Some(root) = CodexCollector::default_root() else {
        eprintln!("USERPROFILE is not set");
        return;
    };
    eprintln!("Scanning the local Codex sessions directory (read-only)");
    let db_path =
        std::env::temp_dir().join(format!("toklens-verify-{}.sqlite3", std::process::id()));
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }

    {
        let store = UsageStore::open(&db_path).expect("open usage store");
        let collector = CodexCollector::new(&root);
        let started = std::time::Instant::now();
        let report = collector
            .sync(&store, &SyncOptions::default())
            .expect("sync");
        eprintln!(
            "first sync: {:?} files={} parsed={} sessions={} messages={} unparsed_lines={}",
            started.elapsed(),
            report.files_seen,
            report.files_parsed,
            report.sessions_imported,
            report.messages_inserted,
            report.unparsed_lines
        );
        let second = collector
            .sync(&store, &SyncOptions::default())
            .expect("sync");
        eprintln!(
            "second sync (增量): parsed={} skipped_unchanged={}",
            second.files_parsed, second.files_skipped_unchanged
        );
        let totals = store
            .totals(&TotalsFilter {
                client: Some(Client::Codex),
                ..TotalsFilter::default()
            })
            .expect("totals");
        eprintln!(
            "totals: sessions={} messages={} input={} output={} reasoning={} cache_read={} cache_write={} total={}",
            totals.sessions,
            totals.messages,
            totals.input_tokens,
            totals.output_tokens,
            totals.reasoning_tokens,
            totals.cache_read_tokens,
            totals.cache_write_tokens,
            totals.total_tokens
        );
    }

    let connection = rusqlite::Connection::open(&db_path).expect("open usage store read-only");
    let mut statement = connection
        .prepare(
            "SELECT client_session_id, COALESCE(total_tokens, -1), message_coverage
             FROM sessions ORDER BY client_session_id",
        )
        .expect("prepare");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("query sessions");
    for row in rows.flatten() {
        println!("{}\t{}\t{}", row.0, row.1, row.2);
    }
    drop(statement);
    drop(connection);

    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }
}
