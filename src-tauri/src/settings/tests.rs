use super::*;
use crate::usage::client::Client;
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
    assert!(!snapshot.settings.sidebar_collapsed);
    assert_eq!(snapshot.settings.default_range, "today");
    assert_eq!(snapshot.settings.token_display, model::TokenDisplay::Auto);
    assert_eq!(snapshot.settings.sync_interval_minutes, 5);
    assert_eq!(snapshot.settings.clients().len(), Client::ALL.len());
}

#[test]
fn token_display_preferences_persist_and_reset_without_changing_other_settings() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    for display in ["auto", "chinese", "k", "m", "b", "exact"] {
        let patch: SettingsPatch = serde_json::from_value(serde_json::json!({
            "tokenDisplay": display,
        }))
        .expect("synthetic display patch");
        store.update(patch).expect("save display preference");
        let settings = SettingsStore::load(&directory.0)
            .snapshot()
            .expect("reload display preference")
            .settings;
        assert_eq!(
            serde_json::to_value(&settings).expect("serialize")["tokenDisplay"],
            display
        );
        assert_eq!(settings.default_range, "today");
        assert!(!settings.automatic_updates);
    }
    assert!(
        serde_json::from_value::<SettingsPatch>(serde_json::json!({
            "tokenDisplay": "unsupported",
        }))
        .is_err()
    );
    store
        .update(AppSettings::default().into())
        .expect("reset preferences");
    assert_eq!(
        SettingsStore::load(&directory.0)
            .snapshot()
            .expect("reset")
            .settings
            .token_display,
        model::TokenDisplay::Auto
    );
}

#[test]
fn existing_ranges_survive_missing_sidebar_preference() {
    for range in ["7d", "30d"] {
        let directory = Directory::new();
        std::fs::write(
            directory.0.join("settings.json"),
            format!(r#"{{"schemaVersion":1,"settings":{{"defaultRange":"{range}"}}}}"#),
        )
        .expect("synthetic old settings");
        let snapshot = SettingsStore::load(&directory.0)
            .snapshot()
            .expect("settings");
        assert_eq!(snapshot.settings.default_range, range);
        assert!(!snapshot.settings.sidebar_collapsed);
        assert!(snapshot.warning.is_none());
    }
}

#[test]
fn sidebar_persists_and_reset_matches_new_install_defaults() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    assert_eq!(
        store.snapshot().expect("new settings").settings,
        AppSettings::default()
    );
    store.record_update_check(123).expect("timestamp");
    store
        .update(SettingsPatch {
            sidebar_collapsed: Some(true),
            default_range: Some("7d".into()),
            ..Default::default()
        })
        .expect("save preferences");
    let reloaded = SettingsStore::load(&directory.0);
    let settings = reloaded.snapshot().expect("persisted settings").settings;
    assert!(settings.sidebar_collapsed);
    assert_eq!(settings.default_range, "7d");
    reloaded
        .update(AppSettings::default().into())
        .expect("reset defaults");
    let reset = SettingsStore::load(&directory.0);
    assert_eq!(
        reset.snapshot().expect("reset settings").settings,
        AppSettings::default()
    );
    assert_eq!(
        reset.last_update_check().expect("timestamp preserved"),
        Some(123)
    );
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

#[test]
fn schema_v1_document_migrates_to_current_without_losing_settings_or_panels() {
    let directory = Directory::new();
    std::fs::write(
        directory.0.join("settings.json"),
        r#"{"schemaVersion":1,"settings":{"theme":"dark"}}"#,
    )
    .expect("fixture");
    let store = SettingsStore::load(&directory.0);
    let snapshot = store.snapshot().expect("settings");
    assert!(snapshot.warning.is_none());
    assert_eq!(snapshot.settings.theme, model::Theme::Dark);
    assert!(store.panels().expect("panels").is_empty());
    assert_eq!(store.llm().expect("llm"), LlmConfig::default());

    // 保存后写回 SCHEMA_VERSION 且补齐 panels 与 llm。
    store
        .update(SettingsPatch {
            collection_paused: Some(true),
            ..Default::default()
        })
        .expect("save");
    let value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(directory.0.join("settings.json")).expect("read"),
    )
    .expect("json");
    assert_eq!(value["schemaVersion"], serde_json::json!(SCHEMA_VERSION));
    assert_eq!(value["panels"], serde_json::json!([]));
    assert_eq!(value["llm"], serde_json::json!({}));
    assert_eq!(value["settings"]["theme"], serde_json::json!("dark"));
}

fn migrated(value: serde_json::Value) -> serde_json::Value {
    migrate_document(value).expect("migration")
}

#[test]
fn migrate_document_is_a_pure_function_with_explicit_steps() {
    // 2 → 3：显式补齐 llm 键，并标记当前版本；既有字段原样保留。
    let v2 = migrated(serde_json::json!({
        "schemaVersion": 2,
        "settings": { "theme": "dark" },
        "panels": [{ "specVersion": 1, "id": "p1" }],
    }));
    assert_eq!(v2["schemaVersion"], serde_json::json!(SCHEMA_VERSION));
    assert_eq!(v2["llm"], serde_json::json!({}));
    assert_eq!(v2["settings"]["theme"], serde_json::json!("dark"));
    assert_eq!(v2["panels"][0]["id"], serde_json::json!("p1"));

    // 1 → 3 一步到位；缺失 schemaVersion 视为当前版本。
    let v1 = migrated(serde_json::json!({ "schemaVersion": 1, "settings": {} }));
    assert_eq!(v1["schemaVersion"], serde_json::json!(SCHEMA_VERSION));
    assert_eq!(v1["llm"], serde_json::json!({}));

    let missing = migrated(serde_json::json!({ "settings": {} }));
    assert_eq!(missing["schemaVersion"], serde_json::json!(SCHEMA_VERSION));

    // 已含 llm 的当前版本不覆盖用户值。
    let current = migrated(serde_json::json!({
        "schemaVersion": SCHEMA_VERSION,
        "llm": { "baseUrl": "https://api.example.com", "model": "m" },
    }));
    assert_eq!(current["llm"]["model"], serde_json::json!("m"));

    // 高于当前版本：拒绝，调用方回退默认并提示（不静默丢弃）。
    assert!(migrate_document(serde_json::json!({ "schemaVersion": SCHEMA_VERSION + 1 })).is_err());
    assert!(migrate_document(serde_json::json!({ "schemaVersion": -1 })).is_err());
    assert!(migrate_document(serde_json::json!("not an object")).is_err());
}

#[test]
fn llm_config_persists_and_resets_without_changing_other_settings() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    let saved = store
        .transform_llm(|llm| {
            llm.base_url = Some("https://api.deepseek.com".into());
            llm.model = Some("deepseek-chat".into());
            Ok(())
        })
        .expect("save llm");
    assert_eq!(saved.base_url.as_deref(), Some("https://api.deepseek.com"));

    let reloaded = SettingsStore::load(&directory.0);
    let llm = reloaded.llm().expect("llm");
    assert_eq!(llm.model.as_deref(), Some("deepseek-chat"));
    // 非机密：设置文件里有 baseUrl/model，但绝无 key 字段。
    let raw = std::fs::read_to_string(directory.0.join("settings.json")).expect("read");
    assert!(raw.contains("deepseek-chat"));
    assert!(!raw.contains("apiKey") && !raw.contains("api_key"));

    // 恢复默认偏好不触碰 llm（llm 独立于 AppSettings）。
    reloaded
        .update(AppSettings::default().into())
        .expect("reset preferences");
    assert_eq!(
        SettingsStore::load(&directory.0).llm().expect("llm"),
        LlmConfig {
            base_url: Some("https://api.deepseek.com".into()),
            model: Some("deepseek-chat".into()),
        }
    );
}

#[test]
fn panels_survive_settings_updates_and_reload() {
    let directory = Directory::new();
    let store = SettingsStore::load(&directory.0);
    store
        .transform_panels(|panels| {
            panels.push(serde_json::json!({ "specVersion": 1, "id": "p1" }));
            Ok(())
        })
        .expect("save panel");
    store
        .update(SettingsPatch {
            theme: Some(model::Theme::Dark),
            ..Default::default()
        })
        .expect("update settings");
    let reloaded = SettingsStore::load(&directory.0);
    let panels = reloaded.panels().expect("panels");
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0]["id"], serde_json::json!("p1"));
    assert_eq!(
        reloaded.snapshot().expect("settings").settings.theme,
        model::Theme::Dark
    );
}

#[test]
fn relocation_serializes_concurrent_preference_updates_and_keeps_new_path() {
    use std::sync::{Arc, mpsc};
    let directory = Directory::new();
    let target = directory.0.join("moved");
    std::fs::create_dir(&target).expect("target");
    let store = Arc::new(SettingsStore::load(&directory.0));
    let worker_store = store.clone();
    let worker_target = target.clone();
    let (locked_sender, locked_receiver) = mpsc::channel();
    let (commit_sender, commit_receiver) = mpsc::channel();
    let relocation = std::thread::spawn(move || {
        worker_store
            .relocate(&worker_target, || {
                locked_sender.send(()).expect("locked");
                commit_receiver.recv().expect("commit");
                Ok(())
            })
            .expect("relocation");
    });
    locked_receiver.recv().expect("lock held");
    let updating_store = store.clone();
    let update = std::thread::spawn(move || {
        updating_store
            .update(SettingsPatch {
                collection_paused: Some(true),
                ..Default::default()
            })
            .expect("update")
    });
    commit_sender.send(()).expect("release");
    relocation.join().expect("relocation thread");
    update.join().expect("update thread");
    assert!(
        SettingsStore::load(&target)
            .snapshot()
            .expect("reload moved settings")
            .settings
            .collection_paused
    );
    assert!(!directory.0.join("settings.json").exists());
}
