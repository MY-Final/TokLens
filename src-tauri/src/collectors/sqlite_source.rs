//! 只读访问客户端 SQLite 库：客户端在运行时就地只读打开，否则回退到内存快照。
//!
//! 客户端库会持续变大（OpenCode 的本地库已超过 2 GB），而 SQLite 单次内存分配
//! 上限约为 2 GB，把整库复制成内存快照会直接失败（`SQLITE_NOMEM`）。
//! 客户端运行时直接就地只读打开：不复制整库，只按需读取页面，既不受该上限约束，
//! 也不会在客户端目录里创建或改写任何文件。
//!
//! 客户端未运行时，SQLite 的只读连接会自行创建 `-shm` / `-wal`，与"不在客户端目录
//! 落任何文件"的约束冲突；这种情况改用内存快照（`sqlite_snapshot`），只有小于
//! 上限的库才装得下。

use crate::collectors::{SyncError, sqlite_snapshot};
use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 打开客户端库用于只读统计。
pub(crate) fn open(path: &Path) -> Result<Connection, SyncError> {
    if client_is_running(path)
        && let Ok(conn) = open_read_only(path)
    {
        return Ok(conn);
    }
    match sqlite_snapshot::open(path) {
        Ok(conn) => Ok(conn),
        Err(SyncError::Database(error)) if is_out_of_memory(&error) => Err(SyncError::TooLarge),
        Err(error) => Err(error),
    }
}

/// 就地只读打开客户端库，并在同一个读事务内读取。
///
/// 延迟读事务保证后续多条查询看到同一个时点：客户端并发写入时，会话累计值与
/// 逐条明细不会读到不同的快照。
pub(crate) fn open_read_only(path: &Path) -> Result<Connection, SyncError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "query_only", true)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.execute_batch("BEGIN DEFERRED")?;
    Ok(conn)
}

/// 客户端是否正持有这个库：`-wal` 与共享内存索引同时存在才说明它开着这个库。
///
/// 只有这种情况下才敢就地只读打开：此时 SQLite 直接复用客户端的 `-wal` / `-shm`，
/// 不会新建或改写客户端目录里的任何文件。
fn client_is_running(path: &Path) -> bool {
    side_file(path, "-wal").is_file() && side_file(path, "-shm").is_file()
}

fn side_file(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn is_out_of_memory(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _) if failure.code == ErrorCode::OutOfMemory
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::TempDir;

    fn write_db(path: &Path) -> Connection {
        let conn = Connection::open(path).expect("fixture db");
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE usage(id TEXT PRIMARY KEY, tokens INTEGER);
             INSERT INTO usage VALUES('a', 7);",
        )
        .expect("fixture data");
        conn
    }

    fn read_tokens(conn: &Connection) -> i64 {
        conn.query_row("SELECT tokens FROM usage WHERE id = 'a'", [], |row| {
            row.get(0)
        })
        .expect("tokens")
    }

    /// 客户端库目录里除主库之外的文件名。
    fn side_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("source dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name != "client.db")
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_running_client_is_read_in_place_without_touching_its_directory() {
        let dir = TempDir::new();
        let path = dir.0.join("client.db");
        let writer = write_db(&path);
        let before = side_files(&dir.0);
        // 客户端仍持有这个库：-wal 与 -shm 都在。
        assert!(before.contains(&"client.db-wal".to_string()));
        assert!(before.contains(&"client.db-shm".to_string()));
        let conn = open(&path).expect("open");
        assert_eq!(read_tokens(&conn), 7);
        assert_eq!(side_files(&dir.0), before);
        drop(writer);
    }

    #[test]
    fn a_stopped_client_falls_back_to_the_memory_snapshot() {
        let dir = TempDir::new();
        let path = dir.0.join("client.db");
        drop(write_db(&path));
        // 客户端已退出：只剩主库。
        assert_eq!(side_files(&dir.0), Vec::<String>::new());
        let conn = open(&path).expect("open");
        assert_eq!(read_tokens(&conn), 7);
        // 快照路径不会在客户端目录里留下任何文件。
        assert_eq!(side_files(&dir.0), Vec::<String>::new());
    }

    #[test]
    fn the_read_transaction_keeps_one_point_in_time_across_queries() {
        let dir = TempDir::new();
        let path = dir.0.join("client.db");
        let writer = write_db(&path);
        let conn = open_read_only(&path).expect("read-only open");
        let first = read_tokens(&conn);
        // 另一个连接在事务之外写入，读事务内仍应看到旧值。
        writer
            .execute("UPDATE usage SET tokens = 9 WHERE id = 'a'", [])
            .expect("update");
        assert_eq!(first, 7);
        assert_eq!(read_tokens(&conn), 7);
        conn.execute_batch("COMMIT").expect("commit");
    }

    #[test]
    fn a_missing_database_is_reported_as_an_error() {
        let dir = TempDir::new();
        assert!(open_read_only(&dir.0.join("missing.db")).is_err());
        assert!(matches!(
            open(&dir.0.join("missing.db")),
            Err(SyncError::Source(_))
        ));
    }
}
