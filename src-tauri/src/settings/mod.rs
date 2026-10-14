//! Versioned, metadata-only preferences in the application-owned directory.
mod model;
#[cfg(test)]
mod tests;

pub use model::{AppSettings, CloseBehavior, SettingsPatch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// 当前设置文件结构版本。1→2 仅新增 `panels: []`（`#[serde(default)]` 容忍缺失字段）。
pub(crate) const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Document {
    schema_version: u32,
    settings: AppSettings,
    last_update_check_ms: Option<i64>,
    /// 用户自定义面板定义（纯数据，只存面板规格与布局，不含会话正文与任何用量原始数据）。
    panels: Vec<Value>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: AppSettings::default(),
            last_update_check_ms: None,
            panels: Vec::new(),
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
    path: PathBuf,
    document: Document,
    warning: Option<String>,
}

pub struct SettingsStore {
    inner: Mutex<Preferences>,
}

impl SettingsStore {
    pub fn load(directory: &Path) -> Self {
        let path = directory.join("settings.json");
        let (document, warning) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Document>(&bytes) {
                Ok(mut doc)
                    if (doc.schema_version == 1 || doc.schema_version == SCHEMA_VERSION)
                        && doc.settings.validate().is_ok() =>
                {
                    // 1 → 2：仅补 `panels: []`，缺失字段已由 serde 默认值兜底；下次保存落盘。
                    doc.schema_version = SCHEMA_VERSION;
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
            inner: Mutex::new(Preferences {
                path,
                document,
                warning,
            }),
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
        // `change`（经 `AppSettings::patched`）已在应用补丁时校验，无需重复校验。
        document.settings = change(&document.settings)?;
        Self::save(&inner.path, &document)?;
        inner.document = document;
        inner.warning = None;
        Ok(SettingsSnapshot {
            settings: inner.document.settings.clone(),
            warning: None,
        })
    }

    /// 读取面板定义原始 JSON。容忍更高 `specVersion` 的单条面板：个别面板损坏不应重置整份设置。
    pub fn panels(&self) -> Result<Vec<Value>, String> {
        Ok(self.lock()?.document.panels.clone())
    }

    /// 原子地修改面板定义，与 [`SettingsStore::transform`] 共用版本化文档与原子写、随存储迁移。
    pub fn transform_panels(
        &self,
        change: impl FnOnce(&mut Vec<Value>) -> Result<(), String>,
    ) -> Result<Vec<Value>, String> {
        let mut inner = self.lock()?;
        let mut document = inner.document.clone();
        change(&mut document.panels)?;
        Self::save(&inner.path, &document)?;
        inner.document = document;
        inner.warning = None;
        Ok(inner.document.panels.clone())
    }

    pub fn last_update_check(&self) -> Result<Option<i64>, String> {
        Ok(self.lock()?.document.last_update_check_ms)
    }

    pub fn record_update_check(&self, now_ms: i64) -> Result<(), String> {
        let mut inner = self.lock()?;
        let mut document = inner.document.clone();
        document.last_update_check_ms = Some(now_ms);
        Self::save(&inner.path, &document)?;
        inner.document = document;
        Ok(())
    }

    pub(crate) fn relocate<T>(
        &self,
        directory: &Path,
        commit: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut inner = self.lock()?;
        let path = directory.join("settings.json");
        Self::save(&path, &inner.document)?;
        let result = commit()?;
        inner.path = path;
        inner.warning = None;
        Ok(result)
    }

    fn save(path: &Path, document: &Document) -> Result<(), String> {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let bytes = serde_json::to_vec_pretty(document)?;
            let temporary = path.with_extension("json.tmp");
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(temporary, path)?;
            Ok(())
        })();
        result.map_err(|_| "无法保存设置，请检查应用数据目录是否可写".into())
    }
}
