//! Cline 会话文件的类型化投影：只保留身份、时间、模型与用量字段。

use crate::collectors::SyncError;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::path::Path;

/// 会话清单：`<sessionId>.json`。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct Manifest {
    pub session_id: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub workspace_root: Option<String>,
    pub metadata: Option<Metadata>,
}

/// 清单里的用量块。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    /// 会话自身的累计用量（不含子代理）。
    pub usage: Option<Usage>,
}

/// 客户端自报的一组用量计数。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

impl Usage {
    /// 客户端不提供总量；输入包含缓存读写，输出已扣掉推理。
    pub(super) fn counts(&self) -> Option<RawTokenCounts> {
        if self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.cache_read_tokens.is_none()
            && self.cache_write_tokens.is_none()
        {
            return None;
        }
        Some(RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_tokens,
            cache_write: self.cache_write_tokens,
            reasoning: None,
            client_total: None,
        })
    }
}

/// 消息文件：`<sessionId>.messages.json`，当前格式版本为 1。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MessagesFile {
    pub version: Option<u64>,
    /// `lead` / `subagent` / `teammate`。
    pub agent: Option<String>,
    pub session_id: Option<String>,
    pub origin: Option<Origin>,
    #[serde(default)]
    pub messages: Vec<StoredMessage>,
}

/// 消息文件的来源信息。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Origin {
    pub parent_thread_id: Option<String>,
}

/// 一条落盘的消息。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredMessage {
    pub id: Option<String>,
    /// 纪元毫秒。
    pub ts: Option<i64>,
    pub model_info: Option<ModelInfo>,
    /// 只在这一轮的最后一条 assistant 消息上出现。
    pub metrics: Option<Metrics>,
}

/// 产生该条消息的模型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelInfo {
    pub id: Option<String>,
}

/// 一轮调用的用量。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metrics {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    /// 当前落盘格式不提供推理计数；出现该字段说明格式已变，口径需要重新确认。
    pub reasoning_token_count: Option<u64>,
}

impl Metrics {
    pub(super) fn counts(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_tokens,
            cache_write: self.cache_write_tokens,
            reasoning: None,
            client_total: None,
        }
    }
}

/// 当前支持的落盘格式版本。
pub(super) const SUPPORTED_MESSAGES_VERSION: u64 = 1;

fn invalid() -> SyncError {
    std::io::Error::other("unsupported Cline session file").into()
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SyncError> {
    let file = std::fs::File::open(path)?;
    serde_json::from_reader(std::io::BufReader::new(file)).map_err(|_| invalid())
}

pub(super) fn read_manifest(path: &Path) -> Result<Manifest, SyncError> {
    read(path)
}

/// 读取消息文件；版本必须明确为 1，缺失或未知都不猜测结构，按不兼容处理。
pub(super) fn read_messages(path: &Path) -> Result<MessagesFile, SyncError> {
    let file: MessagesFile = read(path)?;
    if file.version != Some(SUPPORTED_MESSAGES_VERSION) {
        return Err(invalid());
    }
    Ok(file)
}
