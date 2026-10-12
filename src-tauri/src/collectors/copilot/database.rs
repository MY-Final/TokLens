//! Filesystem reads only: SQLite never opens the original database or its shm.
use crate::collectors::{SyncError, files::fingerprint};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn invalid() -> SyncError {
    SyncError::Source(std::io::Error::from(std::io::ErrorKind::InvalidData))
}
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded WAL word"),
    )
}
fn checksum(bytes: &[u8], little: bool, mut sum: (u32, u32)) -> (u32, u32) {
    for pair in bytes.chunks_exact(8) {
        let read = |offset| {
            let b = pair[offset..offset + 4].try_into().expect("checksum pair");
            if little {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        };
        sum.0 = sum.0.wrapping_add(read(0)).wrapping_add(sum.1);
        sum.1 = sum.1.wrapping_add(read(4)).wrapping_add(sum.0);
    }
    sum
}
fn wal_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push("-wal");
    PathBuf::from(name)
}
fn stamp(path: &Path) -> std::io::Result<Option<(i64, i64)>> {
    match fingerprint(path) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
pub(super) type Signature = ((i64, i64), Option<(i64, i64)>);
pub(super) fn signature(path: &Path) -> std::io::Result<Signature> {
    Ok((fingerprint(path)?, stamp(&wal_path(path))?))
}

pub(super) fn open(path: &Path) -> Result<Connection, SyncError> {
    let wal = wal_path(path);
    let before = signature(path)?;
    let mut bytes = std::fs::read(path)?;
    let log = if before.1.is_some() {
        std::fs::read(&wal)?
    } else {
        Vec::new()
    };
    if before != signature(path)? {
        return Err(SyncError::ChangedDuringScan);
    }
    overlay(&mut bytes, &log)?;
    // A deserialized database cannot access WAL. The overlaid image is self-contained.
    bytes[18] = 1;
    bytes[19] = 1;
    let mut conn = Connection::open_in_memory()?;
    conn.deserialize_read_exact("main", bytes.as_slice(), bytes.len(), true)?;
    conn.pragma_update(None, "query_only", true)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(conn)
}

fn overlay(main: &mut Vec<u8>, wal: &[u8]) -> Result<(), SyncError> {
    if main.len() < 100 || &main[..16] != b"SQLite format 3\0" {
        return Err(invalid());
    }
    if wal.is_empty() {
        return Ok(());
    }
    if wal.len() < 32 {
        return Err(SyncError::ChangedDuringScan);
    }
    let magic = word(wal, 0);
    if !matches!(magic, 0x377f0682 | 0x377f0683) || word(wal, 4) != 3007000 {
        return Err(invalid());
    }
    let page = word(wal, 8) as usize;
    let main_page = u16::from_be_bytes([main[16], main[17]]) as usize;
    let main_page = if main_page == 1 { 65536 } else { main_page };
    if page != main_page || !(512..=65536).contains(&page) || !page.is_power_of_two() {
        return Err(invalid());
    }
    let little = magic == 0x377f0682;
    let mut sum = checksum(&wal[..24], little, (0, 0));
    if sum != (word(wal, 24), word(wal, 28)) {
        return Err(invalid());
    }
    let mut frames = Vec::new();
    let mut commit = None;
    for frame in wal[32..].chunks_exact(page + 24) {
        // SQLite may retain old frames beyond the latest transaction after a WAL reset.
        if frame[8..16] != wal[16..24] {
            break;
        }
        sum = checksum(&frame[..8], little, sum);
        sum = checksum(&frame[24..], little, sum);
        if sum != (word(frame, 16), word(frame, 20)) {
            break;
        }
        let number = word(frame, 0) as usize;
        if number == 0 {
            return Err(invalid());
        }
        frames.push((number, &frame[24..]));
        let pages = word(frame, 4) as usize;
        if pages != 0 {
            commit = Some((frames.len(), pages));
        }
    }
    if let Some((count, pages)) = commit {
        let size = pages.checked_mul(page).ok_or_else(invalid)?;
        // Bound memory by source size; invalid headers must not trigger huge allocations.
        if size > main.len().saturating_add(wal.len()) {
            return Err(invalid());
        }
        if size > main.len() {
            main.resize(size, 0);
        }
        for &(number, content) in &frames[..count] {
            if number <= pages {
                main[(number - 1) * page..number * page].copy_from_slice(content);
            }
        }
        main.truncate(size);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::test_support::TempDir;

    #[test]
    fn snapshot_excludes_uncommitted_changes_and_rejects_invalid_header() {
        let dir = TempDir::new();
        let path = dir.0.join("source.db");
        let writer = Connection::open(&path).expect("fixture");
        writer
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA cache_size=1;
            CREATE TABLE counters(value INTEGER, payload BLOB);
            INSERT INTO counters VALUES(1,NULL);
            BEGIN IMMEDIATE; INSERT INTO counters VALUES(2,zeroblob(65536));",
            )
            .expect("uncommitted fixture");
        let count = |conn: Connection| {
            conn.query_row("SELECT COUNT(*) FROM counters", [], |r| r.get::<_, i64>(0))
                .expect("count")
        };
        assert_eq!(count(open(&path).expect("snapshot")), 1);
        writer.execute_batch("COMMIT").expect("commit");
        assert_eq!(count(open(&path).expect("snapshot")), 2);
        let mut main = std::fs::read(&path).expect("main");
        let mut wal = std::fs::read(wal_path(&path)).expect("wal");
        wal[24] ^= 1;
        assert!(overlay(&mut main, &wal).is_err());
    }
}
