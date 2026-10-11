use super::{UpdateManager, UpdatePhase, check_updates};
use crate::settings::SettingsStore;
use crate::usage::time::now_ms;
use tauri::{AppHandle, Manager};
fn automatic_due(enabled: bool, last: Option<i64>, now: i64) -> bool {
    enabled && last.is_none_or(|last| now < last || now.saturating_sub(last) >= 86_400_000)
}

pub fn maybe_check_automatically(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let store = app.state::<SettingsStore>();
    let Ok(snapshot) = store.snapshot() else {
        return;
    };
    let Ok(last) = store.last_update_check() else {
        return;
    };
    let ready = app.state::<UpdateManager>().lock().is_ok_and(|session| {
        matches!(
            session.view.phase,
            UpdatePhase::Idle
                | UpdatePhase::UpToDate
                | UpdatePhase::Failed
                | UpdatePhase::Unavailable
        )
    });
    if ready && automatic_due(snapshot.settings.automatic_updates, last, now_ms()) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = check_updates(app).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_checks_are_opt_in_and_at_most_daily() {
        assert!(!automatic_due(false, None, 0));
        assert!(automatic_due(true, None, 0));
        assert!(!automatic_due(true, Some(0), 86_399_999));
        assert!(automatic_due(true, Some(0), 86_400_000));
        assert!(automatic_due(true, Some(100), 1));
    }
}
