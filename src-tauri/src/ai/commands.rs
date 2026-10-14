//! 面板 AI 生成的 Tauri commands：配置 / 凭据（write-only）/ 连接测试 / 生成规格。
//!
//! 命令名保持 `snake_case`；前端经 `invoke` 调用，参数以 camelCase 传入。
//! 所有错误经可序列化的 [`CommandError`] 返回（中文、可读）。API Key 绝不进入任何返回值。

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::config::{LlmConfigDto, LlmConfigPatch, apply_patch};
use super::credential::{self, CredentialStore, KeyringStore};
use super::http::{self, LlmUsageDto};
use super::prompt::system_prompt;
use crate::commands::CommandError;
use crate::panels::get_panel_catalog;
use crate::settings::SettingsStore;

/// 连接测试结果：`ok=false` 时 `message` 为错误摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConnectionDto {
    pub ok: bool,
    pub message: String,
}

/// 生成结果：已过校验的规格 + 可选用量元数据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedPanelSpecDto {
    pub spec: Value,
    pub usage: Option<LlmUsageDto>,
}

/// 一次出网所需的上下文（明文 Key 只在 Rust 内使用）。
struct LlmContext {
    base_url: String,
    model: String,
    key: Option<String>,
}

fn llm_config_dto(app: &AppHandle) -> Result<LlmConfigDto, CommandError> {
    let config = app
        .state::<SettingsStore>()
        .llm()
        .map_err(CommandError::new)?;
    Ok(LlmConfigDto::new(
        &config,
        credential::has_secret(&KeyringStore),
    ))
}

/// 读取配置并校验已配置；未配置即禁用（返回可读错误）。
fn load_context(app: &AppHandle) -> Result<LlmContext, CommandError> {
    let config = app
        .state::<SettingsStore>()
        .llm()
        .map_err(CommandError::new)?;
    let base_url = config
        .base_url
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CommandError::new("请先配置 Base URL（设置 → 面板 AI）"))?;
    let model = config
        .model
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CommandError::new("请先配置模型名（设置 → 面板 AI）"))?;
    Ok(LlmContext {
        base_url,
        model,
        key: credential::request_secret(),
    })
}

/// 读取非机密配置与 `hasKey`（**永不含 API Key**）。
#[tauri::command]
pub fn get_llm_config(app: AppHandle) -> Result<LlmConfigDto, CommandError> {
    llm_config_dto(&app)
}

/// 更新 Base URL / 模型名（缺省保持不变，空串清除）。API Key 不经此命令。
#[tauri::command(rename_all = "camelCase")]
pub fn set_llm_config(
    app: AppHandle,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<LlmConfigDto, CommandError> {
    let patch = LlmConfigPatch { base_url, model };
    app.state::<SettingsStore>()
        .transform_llm(|current| {
            *current = apply_patch(current, &patch)?;
            Ok(())
        })
        .map_err(CommandError::new)?;
    llm_config_dto(&app)
}

/// 写入 API Key（write-only；空串=清除）。**绝不回读明文**。
#[tauri::command]
pub fn set_llm_credential(app: AppHandle, key: String) -> Result<LlmConfigDto, CommandError> {
    credential::apply_secret(&KeyringStore, &key).map_err(CommandError::new)?;
    llm_config_dto(&app)
}

/// 清除 API Key。
#[tauri::command]
pub fn clear_llm_credential(app: AppHandle) -> Result<LlmConfigDto, CommandError> {
    KeyringStore.clear().map_err(CommandError::new)?;
    llm_config_dto(&app)
}

/// 连接测试：最小 chat 调用（`max_tokens=1`）；网络 / HTTP 失败作为 `ok=false` 摘要返回。
#[tauri::command]
pub async fn test_llm_connection(app: AppHandle) -> Result<LlmConnectionDto, CommandError> {
    let context = load_context(&app)?;
    let body = http::build_ping_body(&context.model);
    match http::chat_completion(&context.base_url, context.key.as_deref(), body).await {
        Ok(response) => match http::extract_content(&response) {
            Ok(_) => Ok(LlmConnectionDto {
                ok: true,
                message: "连接成功".into(),
            }),
            Err(error) => Ok(LlmConnectionDto {
                ok: false,
                message: error,
            }),
        },
        Err(error) => Ok(LlmConnectionDto {
            ok: false,
            message: error.to_string(),
        }),
    }
}

/// 自然语言 → `PanelSpec`。
///
/// system prompt 内嵌 `get_panel_catalog()`（SSOT），响应清洗后过既有校验器；全部错误经 `CommandError`。
#[tauri::command]
pub async fn generate_panel_spec(
    app: AppHandle,
    prompt: String,
) -> Result<GeneratedPanelSpecDto, CommandError> {
    let context = load_context(&app)?;
    let requirement = prompt.trim();
    if requirement.is_empty() {
        return Err(CommandError::new("请先输入面板需求"));
    }
    let system = system_prompt(&get_panel_catalog());
    let body = http::build_generation_body(&context.model, &system, requirement);
    let response = http::chat_completion(&context.base_url, context.key.as_deref(), body).await?;
    let content = http::extract_content(&response)
        .map_err(|error| CommandError::new(format!("响应非法：{error}")))?;
    let enabled = crate::panels::query::enabled_clients(&app)?;
    let spec = http::finish_generated_spec(&content, &enabled, requirement)
        .map_err(|error| CommandError::new(format!("生成的规格无效：{error}")))?;
    let spec =
        serde_json::to_value(&spec).map_err(|_| CommandError::new("无法序列化生成的规格"))?;
    Ok(GeneratedPanelSpecDto {
        spec,
        usage: http::extract_usage(&response),
    })
}
