use super::{request_exit, show_window};
use tauri::{
    AppHandle, Emitter,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "立即采集", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "打开设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &sync, &settings, &quit])?;
    let mut tray = TrayIconBuilder::with_id("toklens")
        .tooltip("TokLens · 本地 AI 用量统计")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "settings" => {
                show_window(app);
                let _ = app.emit("navigate-settings", ());
            }
            "sync" => {
                if let Err(error) = crate::commands::request_sync(app, false) {
                    show_window(app);
                    let _ = app.emit("desktop-error", error.message);
                }
            }
            "quit" => request_exit(app),
            _ => {}
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
    Ok(())
}
