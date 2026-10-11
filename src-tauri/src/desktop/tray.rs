use super::{preferences, request_exit, show_window};
use crate::commands::{AppState, SyncStateDto};
use crate::settings::{SettingsPatch, SettingsStore};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Listener, Manager,
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
};

const FREQUENCIES: [u32; 5] = [0, 1, 5, 15, 30];
struct TrayRuntime {
    status: MenuItem<tauri::Wry>,
    sync: MenuItem<tauri::Wry>,
    pause: MenuItem<tauri::Wry>,
    frequencies: Vec<(u32, CheckMenuItem<tauri::Wry>)>,
    cached: Mutex<TrayCache>,
}
struct TrayCache {
    summary: String,
    last_finished_ms: Option<i64>,
    saw_native_sync: bool,
}
impl TrayCache {
    fn observe_sync(&mut self, sync: &SyncStateDto, clearing: bool) {
        if let Some(finished) = sync.finished_at_ms {
            self.last_finished_ms = self.last_finished_ms.max(Some(finished));
            self.saw_native_sync = true;
        } else if sync.running || sync.started_at_ms.is_some() || clearing {
            self.saw_native_sync = true;
        } else if self.saw_native_sync {
            // A cleared native state invalidates both seeded and session-local scan times.
            self.last_finished_ms = None;
            self.saw_native_sync = false;
        }
    }
}
struct Presentation {
    status: String,
    pause: &'static str,
    can_sync: bool,
}
fn tooltip(sync: &SyncStateDto, paused: bool, clearing: bool, summary: &str) -> String {
    let state = if clearing {
        "清除中"
    } else if sync.running && paused {
        "采集中·完成后暂停"
    } else if sync.running {
        "采集中"
    } else if paused {
        "已暂停"
    } else if sync.error.is_some() {
        "上次采集有失败"
    } else {
        "就绪"
    };
    let value = format!("TokLens · {state}\n{summary}");
    if value.encode_utf16().count() <= 127 {
        value
    } else {
        // Never cut off completeness/unknown markers to fit the Windows tooltip limit.
        format!("TokLens · {state}\n今日用量摘要过长，请打开总览查看完整数据与未知字段")
    }
}
fn presentation(
    sync: &SyncStateDto,
    paused: bool,
    clearing: bool,
    last_finished_ms: Option<i64>,
    now_ms: i64,
) -> Presentation {
    let state = if clearing {
        "正在清除统计"
    } else if sync.running && paused {
        "采集中 · 完成后暂停"
    } else if sync.running {
        "采集中"
    } else if paused {
        "采集已暂停"
    } else {
        "采集就绪"
    };
    let last = last_finished_ms.map_or_else(
        || "尚未采集".to_owned(),
        |finished| {
            let minutes = now_ms.saturating_sub(finished).max(0) / 60_000;
            if minutes == 0 {
                "上次采集：刚刚".to_owned()
            } else {
                format!("上次采集：{minutes} 分钟前")
            }
        },
    );
    let failure = if !sync.running && sync.error.is_some() {
        " · 上次采集失败或部分失败"
    } else {
        ""
    };
    Presentation {
        status: format!("{state} · {last}{failure}"),
        pause: if paused {
            "恢复采集"
        } else {
            "暂停采集"
        },
        can_sync: !sync.running && !paused && !clearing,
    }
}

/// Refresh from authoritative native state, never from frontend event payloads.
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(runtime) = handle.try_state::<TrayRuntime>() else {
            return;
        };
        let Some(state) = handle.try_state::<AppState>() else {
            return;
        };
        let Ok(settings) = handle.state::<SettingsStore>().snapshot() else {
            return;
        };
        let sync = state.sync_state();
        let clearing = state.activity.is_clearing();
        // No native GUI calls while the cache, settings or synchronization locks are held.
        let (summary, last_finished_ms) = {
            let mut cached = runtime.cached.lock().unwrap_or_else(|e| e.into_inner());
            cached.observe_sync(&sync, clearing);
            (cached.summary.clone(), cached.last_finished_ms)
        };
        let view = presentation(
            &sync,
            settings.settings.collection_paused,
            clearing,
            last_finished_ms,
            crate::usage::time::now_ms(),
        );
        let _ = runtime.status.set_text(&view.status);
        let _ = runtime.sync.set_enabled(view.can_sync);
        let _ = runtime.pause.set_text(view.pause);
        let _ = runtime.pause.set_enabled(!clearing);
        for (minutes, item) in &runtime.frequencies {
            let _ = item.set_checked(*minutes == settings.settings.sync_interval_minutes);
        }
        if let Some(tray) = handle.tray_by_id("toklens") {
            let _ = tray.set_tooltip(Some(tooltip(
                &sync,
                settings.settings.collection_paused,
                clearing,
                &summary,
            )));
        }
    });
}
pub fn update_usage_summary(app: &AppHandle, summary: String) {
    if let Some(runtime) = app.try_state::<TrayRuntime>() {
        runtime
            .cached
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .summary = summary;
    }
    refresh(app);
}
/// Seed from a worker's latest-generation source metadata query, never from the GUI thread.
pub fn update_last_scan(app: &AppHandle, last_scan_ms: Option<i64>) {
    if let Some(runtime) = app.try_state::<TrayRuntime>() {
        let native = app.state::<AppState>().sync_state();
        let mut cached = runtime.cached.lock().unwrap_or_else(|e| e.into_inner());
        cached.last_finished_ms = if last_scan_ms.is_some() {
            cached
                .last_finished_ms
                .max(last_scan_ms)
                .max(native.finished_at_ms)
        } else {
            native.finished_at_ms
        };
    }
    refresh(app);
}
fn report_error(app: &AppHandle, error: crate::commands::CommandError) {
    show_window(app);
    let _ = app.emit("desktop-error", error.message);
    refresh(app);
}
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "采集就绪 · 尚未采集", false, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "立即采集", false, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "暂停采集", true, None::<&str>)?;
    let frequencies = FREQUENCIES
        .iter()
        .map(|minutes| {
            let label = if *minutes == 0 {
                "关闭自动采集".to_owned()
            } else {
                format!("每 {minutes} 分钟")
            };
            CheckMenuItem::with_id(
                app,
                format!("frequency-{minutes}"),
                label,
                true,
                false,
                None::<&str>,
            )
            .map(|item| (*minutes, item))
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let frequency_items = frequencies
        .iter()
        .map(|(_, item)| item as &dyn IsMenuItem<tauri::Wry>)
        .collect::<Vec<_>>();
    let frequency = Submenu::with_items(app, "采集频率", true, &frequency_items)?;
    let pages = [
        ("overview", "总览"),
        ("analytics", "用量分析"),
        ("sessions", "会话明细"),
        ("sources", "数据源管理"),
    ]
    .into_iter()
    .map(|(id, label)| MenuItem::with_id(app, id, label, true, None::<&str>))
    .collect::<tauri::Result<Vec<_>>>()?;
    let settings = MenuItem::with_id(app, "settings", "打开设置", true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "检查更新", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let control_separator = PredefinedMenuItem::separator(app)?;
    let page_separator = PredefinedMenuItem::separator(app)?;
    let settings_separator = PredefinedMenuItem::separator(app)?;
    let exit_separator = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn IsMenuItem<tauri::Wry>> = vec![
        &show,
        &status,
        &control_separator,
        &sync,
        &pause,
        &frequency,
        &page_separator,
    ];
    items.extend(pages.iter().map(|item| item as &dyn IsMenuItem<tauri::Wry>));
    items.extend([
        &settings_separator as &dyn IsMenuItem<tauri::Wry>,
        &settings,
        &updates,
        &exit_separator,
        &quit,
    ]);
    let menu = Menu::with_items(app, &items)?;
    let mut tray = TrayIconBuilder::with_id("toklens")
        .tooltip("TokLens · 今日用量加载中")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "settings" => {
                show_window(app);
                let _ = app.emit("navigate-settings", ());
            }
            "updates" => {
                show_window(app);
                let _ = app.emit("navigate-updates", ());
            }
            page @ ("overview" | "analytics" | "sessions" | "sources") => {
                show_window(app);
                let _ = app.emit("navigate-page", page);
            }
            "pause" => {
                let result = app
                    .state::<SettingsStore>()
                    .snapshot()
                    .map_err(crate::commands::CommandError::new)
                    .and_then(|snapshot| {
                        preferences::update(
                            app,
                            SettingsPatch {
                                collection_paused: Some(!snapshot.settings.collection_paused),
                                ..Default::default()
                            },
                        )
                    });
                if let Err(error) = result {
                    report_error(app, error);
                }
            }
            "sync" => {
                if let Err(error) = crate::commands::request_sync(app, false) {
                    report_error(app, error);
                }
            }
            "quit" => request_exit(app),
            id => {
                if let Some(minutes) = id
                    .strip_prefix("frequency-")
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|minutes| FREQUENCIES.contains(minutes))
                {
                    if let Err(error) = preferences::update(
                        app,
                        SettingsPatch {
                            sync_interval_minutes: Some(minutes),
                            ..Default::default()
                        },
                    ) {
                        report_error(app, error);
                    }
                    // Native check items toggle themselves; restore the single persisted choice.
                    refresh(app);
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    #[cfg(target_os = "windows")]
    {
        tray = tray.on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    button_state: tauri::tray::MouseButtonState::Up,
                    ..
                }
            ) {
                show_window(tray.app_handle());
            }
        });
    }
    tray.build(app)?;
    app.manage(TrayRuntime {
        status,
        sync,
        pause,
        frequencies,
        cached: Mutex::new(TrayCache {
            summary: "今日用量加载中".into(),
            last_finished_ms: None,
            saw_native_sync: false,
        }),
    });
    for event in ["settings-changed", "usage-sync"] {
        let handle = app.clone();
        app.listen(event, move |_| refresh(&handle));
    }
    refresh(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeded_time_survives_restart_and_newer_native_time_wins_until_clear() {
        let mut cached = TrayCache {
            summary: "synthetic".into(),
            last_finished_ms: Some(100),
            saw_native_sync: false,
        };
        cached.observe_sync(&SyncStateDto::default(), false);
        assert_eq!(cached.last_finished_ms, Some(100));
        let completed = SyncStateDto {
            finished_at_ms: Some(200),
            ..Default::default()
        };
        cached.observe_sync(&completed, false);
        assert_eq!(cached.last_finished_ms, Some(200));
        let running = SyncStateDto {
            running: true,
            ..Default::default()
        };
        cached.observe_sync(&running, false);
        assert_eq!(cached.last_finished_ms, Some(200));
        cached.observe_sync(&SyncStateDto::default(), false);
        assert_eq!(cached.last_finished_ms, None);
        cached.last_finished_ms = Some(300);
        cached.observe_sync(&SyncStateDto::default(), true);
        cached.observe_sync(&SyncStateDto::default(), false);
        assert_eq!(cached.last_finished_ms, None);
    }
    #[test]
    fn running_and_paused_explains_that_current_scan_continues() {
        let sync = SyncStateDto {
            running: true,
            ..Default::default()
        };
        let view = presentation(&sync, true, false, Some(60_000), 180_000);
        assert!(view.status.contains("采集中 · 完成后暂停"));
        assert!(view.status.contains("2 分钟前"));
        assert!(!view.can_sync);
        assert_eq!(view.pause, "恢复采集");
    }
    #[test]
    fn idle_paused_clearing_and_failed_states_are_visible() {
        let sync = SyncStateDto::default();
        assert!(presentation(&sync, false, false, None, 0).can_sync);
        assert!(
            presentation(&sync, false, false, None, 0)
                .status
                .contains("尚未采集")
        );
        assert!(!presentation(&sync, true, false, None, 0).can_sync);
        let clearing = presentation(&sync, false, true, None, 0);
        assert!(!clearing.can_sync);
        assert!(clearing.status.contains("正在清除统计"));
        let failed = SyncStateDto {
            error: Some("synthetic failure".into()),
            ..Default::default()
        };
        assert!(
            presentation(&failed, false, false, Some(0), 0)
                .status
                .contains("失败或部分失败")
        );
    }
    #[test]
    fn frequency_choices_match_settings_validation() {
        for minutes in FREQUENCIES {
            let settings = crate::settings::AppSettings::default()
                .patched(SettingsPatch {
                    sync_interval_minutes: Some(minutes),
                    ..Default::default()
                })
                .expect("supported persisted frequency");
            assert_eq!(
                FREQUENCIES
                    .iter()
                    .filter(|value| **value == settings.sync_interval_minutes)
                    .count(),
                1
            );
        }
    }
    #[test]
    fn tooltip_keeps_unknown_markers_and_fits_windows_limit() {
        let sync = SyncStateDto::default();
        let summary = "今日用量：已知 Token 10；2 条总量未知";
        assert!(tooltip(&sync, false, false, summary).contains(summary));
        let long = tooltip(&sync, false, false, &"😀".repeat(100));
        assert!(long.encode_utf16().count() <= 127);
        assert!(long.contains("未知字段"));
        assert!(!long.contains('😀'));
    }
}
