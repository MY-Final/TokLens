//! Native timer: elapsed time is injected into the schedule for deterministic tests.
use super::request_sync;
use crate::settings::SettingsStore;
use std::time::{Duration, Instant};
use tauri::Manager;

#[derive(Default)]
struct Schedule {
    interval: u32,
    next: Option<Duration>,
}
impl Schedule {
    fn due(&mut self, elapsed: Duration, interval: u32, paused: bool) -> bool {
        if paused || interval == 0 {
            self.interval = 0;
            self.next = None;
            return false;
        }
        let period = Duration::from_secs(u64::from(interval) * 60);
        if self.interval != interval || self.next.is_none() {
            self.interval = interval;
            self.next = Some(elapsed + period);
            return false;
        }
        if self.next.is_some_and(|next| elapsed >= next) {
            self.next = Some(elapsed + period);
            return true;
        }
        false
    }
}

pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut schedule = Schedule::default();
        loop {
            if app
                .state::<crate::desktop::DesktopState>()
                .exit_requested
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
            if let Ok(snapshot) = app.state::<SettingsStore>().snapshot()
                && schedule.due(
                    start.elapsed(),
                    snapshot.settings.sync_interval_minutes,
                    snapshot.settings.collection_paused,
                )
            {
                let _ = request_sync(&app, false);
            }
            std::thread::sleep(Duration::from_secs(15));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_skips_backlog_and_rearms_after_pause_or_interval_change() {
        let mut schedule = Schedule::default();
        let t = Duration::from_secs;
        assert!(!schedule.due(t(0), 5, false));
        assert!(!schedule.due(t(299), 5, false));
        assert!(schedule.due(t(300), 5, false));
        assert!(schedule.due(t(9000), 5, false));
        assert!(!schedule.due(t(9000), 5, false));
        assert!(!schedule.due(t(9010), 5, true));
        assert!(!schedule.due(t(9020), 1, false));
        assert!(schedule.due(t(9080), 1, false));
        assert!(!schedule.due(t(10000), 0, false));
    }
}
