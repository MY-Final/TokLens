//! 面板 AI 生成（自然语言 → `PanelSpec`），OpenAI 兼容协议、Rust 代发。
//!
//! 边界（见 `docs/generative-panel-spec.md` 与 `docs/architecture.md` ADR「面板 AI 生成」）：
//! - **默认离线、opt-in**：未配置 Base URL / 模型即整体禁用；仅用户显式触发时出网，无遥测。
//! - **出网白名单**：仅 `https://`（任意主机，用户自配）与 `http://localhost` / `http://127.0.0.1`。
//! - **Rust 代发**：前端不直连；不改 CSP、不改 `capabilities`。
//! - **Key 仅存 OS 凭据库**（[`credential`]），write-only；绝不回读明文给前端，绝不落日志 / 设置 / 导出。
//! - **目录即契约**：system prompt 内嵌 `get_panel_catalog()`（SSOT），响应必须过既有 `PanelSpec` 校验器。
//!
//! 依赖方向：`ai` → `panels`（目录 / 校验器）/ `settings`（非机密配置）/ `commands`（CommandError）。
//! `panels` 不反向依赖 `ai`。

pub(crate) mod commands;
mod config;
mod credential;
mod http;
mod prompt;

#[cfg(test)]
mod tests;

pub use config::{LlmConfigDto, LlmConfigPatch};
pub use credential::{CredentialStore, KeyringStore};
pub use http::LlmUsageDto;
