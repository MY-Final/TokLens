//! OpenAI 兼容协议的请求构造、响应清洗 / 解析（纯函数）与 `reqwest` 调用。
//!
//! 出网仅在用户显式触发时发生；API Key 只作为请求头，绝不进入日志、DTO 或错误信息。

use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::commands::CommandError;
use crate::panels::spec::{PanelSpec, SUPPORTED_SPEC_VERSION, parse_spec, validate_spec};
use crate::usage::client::Client;

/// 响应的最大字节数（1 MB），超出即拒绝，避免异常响应放大。
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
/// 连接超时与整体超时。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
/// 生成面板的 `max_tokens`。
const GENERATED_MAX_TOKENS: u32 = 1024;

/// LLM 用量（响应 `usage` 字段；可选，纯元数据）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmUsageDto {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

/// 拼接 chat completions 端点：`{baseUrl}/chat/completions`。
pub fn chat_completions_url(base_url: &str) -> String {
    format!("{base_url}/chat/completions")
}

/// 构造面板生成的请求体（OpenAI 兼容：system=目录+规则，user=需求）。
pub fn build_generation_body(model: &str, system: &str, user: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "temperature": 0.2,
        "max_tokens": GENERATED_MAX_TOKENS,
    })
}

/// 构造连接测试的请求体：最小 chat 调用（`max_tokens=1`）。
pub fn build_ping_body(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [ { "role": "user", "content": "ping" } ],
        "temperature": 0,
        "max_tokens": 1,
    })
}

/// 取出 `choices[0].message.content`。
pub fn extract_content(response: &Value) -> Result<String, String> {
    response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "响应缺少 choices[0].message.content".to_string())
}

/// 取出可选 `usage`。
pub fn extract_usage(response: &Value) -> Option<LlmUsageDto> {
    let usage = response.get("usage")?;
    Some(LlmUsageDto {
        prompt_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
        completion_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
    })
}

/// 剥离 ```json 围栏并去除首尾空白（无围栏时仅 trim）。
pub fn strip_code_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    // 丢弃围栏信息行（如 ```json）。
    let body = match rest.find('\n') {
        Some(index) => &rest[index + 1..],
        None => "",
    };
    let body = body.trim_end();
    let body = body.strip_suffix("```").unwrap_or(body);
    body.trim().to_string()
}

/// 清洗模型输出并过 `PanelSpec` 权威校验器（拒绝未知字段 / 越界 / 未知客户端）。
///
/// 仅做最小修补：缺失 `specVersion` 补 1、强制 `source.origin=ai`、缺失或空的 `id` 由 `seed` 确定性派生。
/// 其余一律交给既有校验器，不合法则返回可读错误。
pub fn finish_generated_spec(
    content: &str,
    enabled: &[Client],
    seed: &str,
) -> Result<PanelSpec, String> {
    let cleaned = strip_code_fence(content);
    let mut value: Value =
        serde_json::from_str(&cleaned).map_err(|error| format!("模型未返回合法 JSON：{error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "模型返回的 JSON 不是对象".to_string())?;
    object
        .entry("specVersion".to_string())
        .or_insert_with(|| Value::from(SUPPORTED_SPEC_VERSION));
    object.insert("source".to_string(), json!({ "origin": "ai" }));
    let needs_id = object
        .get("id")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty);
    if needs_id {
        object.insert(
            "id".to_string(),
            Value::from(format!("p_ai_{:016x}", fnv1a64(seed.as_bytes()))),
        );
    }
    let parsed = parse_spec(value)?;
    validate_spec(parsed, enabled).map(|validated| validated.spec)
}

/// FNV-1a 64 位散列，用于确定性地派生 AI 面板 `id`。
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 复用既有 TLS 提供方：`rustls-no-provider` 需要进程默认提供方（与 updater 一致用 ring）。
fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

fn http_client() -> Result<&'static reqwest::Client, CommandError> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    ensure_crypto_provider();
    let built = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .build()
        .map_err(|_| CommandError::new("无法初始化网络客户端"))?;
    Ok(CLIENT.get_or_init(|| built))
}

/// 发送一次 chat completions 请求并返回解析后的 JSON。
///
/// `key` 仅作为请求头；错误摘要不包含 URL、主机或任何请求 / 凭据内容。
pub async fn chat_completion(
    base_url: &str,
    key: Option<&str>,
    body: Value,
) -> Result<Value, CommandError> {
    let client = http_client()?;
    let url = chat_completions_url(base_url);
    let mut request = client.post(&url).json(&body);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| CommandError::new(format!("网络请求失败：{}", network_summary(&error))))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CommandError::new(format!(
            "服务返回 HTTP {}",
            status.as_u16()
        )));
    }
    let bytes = read_bounded(response).await?;
    serde_json::from_slice::<Value>(&bytes).map_err(|_| CommandError::new("响应不是合法 JSON"))
}

async fn read_bounded(mut response: reqwest::Response) -> Result<Vec<u8>, CommandError> {
    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Err(CommandError::new("响应体过大，已拒绝"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| CommandError::new(format!("读取响应失败：{}", network_summary(&error))))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(CommandError::new("响应体过大，已拒绝"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// 非泄露性错误摘要：不包含 URL、主机或任何请求内容。
fn network_summary(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "无法连接到服务地址"
    } else if error.is_body() || error.is_decode() {
        "响应读取失败"
    } else {
        "网络错误"
    }
}
