use serde::Serialize;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::Update;

#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Disabled,
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Downloaded,
    Installing,
    Unavailable,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub phase: UpdatePhase,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub platform: &'static str,
}
impl Default for UpdateState {
    fn default() -> Self {
        let supported = cfg!(any(target_os = "windows", target_os = "macos"));
        Self {
            phase: if cfg!(debug_assertions) || !supported {
                UpdatePhase::Disabled
            } else {
                UpdatePhase::Idle
            },
            version: None,
            notes: None,
            downloaded_bytes: 0,
            total_bytes: None,
            error: if cfg!(debug_assertions) {
                Some("开发模式不执行更新，请使用正式安装版本".into())
            } else if !supported {
                Some("此平台暂不支持应用内更新".into())
            } else {
                None
            },
            platform: if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "macos") {
                "macos"
            } else {
                "unsupported"
            },
        }
    }
}
impl UpdateState {
    pub fn busy(&self) -> bool {
        matches!(
            self.phase,
            UpdatePhase::Checking | UpdatePhase::Downloading | UpdatePhase::Installing
        )
    }
}

#[derive(Default)]
pub struct UpdateSession {
    pub view: UpdateState,
    pub update: Option<Update>,
    pub bytes: Option<Vec<u8>>,
}
impl UpdateSession {
    pub fn complete_download(&mut self, result: Result<Vec<u8>, tauri_plugin_updater::Error>) {
        match result {
            Ok(bytes) => {
                self.bytes = Some(bytes);
                self.view.phase = UpdatePhase::Downloaded;
                self.view.error = None;
            }
            Err(error) => {
                self.bytes = None;
                self.view.phase = UpdatePhase::Failed;
                self.view.error = Some(super::error_message(&error));
            }
        }
    }
}
#[derive(Default)]
pub struct UpdateManager(Mutex<UpdateSession>);
impl UpdateManager {
    pub fn lock(&self) -> Result<MutexGuard<'_, UpdateSession>, String> {
        self.0.lock().map_err(|_| "更新状态异常，请重启应用".into())
    }
}

pub(super) fn broadcast(app: &AppHandle) {
    if let Ok(session) = app.state::<UpdateManager>().lock() {
        let _ = app.emit("update-state", session.view.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_failure_never_leaves_installable_bytes() {
        let mut session = UpdateSession {
            bytes: Some(vec![1]),
            ..Default::default()
        };
        session.complete_download(Err(tauri_plugin_updater::Error::SignatureUtf8(
            "synthetic-invalid-signature".into(),
        )));
        assert!(session.bytes.is_none());
        assert_eq!(session.view.phase, UpdatePhase::Failed);
        assert!(session.view.error.expect("error").contains("签名"));
    }
    #[test]
    fn successful_verified_download_reaches_downloaded_state() {
        let mut session = UpdateSession::default();
        session.complete_download(Ok(vec![1, 2, 3]));
        assert_eq!(session.view.phase, UpdatePhase::Downloaded);
        assert_eq!(session.bytes, Some(vec![1, 2, 3]));
    }
}
