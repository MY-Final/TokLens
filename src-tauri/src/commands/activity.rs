//! Serializes imports, destructive maintenance and shutdown.
use std::sync::{Condvar, Mutex, MutexGuard};

#[derive(Default)]
struct Activity {
    syncing: bool,
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
        if state.exiting {
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
        while state.syncing {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
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
}
