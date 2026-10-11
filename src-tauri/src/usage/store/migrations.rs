//! Append-only, transactional schema migrations.

use super::UsageError;
use crate::usage::time::now_ms;
use rusqlite::{Connection, params};

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

/// 版本化迁移表。新增迁移只能追加，不得修改已发布的迁移。
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial_usage_schema",
    sql: INITIAL_SCHEMA_SQL,
}];

const INITIAL_SCHEMA_SQL: &str = include_str!("initial-schema.sql");

/// 执行未应用的迁移（幂等）。
pub(super) fn migrate(conn: &Connection) -> Result<u32, UsageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version       INTEGER PRIMARY KEY,
             name          TEXT    NOT NULL,
             applied_at_ms INTEGER NOT NULL
         );",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    for migration in MIGRATIONS.iter().filter(|m| i64::from(m.version) > current) {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, applied_at_ms) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, now_ms()],
        )?;
        transaction.commit()?;
    }
    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    // 便于外部工具直读；迁移表仍是事实来源。
    conn.execute_batch(&format!("PRAGMA user_version = {version};"))?;
    Ok(version as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::store::UsageStore;
    #[test]
    fn migration_is_applied_once_and_versioned() {
        let store = UsageStore::open_in_memory().expect("open store");
        assert_eq!(store.schema_version().expect("version"), 1);
        let applied: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");
        assert_eq!(applied, MIGRATIONS.len() as i64);
        let version = migrate(&store.conn).expect("re-run migrations");
        assert_eq!(version, 1);
        let applied_again: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");
        assert_eq!(applied_again, applied);
    }
}
