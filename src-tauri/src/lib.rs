pub mod collectors;
pub mod commands;
pub mod desktop;
pub mod settings;
pub mod updates;
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
    let context = tauri::generate_context!();
    let product_name = context
        .config()
        .product_name
        .clone()
        .unwrap_or_else(|| "TokLens".into());
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--autostart") {
                desktop::show_window(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name(product_name)
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
            let storage_directory = state
                .database_path()
                .parent()
                .ok_or_else(|| std::io::Error::other("Invalid database directory"))?
                .to_path_buf();
            app.manage(state);
            app.manage(settings::SettingsStore::load(&storage_directory));
            app.manage(updates::UpdateManager::default());
            desktop::initialize(app.handle())?;
            // 启动即做一次增量同步：首次运行导入历史，之后只处理变化的文件。
            let preferences = app
                .state::<settings::SettingsStore>()
                .snapshot()
                .map_err(std::io::Error::other)?
                .settings;
            if preferences.sync_on_start && !preferences.collection_paused {
                commands::request_sync(app.handle(), false)?;
            }
            commands::scheduler::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            updates::check::get_update_state,
            updates::check::check_updates,
            updates::download::download_update,
            updates::install::install_update,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::storage::get_storage_status,
            commands::storage::relocate_storage,
            commands::maintenance::clear_statistics,
            commands::maintenance::reset_settings,
            commands::maintenance::open_app_data_dir,
            desktop::get_desktop_status,
            desktop::get_close_pending,
            desktop::resolve_close,
            commands::tray_usage::refresh_tray_usage,
            commands::queries::get_overview_stats,
            commands::queries::get_daily_token_trend,
            commands::queries::get_client_usage,
            commands::queries::get_rankings,
            commands::queries::get_sessions,
            commands::request_queries::get_request_activity,
            commands::request_queries::get_request_analysis,
            commands::request_queries::get_session_detail,
            commands::queries::get_data_source_status,
            commands::queries::sync_codex,
            commands::queries::sync_client,
            commands::queries::sync_usage,
        ])
        .build(context)
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
