//! Stable main/WAL signatures fence a metadata-only, atomic full-ledger import.
use super::{PARSER_VERSION, discovery, parser};
use crate::{
    collectors::{SyncError, SyncOptions, SyncReport, files::home_dir, sqlite_snapshot},
    usage::{
        client::Client,
        store::{SourceFile, SourceFileKind, UsageStore},
        time::now_ms,
    },
};
use std::path::{Path, PathBuf};

pub struct MCodeCollector {
    database: PathBuf,
}

impl MCodeCollector {
    pub fn new(database: impl Into<PathBuf>) -> Self {
        Self {
            database: database.into(),
        }
    }
    pub fn database(&self) -> &Path {
        &self.database
    }
    pub fn default_database() -> Option<PathBuf> {
        discovery::database(
            std::env::var_os("MINIMAX_DATA_DIR"),
            std::env::var_os("MAVIS_DATA_DIR"),
            home_dir(),
        )
    }
    pub fn sync(
        &self,
        store: &UsageStore,
        _options: &SyncOptions,
    ) -> Result<SyncReport, SyncError> {
        match self.sync_before_commit(store, || {}) {
            Err(SyncError::Source(_) | SyncError::Database(_)) => Ok(SyncReport {
                files_seen: 1,
                files_failed: 1,
                ..SyncReport::default()
            }),
            result => result,
        }
    }
    pub(super) fn sync_before_commit(
        &self,
        store: &UsageStore,
        before_commit: impl FnOnce(),
    ) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.database.try_exists()? {
            report.root_missing = true;
            return Ok(report);
        }
        let observed = sqlite_snapshot::signature(&self.database)?;
        let conn = sqlite_snapshot::open(&self.database)?;
        let path = self.database.to_string_lossy().to_string();
        let snapshot = parser::snapshot(&conn, &path)?;
        before_commit();
        if sqlite_snapshot::signature(&self.database)? != observed {
            return Err(SyncError::ChangedDuringScan);
        }
        let source = SourceFile {
            client: Client::Mcode,
            path,
            kind: SourceFileKind::Sqlite,
            size_bytes: Some(observed.0.0),
            mtime_ms: Some(observed.0.1),
            content_offset: 0,
            parser_version: PARSER_VERSION,
            last_scan_ms: Some(now_ms()),
            unparsed_records: snapshot.unparsed as i64,
        };
        let imported =
            store.replace_client_snapshot(Client::Mcode, &[source], &snapshot.sessions)?;
        report.files_seen = 1;
        report.files_parsed = 1;
        report.sessions_imported = snapshot.sessions.len();
        report.unparsed_lines = snapshot.unparsed;
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}
