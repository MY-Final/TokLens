use super::{UsageError, UsageStore};

impl UsageStore {
    pub fn database_path(&self) -> std::path::PathBuf {
        self.conn
            .path()
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
    }

    /// SQLite creates a consistent snapshot including committed WAL records.
    pub fn snapshot_to(&self, path: &std::path::Path) -> Result<(), UsageError> {
        let path = path
            .to_str()
            .ok_or_else(|| UsageError::InvalidInput("Storage path must be valid Unicode".into()))?;
        self.conn.execute("VACUUM INTO ?1", [path])?;
        let snapshot = rusqlite::Connection::open(path)?;
        let result: String = snapshot.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result != "ok" {
            return Err(UsageError::InvalidInput(
                "Database snapshot failed integrity validation".into(),
            ));
        }
        drop(snapshot);
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?
            .sync_all()?;
        Ok(())
    }

    /// Clear only TokLens metadata and import watermarks, retaining schema and migrations.
    pub fn clear_statistics(&self) -> Result<(), UsageError> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute_batch(
            "DELETE FROM messages; DELETE FROM sessions; DELETE FROM source_files;",
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clear_removes_records_and_watermarks_without_changing_schema() {
        let store = UsageStore::open_in_memory().expect("store");
        store.conn.execute_batch("INSERT INTO source_files(client,path,kind,parser_version) VALUES('codex','synthetic.jsonl','jsonl',1);
          INSERT INTO sessions(id,client,client_session_id,semantics_id,total_basis,message_coverage,source_kind,cost_source,imported_at_ms,updated_at_ms)
          VALUES('s','codex','s','codex_v1','sum_of_messages','complete','measured','none',1,1);
          INSERT INTO messages(id,session_id,client,dedup_key,semantics_id,source_kind,cost_source,imported_at_ms,updated_at_ms)
          VALUES('m','s','codex','m','codex_v1','measured','none',1,1);").expect("synthetic records");
        store.clear_statistics().expect("clear");
        for table in ["messages", "sessions", "source_files"] {
            let count: i64 = store
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count");
            assert_eq!(count, 0);
        }
        assert_eq!(store.schema_version().expect("version"), 1);
        store.clear_statistics().expect("idempotent");
    }
}
