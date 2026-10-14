//! LLM 非机密配置（Base URL / 模型名）的规范化与校验（纯函数），以及跨端 DTO。
//!
//! 本模块只处理非机密值；API Key 一律经 [`super::credential`] 存入 OS 凭据库，绝不进入设置或 DTO。

use serde::{Deserialize, Serialize};

use crate::settings::LlmConfig;

/// Base URL 最大字符数。
pub const BASE_URL_MAX_LEN: usize = 512;
/// 模型名最大字符数。
pub const MODEL_MAX_LEN: usize = 200;

/// `set_llm_config` 入参：字段缺省表示保持不变，空串（或纯空白）表示清除。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmConfigPatch {
    pub base_url: Option<String>,
    pub model: Option<String>,
}

/// 跨端返回的 LLM 配置；**永不包含 API Key**，只给出 `hasKey` 布尔。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigDto {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub has_key: bool,
}

impl LlmConfigDto {
    pub fn new(config: &LlmConfig, has_key: bool) -> Self {
        Self {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            has_key,
        }
    }
}

/// 规范化并可校验 Base URL：仅 `https://`（任意主机）或 `http://localhost` / `http://127.0.0.1`。
///
/// 去除首尾空白与结尾斜杠；拒绝空串、过长、非法 scheme、非本地 http、含凭据 / 查询 / 片段。
pub fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Base URL 不能为空".into());
    }
    if trimmed.chars().count() > BASE_URL_MAX_LEN {
        return Err(format!("Base URL 不能超过 {BASE_URL_MAX_LEN} 个字符"));
    }
    let url = reqwest::Url::parse(trimmed).map_err(|_| "Base URL 格式无效".to_string())?;
    match url.scheme() {
        "https" => {}
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if host != "localhost" && host != "127.0.0.1" {
                return Err("仅允许 https:// 或 http://localhost / http://127.0.0.1".into());
            }
        }
        _ => return Err("仅支持 https:// 或本地 http:// 地址".into()),
    }
    if url.host_str().is_none() {
        return Err("Base URL 缺少主机名".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Base URL 不能包含用户名或密码".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Base URL 不能包含查询参数或片段".into());
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}

/// 规范化模型名：去空白、非空、长度上限。
pub fn normalize_model(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("模型名不能为空".into());
    }
    if trimmed.chars().count() > MODEL_MAX_LEN {
        return Err(format!("模型名不能超过 {MODEL_MAX_LEN} 个字符"));
    }
    Ok(trimmed.to_string())
}

/// 应用配置补丁：缺省字段保持不变，空串清除，其余规范化后写入。
pub fn apply_patch(current: &LlmConfig, patch: &LlmConfigPatch) -> Result<LlmConfig, String> {
    let mut next = current.clone();
    if let Some(raw) = &patch.base_url {
        next.base_url = normalize_optional(raw, normalize_base_url)?;
    }
    if let Some(raw) = &patch.model {
        next.model = normalize_optional(raw, normalize_model)?;
    }
    Ok(next)
}

/// 空串（或纯空白）→ `None`（清除），否则规范化。
fn normalize_optional(
    raw: &str,
    normalize: fn(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    if raw.trim().is_empty() {
        Ok(None)
    } else {
        normalize(raw).map(Some)
    }
}
