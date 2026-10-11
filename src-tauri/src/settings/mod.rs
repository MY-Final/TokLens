//! Versioned, metadata-only preferences in the application-owned directory.
mod model;
#[cfg(test)]
mod tests;

pub use model::{AppSettings, CloseBehavior, SettingsPatch};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Document {
    schema_version: u32,
    settings: AppSettings,
    last_update_check_ms: Option<i64>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: 1,
            settings: AppSettings::default(),
            last_update_check_ms: None,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub settings: AppSettings,
    pub warning: Option<String>,
}

struct Preferences {
    document: Document,
    warning: Option<String>,
}

pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<Preferences>,
}

impl SettingsStore {
    pub fn load(directory: &Path) -> Self {
        let path = directory.join("settings.json");
        let (document, warning) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Document>(&bytes) {
                Ok(doc) if doc.schema_version == 1 && doc.settings.validate().is_ok() => {
                    (doc, None)
                }
                _ => (
                    Document::default(),
                    Some("设置文件无法识别，已使用默认值；保存设置后将修复该文件".into()),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Document::default(), None)
            }
            Err(_) => (
                Document::default(),
                Some("无法读取设置，已使用默认值".into()),
            ),
        };
        Self {
            path,
            inner: Mutex::new(Preferences { document, warning }),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Preferences>, String> {
        self.inner
            .lock()
            .map_err(|_| "设置状态异常，请重启应用".into())
    }

    pub fn snapshot(&self) -> Result<SettingsSnapshot, String> {
        let inner = self.lock()?;
        Ok(SettingsSnapshot {
            settings: inner.document.settings.clone(),
            warning: inner.warning.clone(),
        })
    }

    pub fn update(&self, patch: SettingsPatch) -> Result<SettingsSnapshot, String> {
        self.transform(|current| current.patched(patch))
    }

    pub fn transform(
        &self,
        change: impl FnOnce(&AppSettings) -> Result<AppSettings, String>,
    ) -> Result<SettingsSnapshot, String> {
        let mut inner = self.lock()?;
        let mut document = inner.document.clone();
        document.settings = change(&document.settings)?;
        document.settings.validate()?;
        self.save(&document)?;
        inner.document = document;
        inner.warning = None;
        Ok(SettingsSnapshot {
            settings: inner.document.settings.clone(),
            warning: None,
        })
    }

    pub fn last_update_check(&self) -> Result<Option<i64>, String> {
        Ok(self.lock()?.document.last_update_check_ms)
    }

    pub fn record_update_check(&self, now_ms: i64) -> Result<(), String> {
        let mut inner = self.lock()?;
        let mut document = inner.document.clone();
        document.last_update_check_ms = Some(now_ms);
        self.save(&document)?;
        inner.document = document;
        Ok(())
    }

    fn save(&self, document: &Document) -> Result<(), String> {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let bytes = serde_json::to_vec_pretty(document)?;
            let temporary = self.path.with_extension("json.tmp");
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(temporary, &self.path)?;
            Ok(())
        })();
        result.map_err(|_| "无法保存设置，请检查应用数据目录是否可写".into())
    }
}
