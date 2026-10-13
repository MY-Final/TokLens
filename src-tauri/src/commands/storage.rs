//! Application-owned storage selection and transactional relocation.
use super::{AppState, CommandError};
use crate::{
    collectors::CollectorRoots,
    settings::SettingsStore,
    usage::{
        client::Client,
        store::{UsageError, UsageStore},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter, Manager};

const DATABASE: &str = "toklens-usage.sqlite3";
const LOCATOR: &str = "storage-location.json";
const OWN_FILES: [&str; 6] = [
    DATABASE,
    "toklens-usage.sqlite3-wal",
    "toklens-usage.sqlite3-shm",
    "toklens-usage.sqlite3-journal",
    "settings.json",
    "settings.json.tmp",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    directory: String,
    database_path: String,
    locator_path: String,
    warning: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Location {
    schema_version: u32,
    directory: PathBuf,
}

pub(crate) fn resolve_directory(
    system: &Path,
    roots: &CollectorRoots,
) -> Result<PathBuf, UsageError> {
    let path = system.join(LOCATOR);
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(system.to_path_buf());
        }
        Err(error) => return Err(error.into()),
    };
    let location: Location = serde_json::from_slice(&bytes).map_err(|_| {
        UsageError::InvalidInput("存储定位文件损坏，请恢复配置的存储目录后重新启动 TokLens".into())
    })?;
    if location.schema_version != 1 || !location.directory.is_absolute() {
        return Err(UsageError::InvalidInput(
            "存储定位文件的版本或路径无效".into(),
        ));
    }
    let directory = std::fs::canonicalize(location.directory)?;
    if !directory.is_dir() || !directory.join(DATABASE).is_file() {
        return Err(UsageError::InvalidInput(
            "配置的存储目录不可用，请恢复该目录后重新启动 TokLens".into(),
        ));
    }
    std::fs::read_dir(&directory)?;
    validate_target(&directory, roots).map_err(UsageError::InvalidInput)?;
    Ok(directory)
}

pub(super) fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}
fn status(state: &AppState, warning: Option<String>) -> StorageStatus {
    let path = state.database_path();
    StorageStatus {
        directory: display_path(path.parent().unwrap_or(Path::new(""))),
        database_path: display_path(&path),
        locator_path: display_path(&state.system_directory.join(LOCATOR)),
        warning,
    }
}
#[tauri::command]
pub fn get_storage_status(state: tauri::State<'_, AppState>) -> StorageStatus {
    status(&state, None)
}

// Resolve existing ancestors as well as junctions when a destination does not exist yet.
fn resolved(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("存储目录无效"))?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("存储目录无效"))?;
    Ok(resolved(parent)?.join(name))
}
fn overlaps(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        let left = left
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>();
        let right = right
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>();
        left.starts_with(&right) || right.starts_with(&left)
    }
    #[cfg(not(windows))]
    {
        left.starts_with(right) || right.starts_with(left)
    }
}
pub(super) fn validate_target(target: &Path, roots: &CollectorRoots) -> Result<PathBuf, String> {
    if !target.is_absolute()
        || target
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("请选择不包含上级路径跳转的绝对目录".into());
    }
    let target = resolved(target).map_err(|_| "无法解析存储目录")?;
    for client in Client::ALL {
        for (index, source) in roots.source_locations(client).into_iter().enumerate() {
            // Database-backed clients own the containing directory too.
            let source = if source.path.is_file()
                || matches!(client, Client::OpenCode | Client::MimoCode | Client::Mcode)
                || (client == Client::Copilot && roots.copilot.is_some() && index == 0)
            {
                source.path.parent().unwrap_or(&source.path)
            } else {
                &source.path
            };
            let source = resolved(source).map_err(|_| "无法验证客户端数据目录")?;
            if overlaps(&target, &source) {
                return Err("存储目录与客户端原始数据目录重叠，请选择其他目录".into());
            }
        }
    }
    Ok(target)
}
fn save_locator(system: &Path, directory: &Path) -> Result<(), String> {
    let temporary = system.join("storage-location.json.tmp");
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let bytes = serde_json::to_vec_pretty(&Location {
            schema_version: 1,
            directory: directory.to_path_buf(),
        })?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, system.join(LOCATOR))?;
        Ok(())
    })();
    result.map_err(|_| "无法保存存储位置，原存储仍保持有效".into())
}
fn remove_owned(directory: &Path) -> Option<String> {
    let mut failed = false;
    for name in OWN_FILES {
        if let Err(error) = std::fs::remove_file(directory.join(name)) {
            failed |= error.kind() != std::io::ErrorKind::NotFound;
        }
    }
    failed.then(|| "存储迁移成功，但部分旧 TokLens 文件未能删除，请手动检查旧目录".into())
}

pub(crate) fn relocate(
    state: &AppState,
    settings: &SettingsStore,
    target: &Path,
    roots: &CollectorRoots,
) -> Result<StorageStatus, String> {
    let _maintenance = state.activity.begin_clear()?;
    let target = validate_target(target, roots)?;
    let mut source = state.store().map_err(|error| error.to_string())?;
    let old = source
        .database_path()
        .parent()
        .ok_or("当前存储目录无效")?
        .to_path_buf();
    if std::fs::canonicalize(&old).map_err(|_| "无法解析当前存储目录")? == target {
        drop(source);
        return Ok(status(state, None));
    }
    std::fs::create_dir_all(&target).map_err(|_| "无法创建存储目录")?;
    for name in OWN_FILES
        .into_iter()
        .chain([LOCATOR, "storage-location.json.tmp"])
    {
        // The system directory's locator belongs to this app and is expected when moving back.
        if name == LOCATOR
            && target
                == std::fs::canonicalize(&state.system_directory)
                    .map_err(|_| "无法解析定位文件目录")?
        {
            continue;
        }
        if target
            .join(name)
            .try_exists()
            .map_err(|_| "无法检查目标目录")?
        {
            return Err("目标目录已有 TokLens 存储文件，请选择其他目录".into());
        }
    }
    let locator_temporary = state.system_directory.join("storage-location.json.tmp");
    if locator_temporary
        .try_exists()
        .map_err(|_| "无法检查存储定位文件")?
    {
        return Err("存储定位临时文件已存在；请在关闭 TokLens 后备份并移除系统应用目录中的 storage-location.json.tmp，再重试".into());
    }
    // Reserve the SQLite filename exclusively before SQLite writes into the empty file.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target.join(DATABASE))
        .map_err(|_| "无法创建目标数据库文件")?;
    let result = (|| {
        source
            .snapshot_to(&target.join(DATABASE))
            .map_err(|_| "无法创建并验证数据库快照".to_string())?;
        let replacement = UsageStore::open(&target.join(DATABASE))
            .map_err(|_| "无法打开目标数据库".to_string())?;
        settings.relocate(&target, || {
            save_locator(&state.system_directory, &target)?;
            let previous = std::mem::replace(&mut *source, replacement);
            drop(previous);
            Ok(())
        })
    })();
    if let Err(error) = result {
        remove_owned(&target);
        let _ = std::fs::remove_file(locator_temporary);
        return Err(error);
    }
    let warning = remove_owned(&old);
    drop(source);
    Ok(status(state, warning))
}
#[tauri::command]
pub async fn relocate_storage(
    app: AppHandle,
    directory: String,
) -> Result<StorageStatus, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = relocate(
            &app.state::<AppState>(),
            &app.state::<SettingsStore>(),
            Path::new(&directory),
            &CollectorRoots::from_environment(),
        )
        .map_err(CommandError::new)?;
        let _ = app.emit("settings-changed", ());
        let _ = app.emit(super::SYNC_EVENT, app.state::<AppState>().sync_state());
        Ok(result)
    })
    .await
    .map_err(|_| CommandError::new("存储迁移任务未能完成"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "toklens-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("directory");
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn move_to(state: &AppState, settings: &SettingsStore, directory: &Path) -> StorageStatus {
        relocate(state, settings, directory, &CollectorRoots::default()).expect("move")
    }
    #[test]
    fn migration_preserves_wal_metadata_watermarks_settings_and_restart() {
        let root = Directory::new();
        let system = root.0.join("system");
        let state =
            AppState::initialize_with_roots(&system, &CollectorRoots::default()).expect("state");
        let settings = SettingsStore::load(&system);
        settings
            .update(crate::settings::SettingsPatch {
                collection_paused: Some(true),
                ..Default::default()
            })
            .expect("update");
        let writer = rusqlite::Connection::open(state.database_path()).expect("writer");
        writer.execute_batch("PRAGMA journal_mode=WAL; INSERT INTO source_files(client,path,kind,parser_version,content_offset) VALUES('codex','synthetic.jsonl','jsonl',1,42);
          INSERT INTO sessions(id,client,client_session_id,semantics_id,total_basis,message_coverage,source_kind,cost_source,imported_at_ms,updated_at_ms)
          VALUES('s','codex','s','codex_v1','sum_of_messages','complete','measured','none',1,1);").expect("records");
        drop(writer);
        let first = root.0.join("first");
        move_to(&state, &settings, &first);
        let reader = rusqlite::Connection::open(state.database_path()).expect("reader");
        assert_eq!(
            reader
                .query_row("SELECT content_offset FROM source_files", [], |row| row
                    .get::<_, i64>(0))
                .expect("watermark"),
            42
        );
        assert_eq!(
            reader
                .query_row("SELECT count(*) FROM sessions", [], |row| row
                    .get::<_, i64>(0))
                .expect("sessions"),
            1
        );
        drop(reader);
        assert!(!system.join(DATABASE).exists());
        assert!(system.join(LOCATOR).exists());
        let second = root.0.join("second");
        move_to(&state, &settings, &second);
        settings
            .update(crate::settings::SettingsPatch {
                collection_paused: Some(false),
                ..Default::default()
            })
            .expect("new location update");
        assert!(
            !SettingsStore::load(&second)
                .snapshot()
                .expect("reload")
                .settings
                .collection_paused
        );
        settings
            .update(crate::settings::AppSettings::default().into())
            .expect("reset");
        assert_eq!(
            resolve_directory(&system, &CollectorRoots::default()).expect("location"),
            std::fs::canonicalize(&second).expect("canonical")
        );
        let restarted_moved = AppState::initialize_with_roots(&system, &CollectorRoots::default())
            .expect("restart at custom location");
        assert_eq!(
            restarted_moved.database_path(),
            std::fs::canonicalize(&second)
                .expect("canonical")
                .join(DATABASE)
        );
        let moved_reader =
            rusqlite::Connection::open(restarted_moved.database_path()).expect("restart reader");
        assert_eq!(
            moved_reader
                .query_row("SELECT content_offset FROM source_files", [], |row| row
                    .get::<_, i64>(0))
                .expect("restart watermark"),
            42
        );
        drop(moved_reader);
        drop(restarted_moved);
        move_to(&state, &settings, &second);
        move_to(&state, &settings, &system);
        drop(state);
        let restarted =
            AppState::initialize_with_roots(&system, &CollectorRoots::default()).expect("restart");
        assert_eq!(
            restarted
                .store()
                .expect("store")
                .schema_version()
                .expect("schema"),
            1
        );
        drop(restarted);
    }
    #[test]
    fn rejects_collisions_sources_and_relative_paths_without_changing_active_storage() {
        let root = Directory::new();
        let system = root.0.join("system");
        let state =
            AppState::initialize_with_roots(&system, &CollectorRoots::default()).expect("state");
        let settings = SettingsStore::load(&system);
        let target = root.0.join("target");
        std::fs::create_dir(&target).expect("target");
        std::fs::write(target.join("settings.json"), "unrelated").expect("collision");
        assert!(relocate(&state, &settings, &target, &CollectorRoots::default()).is_err());
        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).expect("preserved"),
            "unrelated"
        );
        assert!(
            relocate(
                &state,
                &settings,
                Path::new("relative"),
                &CollectorRoots::default()
            )
            .is_err()
        );
        let roots = CollectorRoots {
            codex: Some(root.0.join("client")),
            ..Default::default()
        };
        assert!(relocate(&state, &settings, &root.0.join("client/child"), &roots).is_err());
        assert!(!root.0.join("client").exists());
        assert!(!system.join(LOCATOR).exists());
        let relative_source = PathBuf::from(format!(
            "toklens-synthetic-missing-client-{}/database",
            std::process::id()
        ));
        let copilot_roots = CollectorRoots {
            copilot: Some(relative_source.clone()),
            ..Default::default()
        };
        let sibling = std::env::current_dir()
            .expect("cwd")
            .join(relative_source.parent().expect("parent"))
            .join("statistics");
        assert!(validate_target(&sibling, &copilot_roots).is_err());
    }
    #[test]
    fn malformed_unknown_version_and_missing_database_locators_fail_closed() {
        let root = Directory::new();
        let system = root.0.join("system");
        std::fs::create_dir(&system).expect("system");
        let target = root.0.join("missing-database");
        std::fs::create_dir(&target).expect("target");
        for bytes in [
            b"invalid".to_vec(),
            serde_json::to_vec(&Location {
                schema_version: 2,
                directory: target.clone(),
            })
            .expect("version fixture"),
            serde_json::to_vec(&Location {
                schema_version: 1,
                directory: target.clone(),
            })
            .expect("missing database fixture"),
        ] {
            std::fs::write(system.join(LOCATOR), bytes).expect("locator fixture");
            assert!(AppState::initialize_with_roots(&system, &CollectorRoots::default()).is_err());
            assert!(!system.join(DATABASE).exists());
            assert!(!target.join(DATABASE).exists());
        }
    }

    #[test]
    fn pointer_commit_failure_rolls_back_and_missing_configured_storage_fails_closed() {
        let root = Directory::new();
        let system = root.0.join("system");
        let state =
            AppState::initialize_with_roots(&system, &CollectorRoots::default()).expect("state");
        let settings = SettingsStore::load(&system);
        std::fs::create_dir(system.join(LOCATOR)).expect("force rename failure");
        let target = root.0.join("target");
        assert!(relocate(&state, &settings, &target, &CollectorRoots::default()).is_err());
        assert!(
            std::fs::canonicalize(state.database_path())
                .expect("active database path")
                .starts_with(std::fs::canonicalize(&system).expect("original storage directory"))
        );
        assert!(!target.join(DATABASE).exists());
        assert!(!target.join("settings.json").exists());
        settings
            .update(crate::settings::SettingsPatch {
                collection_paused: Some(true),
                ..Default::default()
            })
            .expect("old settings remain writable");
        assert!(system.join("settings.json").exists());
        std::fs::remove_dir(system.join(LOCATOR)).expect("remove injection");
        save_locator(&system, &root.0.join("missing")).expect("locator");
        assert!(AppState::initialize_with_roots(&system, &CollectorRoots::default()).is_err());
    }
}
