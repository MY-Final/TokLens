use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "toklens-settings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("synthetic directory");
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn defaults_preserve_local_first_and_upgrade_missing_fields() {
    let directory = Directory::new();
    std::fs::write(
        directory.0.join("settings.json"),
        r#"{"settings":{"theme":"dark"}}"#,
    )
    .expect("fixture");
    let snapshot = SettingsStore::load(&directory.0)
        .snapshot()
        .expect("settings");
    assert_eq!(snapshot.settings.theme, model::Theme::Dark);
    assert!(!snapshot.settings.automatic_updates);
    assert!(!snapshot.settings.autostart);
    assert_eq!(snapshot.settings.sync_interval_minutes, 5);
    assert_eq!(snapshot.settings.clients().len(), 7);
}

#[test]
fn invalid_document_recovers_with_visible_warning() {
    let directory = Directory::new();
    std::fs::write(directory.0.join("settings.json"), "invalid").expect("fixture");
    let store = SettingsStore::load(&directory.0);
    assert!(store.snapshot().expect("settings").warning.is_some());
    store
        .update(SettingsPatch {
            collection_paused: Some(true),
            ..Default::default()
        })
        .expect("save");
    let snapshot = SettingsStore::load(&directory.0)
        .snapshot()
        .expect("reload");
    assert!(snapshot.settings.collection_paused);
    assert!(snapshot.warning.is_none());
}

#[test]
fn failed_save_or_validation_does_not_change_runtime_preferences() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    assert!(
        store
            .update(SettingsPatch {
                sync_interval_minutes: Some(3),
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        store
            .update(SettingsPatch {
                enabled_clients: Some(vec!["codex".into(), "codex".into()]),
                ..Default::default()
            })
            .is_err()
    );
    std::fs::create_dir(directory.0.join("settings.json.tmp")).expect("block write");
    assert!(
        store
            .update(SettingsPatch {
                collection_paused: Some(true),
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        !store
            .snapshot()
            .expect("settings")
            .settings
            .collection_paused
    );
}

#[test]
fn patches_and_update_timestamp_do_not_overwrite_each_other() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    store.record_update_check(123).expect("timestamp");
    store
        .update(SettingsPatch {
            enabled_clients: Some(vec![]),
            ..Default::default()
        })
        .expect("patch");
    store
        .update(SettingsPatch {
            close_behavior: Some(CloseBehavior::Tray),
            ..Default::default()
        })
        .expect("replace atomically");
    let reloaded = SettingsStore::load(&directory.0);
    assert_eq!(reloaded.last_update_check().expect("timestamp"), Some(123));
    assert!(
        reloaded
            .snapshot()
            .expect("settings")
            .settings
            .clients()
            .is_empty()
    );
}
