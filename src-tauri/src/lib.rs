pub mod collectors;
pub mod commands;
pub mod desktop;
pub mod settings;
pub mod usage;

use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
struct AppInfo {
    name: String,
    version: String,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "TokLens".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--autostart") {
                desktop::show_window(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--autostart")
                .build(),
        )
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(desktop::WINDOW_FLAGS)
                .skip_initial_state("main")
                .build(),
        )
        .on_window_event(desktop::on_window_event)
        .setup(|app| {
            // 本地库放在 TokLens 专属应用数据目录（客户端数据目录只读，绝不写入）。
            let data_dir = app.path().app_data_dir()?;
            let state = commands::AppState::initialize(&data_dir)?;
            app.manage(state);
            app.manage(settings::SettingsStore::load(&data_dir));
            desktop::initialize(app.handle())?;
            // 启动即做一次增量同步：首次运行导入历史，之后只处理变化的文件。
            commands::request_sync(app.handle(), false)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            commands::settings::get_settings,
            commands::settings::update_settings,
            desktop::get_desktop_status,
            desktop::get_close_pending,
            desktop::resolve_close,
            commands::queries::get_overview_stats,
            commands::queries::get_daily_token_trend,
            commands::queries::get_client_usage,
            commands::queries::get_rankings,
            commands::queries::get_sessions,
            commands::queries::get_data_source_status,
            commands::queries::sync_codex,
            commands::queries::sync_usage,
        ])
        .build(tauri::generate_context!())
        .expect("TokLens cannot initialize the desktop runtime")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { api, .. }
                if !app
                    .state::<desktop::DesktopState>()
                    .allow_exit
                    .load(std::sync::atomic::Ordering::SeqCst) =>
            {
                api.prevent_exit();
                desktop::request_exit(app);
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => desktop::show_window(app),
            _ => {}
        });
}
