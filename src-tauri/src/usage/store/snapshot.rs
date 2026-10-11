//! Atomic replacement for collectors that provide a complete client snapshot.

use super::writes::{upsert_message_conn, upsert_session_conn, upsert_source_file_conn};
use super::{ImportSummary, SourceFile, UsageError, UsageStore};
use crate::usage::client::Client;
use crate::usage::model::{MessageRecord, SessionRecord};
use rusqlite::params;
use std::collections::BTreeSet;

/// One complete session and its deduplicated metadata-only requests.
pub struct SessionImport {
    pub session: SessionRecord,
    pub messages: Vec<MessageRecord>,
    pub source_path: String,
    pub message_source_paths: std::collections::BTreeMap<String, String>,
}

impl UsageStore {
    /// Only call after a complete, successful scan. Missing roots and failed scans must not call this.
    /// Replacement also handles corrected totals, truncated files and deleted source sessions.
    pub fn replace_client_snapshot(
        &self,
        client: Client,
        files: &[SourceFile],
        sessions: &[SessionImport],
    ) -> Result<ImportSummary, UsageError> {
        let mut paths = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for file in files {
            if file.client != client || !paths.insert(file.path.as_str()) {
                return Err(UsageError::InvalidInput("invalid snapshot source".into()));
            }
        }
        for entry in sessions {
            if entry.session.client != client
                || !ids.insert(entry.session.client_session_id.as_str())
                || !paths.contains(entry.source_path.as_str())
                || entry
                    .message_source_paths
                    .values()
                    .any(|path| !paths.contains(path.as_str()))
                || entry.messages.iter().any(|message| {
                    message.client != client
                        || message.client_session_id != entry.session.client_session_id
                })
            {
                return Err(UsageError::InvalidInput("invalid snapshot session".into()));
            }
        }
        let transaction = self.conn.unchecked_transaction()?;
        let pruned: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM messages WHERE client = ?1",
            [client.as_str()],
            |row| row.get(0),
        )?;
        transaction.execute("DELETE FROM sessions WHERE client = ?1", [client.as_str()])?;
        transaction.execute(
            "DELETE FROM source_files WHERE client = ?1",
            [client.as_str()],
        )?;
        let mut sources = std::collections::BTreeMap::new();
        for file in files {
            sources.insert(
                file.path.as_str(),
                upsert_source_file_conn(&transaction, file)?,
            );
        }
        let mut summary = ImportSummary {
            messages_pruned: pruned as usize,
            ..Default::default()
        };
        for entry in sessions {
            summary.session = Some(upsert_session_conn(&transaction, &entry.session)?);
            for message in &entry.messages {
                upsert_message_conn(
                    &transaction,
                    message,
                    sources
                        .get(
                            entry
                                .message_source_paths
                                .get(&message.dedup_key)
                                .unwrap_or(&entry.source_path)
                                .as_str(),
                        )
                        .copied(),
                )?;
                summary.messages_inserted += 1;
            }
        }
        transaction.commit()?;
        Ok(summary)
    }

    pub fn source_paths(&self, client: Client) -> Result<Vec<String>, UsageError> {
        let mut statement = self
            .conn
            .prepare("SELECT path FROM source_files WHERE client = ?1 ORDER BY path")?;
        Ok(statement
            .query_map(params![client.as_str()], |row| row.get(0))?
            .collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::normalize::{ClaudeSessionRollup, claude_code_session};
    use crate::usage::store::SourceFileKind;
    use crate::usage::tokens::RawTokenCounts;

    #[test]
    fn failed_snapshot_rolls_back_watermarks_and_preserves_other_clients() {
        let store = UsageStore::open_in_memory().expect("store");
        let file = SourceFile {
            client: Client::ClaudeCode,
            path: "fixture.jsonl".into(),
            kind: SourceFileKind::Jsonl,
            size_bytes: Some(1),
            mtime_ms: Some(1),
            content_offset: 1,
            parser_version: 1,
            last_scan_ms: Some(1),
            unparsed_records: 0,
        };
        let session = claude_code_session(&ClaudeSessionRollup {
            session_id: "s1",
            cwd: None,
            model: None,
            started_at: None,
            ended_at: None,
            message_count: 0,
            usage: RawTokenCounts::default(),
        });
        store
            .import_session(&file, &session, &[])
            .expect("existing");
        let mut other = session.clone();
        other.client = Client::Codex;
        other.client_session_id = "keep".into();
        store.upsert_session(&other).expect("other client");
        // The empty source path fails during writes, after DELETE, so this exercises real rollback.
        let bad = SourceFile {
            path: String::new(),
            ..file.clone()
        };
        assert!(
            store
                .replace_client_snapshot(Client::ClaudeCode, &[bad], &[])
                .is_err()
        );
        assert_eq!(store.session_count().expect("count"), 2);
        assert_eq!(
            store
                .source_file(Client::ClaudeCode, &file.path)
                .expect("watermark")
                .expect("preserved")
                .size_bytes,
            Some(1)
        );
        store
            .replace_client_snapshot(Client::ClaudeCode, &[], &[])
            .expect("complete empty snapshot");
        assert_eq!(store.session_count().expect("count"), 1);
        assert!(
            store
                .source_paths(Client::ClaudeCode)
                .expect("paths")
                .is_empty()
        );
    }
}
