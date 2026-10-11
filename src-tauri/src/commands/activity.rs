//! Serializes imports, destructive maintenance and shutdown.
use std::sync::{Condvar, Mutex, MutexGuard};

#[derive(Default)]
struct Activity {
    syncing: bool,
    clearing: bool,
    exiting: bool,
}
#[derive(Default)]
pub struct ActivityGate {
    inner: Mutex<Activity>,
    changed: Condvar,
}

impl ActivityGate {
    fn lock(&self) -> MutexGuard<'_, Activity> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub fn start_sync(&self) -> Result<bool, String> {
        let mut state = self.lock();
        if state.exiting || state.clearing {
            return Err("应用正在退出或清除统计，请稍后重试".into());
        }
        if state.syncing {
            return Ok(false);
        }
        state.syncing = true;
        Ok(true)
    }
    pub fn finish_sync(&self) {
        self.lock().syncing = false;
        self.changed.notify_all();
    }
    pub fn shutdown(&self) {
        let mut state = self.lock();
        state.exiting = true;
        while state.syncing || state.clearing {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
    pub fn is_clearing(&self) -> bool {
        self.lock().clearing
    }
    pub fn begin_clear(&self) -> Result<ClearGuard<'_>, String> {
        let mut state = self.lock();
        if state.exiting || state.clearing {
            return Err("已有维护操作正在执行".into());
        }
        state.clearing = true;
        while state.syncing {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        Ok(ClearGuard(self))
    }
}

pub struct ClearGuard<'a>(&'a ActivityGate);
impl Drop for ClearGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().clearing = false;
        self.0.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn overlapping_scans_and_shutdown_are_serialized() {
        let gate = Arc::new(ActivityGate::default());
        assert_eq!(gate.start_sync(), Ok(true));
        assert_eq!(gate.start_sync(), Ok(false));
        let worker = gate.clone();
        let thread = std::thread::spawn(move || worker.shutdown());
        gate.finish_sync();
        thread.join().expect("shutdown completes after scan");
        assert!(gate.start_sync().is_err());
    }
    #[test]
    fn maintenance_blocks_scans_and_releases_on_drop() {
        let gate = ActivityGate::default();
        {
            let _guard = gate.begin_clear().expect("clear");
            assert!(gate.start_sync().is_err());
            assert!(gate.begin_clear().is_err());
        }
        assert_eq!(gate.start_sync(), Ok(true));
        gate.finish_sync();
    }
    #[test]
    fn maintenance_waits_for_running_scan() {
        let gate = Arc::new(ActivityGate::default());
        gate.start_sync().expect("scan");
        let worker = gate.clone();
        let thread = std::thread::spawn(move || {
            let _guard = worker.begin_clear().expect("clear");
        });
        gate.finish_sync();
        thread.join().expect("finished");
        assert_eq!(gate.start_sync(), Ok(true));
    }
}
