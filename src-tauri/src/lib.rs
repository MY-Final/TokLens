pub mod collectors;
pub mod commands;
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
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 本地库放在 TokLens 专属应用数据目录（客户端数据目录只读，绝不写入）。
            let data_dir = app.path().app_data_dir()?;
            let state = commands::AppState::initialize(&data_dir)?;
            app.manage(state);
            // 启动即做一次增量同步：首次运行导入历史，之后只处理变化的文件。
            commands::request_sync(app.handle(), false)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            commands::queries::get_overview_stats,
            commands::queries::get_daily_token_trend,
            commands::queries::get_client_usage,
            commands::queries::get_rankings,
            commands::queries::get_sessions,
            commands::queries::get_data_source_status,
            commands::queries::sync_codex,
            commands::queries::sync_usage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
