//! Read the committed main+WAL image in memory so the client's -wal/-shm are never opened.

use super::{PARSER_VERSION, queries};
use crate::collectors::files::{fingerprint, home_dir};
use crate::collectors::sqlite_snapshot;
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::path::{Path, PathBuf};

pub struct OpenCodeCollector {
    database: PathBuf,
}

impl OpenCodeCollector {
    pub fn default_database() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
            .map(|data| data.join("opencode").join("opencode.db"))
    }

    pub fn new(database: impl Into<PathBuf>) -> Self {
        Self {
            database: database.into(),
        }
    }
    pub fn database(&self) -> &Path {
        &self.database
    }

    pub fn sync(
        &self,
        store: &UsageStore,
        _options: &SyncOptions,
    ) -> Result<SyncReport, SyncError> {
        let mut report = SyncReport::default();
        if !self.database.try_exists()? {
            report.root_missing = true;
            return Ok(report);
        }
        report.files_seen = 1;
        let path = self.database.to_string_lossy().to_string();
        // Read the main file plus committed WAL frames into memory. SQLite never opens
        // the source, so no -wal/-shm is created or locked beside the client database.
        let conn = sqlite_snapshot::open(&self.database)?;
        let snapshot = queries::snapshot(&conn, &path)?;
        let (size, mtime) = fingerprint(&self.database)?;
        let source = SourceFile {
            client: Client::OpenCode,
            path,
            kind: SourceFileKind::Sqlite,
            size_bytes: Some(size),
            mtime_ms: Some(mtime),
            content_offset: 0,
            parser_version: PARSER_VERSION,
            last_scan_ms: Some(now_ms()),
            unparsed_records: snapshot.unparsed as i64,
        };
        let imported =
            store.replace_client_snapshot(Client::OpenCode, &[source], &snapshot.sessions)?;
        report.files_parsed = 1;
        report.unparsed_lines = snapshot.unparsed;
        report.sessions_imported = snapshot.sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}
