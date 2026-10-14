//! Read the committed main+WAL image in memory so the client's -wal/-shm are never opened.

use super::{PARSER_VERSION, queries};
use crate::collectors::files::{fingerprint, home_dir};
use crate::collectors::sqlite_snapshot;
use crate::collectors::{SyncError, SyncOptions, SyncReport};
use crate::usage::client::Client;
use crate::usage::store::{SourceFile, SourceFileKind, UsageStore};
use crate::usage::time::now_ms;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub struct MiMoCodeCollector {
    database: PathBuf,
}

impl MiMoCodeCollector {
    pub fn default_database() -> Option<PathBuf> {
        resolve_database(
            std::env::var_os("MIMOCODE_DB"),
            std::env::var_os("XDG_DATA_HOME"),
            home_dir(),
        )
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
            client: Client::MimoCode,
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
            store.replace_client_snapshot(Client::MimoCode, &[source], &snapshot.sessions)?;
        report.files_parsed = 1;
        report.unparsed_lines = snapshot.unparsed;
        report.sessions_imported = snapshot.sessions.len();
        report.messages_inserted = imported.messages_inserted;
        report.messages_pruned = imported.messages_pruned;
        Ok(report)
    }
}

/// `MIMOCODE_DB` wins outright when it is absolute, even without `XDG_DATA_HOME` or
/// HOME. A relative value or the default is resolved under `<data root>/mimocode`.
pub(super) fn resolve_database(
    mimocode_db: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(path) = mimocode_db
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        if path.is_absolute() {
            return Some(path);
        }
        return data_root(xdg_data_home, home).map(|data| data.join("mimocode").join(path));
    }
    data_root(xdg_data_home, home).map(|data| data.join("mimocode").join("mimocode.db"))
}

fn data_root(xdg_data_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    xdg_data_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".local").join("share")))
}
