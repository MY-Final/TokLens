//! Versioned, metadata-only preferences in the application-owned directory.
mod model;
#[cfg(test)]
mod tests;

pub use model::{AppSettings, CloseBehavior, LlmConfig, SettingsPatch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// 当前设置文件结构版本。1→2 新增 `panels: []`；2→3 新增非机密 `llm: { baseUrl?, model? }`。
/// 缺失字段由 `#[serde(default)]` 容忍；迁移纯函数见 [`migrate_document`]。
pub(crate) const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Document {
    schema_version: u32,
    settings: AppSettings,
    last_update_check_ms: Option<i64>,
    /// 用户自定义面板定义（纯数据，只存面板规格与布局，不含会话正文与任何用量原始数据）。
    panels: Vec<Value>,
    /// 面板 AI 生成的非机密配置（Base URL / 模型名）；API Key 只存 OS 凭据库，绝不落此文件。
    llm: LlmConfig,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: AppSettings::default(),
            last_update_check_ms: None,
            panels: Vec::new(),
            llm: LlmConfig::default(),
        }
    }
}

/// 将任意已知版本的设置文档前向迁移到 [`SCHEMA_VERSION`]（纯函数，可单测）。
///
/// - 缺失 `schemaVersion` 视为当前版本（与 `Document` 的 serde 默认一致）。
/// - 高于当前版本：返回错误，调用方回退默认设置并给出可见警告（不静默丢弃）。
/// - 2 → 3：补齐非机密 `llm` 配置（缺失字段仍由 serde 默认兜底，这里显式确保键存在以便写回）。
pub(crate) fn migrate_document(mut value: Value) -> Result<Value, String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "设置文件根节点无效".to_string())?;
    let version = match object.get("schemaVersion") {
        None => SCHEMA_VERSION,
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| "设置文件版本无效".to_string())?,
        Some(_) => return Err("设置文件版本无效".to_string()),
    };
    if version > SCHEMA_VERSION {
        return Err(format!("设置文件版本 {version} 暂不支持，请升级 TokLens"));
    }
    if version < 3 {
        object
            .entry("llm".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    object.insert("schemaVersion".to_string(), Value::from(SCHEMA_VERSION));
    Ok(value)
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
            Ok(bytes) => {
                let migrated = serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| ())
                    .and_then(|value| migrate_document(value).map_err(|_| ()));
                match migrated.map(serde_json::from_value::<Document>) {
                    Ok(Ok(doc)) if doc.settings.validate().is_ok() => (doc, None),
                    _ => (
                        Document::default(),
                        Some("设置文件无法识别，已使用默认值；保存设置后将修复该文件".into()),
                    ),
                }
            }
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

    /// 读取面板 AI 生成的非机密配置（Base URL / 模型名）；不含 API Key。
    pub fn llm(&self) -> Result<LlmConfig, String> {
        Ok(self.lock()?.document.llm.clone())
    }

    /// 原子地修改非机密 LLM 配置；与 `transform`/`transform_panels` 共用版本化文档与原子写。
    pub fn transform_llm(
        &self,
        change: impl FnOnce(&mut LlmConfig) -> Result<(), String>,
    ) -> Result<LlmConfig, String> {
        let mut inner = self.lock()?;
        let mut document = inner.document.clone();
        change(&mut document.llm)?;
        Self::save(&inner.path, &document)?;
        inner.document = document;
        inner.warning = None;
        Ok(inner.document.llm.clone())
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
