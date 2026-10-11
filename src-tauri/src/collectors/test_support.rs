//! Isolated synthetic source directories; no tests depend on a real client or clock.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "toklens-collectors-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create synthetic directory");
        Self(path)
    }
    pub fn write(&self, path: &str, content: &str) -> PathBuf {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("create fixture parent");
        std::fs::write(&path, content).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn assert_private(store: &crate::usage::store::UsageStore) {
    let conn = store.connection_for_tests();
    for table in ["sessions", "messages", "source_files"] {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {table}"))
            .expect("privacy query");
        let mut rows = stmt.query([]).expect("rows");
        while let Some(row) = rows.next().expect("row") {
            for index in 0..row.as_ref().column_count() {
                if let rusqlite::types::ValueRef::Text(value) = row.get_ref(index).expect("column")
                {
                    assert!(!String::from_utf8_lossy(value).contains("PRIVATE_SENTINEL"));
                }
            }
        }
    }
}
