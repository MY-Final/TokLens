use serde::Serialize;

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
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}