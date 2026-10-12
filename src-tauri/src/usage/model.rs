//! 统一数据模型：客户端无关的会话/消息结构与去重标识。
//!
//! 字段语义与去重叠规则见 [`crate::usage::tokens`]；每个客户端的字段差异只允许在
//! [`crate::usage::normalize`] 中处理，模型本身不包含任何客户端特有字段。

use crate::usage::tokens::{Client, NormalizedUsage};

/// 数据来源等级（见 `docs/data-metrics.md`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// 客户端真实记录。
    Measured,
    /// 由真实字段确定性换算。
    Derived,
    /// 含假设/经验系数的推算。
    Estimated,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Measured => "measured",
            SourceKind::Derived => "derived",
            SourceKind::Estimated => "estimated",
        }
    }

    pub fn parse(value: &str) -> Option<SourceKind> {
        match value {
            "measured" => Some(SourceKind::Measured),
            "derived" => Some(SourceKind::Derived),
            "estimated" => Some(SourceKind::Estimated),
            _ => None,
        }
    }
}

/// 会话总量的取值依据。用于区分"客户端累计值"与"请求求和"，避免两者混用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTotalBasis {
    /// 客户端提供累计值（Codex `thread_token_usage`）：取该线程累计值最大值，**禁止跨文件相加**。
    ClientCumulative,
    /// 客户端提供会话级总量（OpenCode `session.tokens_*`）。
    ClientReported,
    /// 客户端只有请求级数据，总量由去重后的消息求和得出（Claude Code）。
    SumOfMessages,
}

impl SessionTotalBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionTotalBasis::ClientCumulative => "client_cumulative",
            SessionTotalBasis::ClientReported => "client_reported",
            SessionTotalBasis::SumOfMessages => "sum_of_messages",
        }
    }

    pub fn parse(value: &str) -> Option<SessionTotalBasis> {
        match value {
            "client_cumulative" => Some(SessionTotalBasis::ClientCumulative),
            "client_reported" => Some(SessionTotalBasis::ClientReported),
            "sum_of_messages" => Some(SessionTotalBasis::SumOfMessages),
            _ => None,
        }
    }
}

/// 请求明细相对会话累计值的完整度（Codex 分页/续接文件只保留部分请求）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCoverage {
    /// 明细覆盖整个会话。
    Complete,
    /// 明细只覆盖部分（源文件被截断/分页/续接），会话总量仍以累计值为准。
    Partial,
    /// 没有任何请求级明细（只有会话级累计值或旧格式事件）。
    NoMessages,
}

impl MessageCoverage {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageCoverage::Complete => "complete",
            MessageCoverage::Partial => "partial",
            MessageCoverage::NoMessages => "none",
        }
    }

    pub fn parse(value: &str) -> Option<MessageCoverage> {
        match value {
            "complete" => Some(MessageCoverage::Complete),
            "partial" => Some(MessageCoverage::Partial),
            "none" => Some(MessageCoverage::NoMessages),
            _ => None,
        }
    }
}

/// 费用来源。TokLens 本轮不做价格换算，只保存客户端自报成本（费用与 Token 采集解耦）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostSource {
    /// 客户端自己算出的成本（仅 OpenCode 提供）。
    ClientReported,
    /// 无成本数据；不用 token 反推、不落猜测值。
    None,
}

impl CostSource {
    pub fn as_str(self) -> &'static str {
        match self {
            CostSource::ClientReported => "client_reported",
            CostSource::None => "none",
        }
    }

    pub fn parse(value: &str) -> Option<CostSource> {
        match value {
            "client_reported" => Some(CostSource::ClientReported),
            "none" => Some(CostSource::None),
            _ => None,
        }
    }
}

/// 客户端无关的会话记录（写库前）。
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub client: Client,
    /// 客户端原始会话/线程标识。
    pub client_session_id: String,
    /// 续接来源（Codex `history_base.thread_id`、OpenCode `parent_id`）；用于去重与溯源。
    pub parent_client_session_id: Option<String>,
    /// 原始工作目录/工作树。仅本机存储，不进日志与遥测。
    pub project_path: Option<String>,
    pub model: Option<String>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    /// 口径标识（[`crate::usage::tokens::TokenSemantics::id`]）。
    pub semantics_id: String,
    pub total_basis: SessionTotalBasis,
    pub coverage: MessageCoverage,
    pub usage: NormalizedUsage,
    pub source_kind: SourceKind,
    pub cost_source: CostSource,
    pub client_cost_usd: Option<f64>,
}

/// 客户端无关的请求级记录（写库前）。
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub client: Client,
    pub client_session_id: String,
    /// 客户端内去重标识（见 [`dedup_key_note`]）。
    pub dedup_key: String,
    /// 源内序号，用于稳定排序（Codex `ordinal`、OpenCode `seq`）。
    pub seq: Option<i64>,
    pub created_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub model: Option<String>,
    /// OpenCode `agent`/`mode`，Claude Code 无此字段。
    pub agent: Option<String>,
    pub usage: NormalizedUsage,
    /// Claude Code 子代理消息（`isSidechain`）。
    pub is_sidechain: bool,
    pub semantics_id: String,
    pub source_kind: SourceKind,
    /// 记录定位信息（不含正文），例如 `ordinal=812` / `response_id=...`。
    pub source_ref: Option<String>,
    pub cost_source: CostSource,
    pub client_cost_usd: Option<f64>,
}

/// 去重标识说明（与 `docs/data-model.md` 一致）：
///
/// - Codex：`response_id`；旧格式（无 `response_id`）用 `token_count:<ordinal>`。
/// - Claude Code：`message.id`（同一 `(sessionId, message.id)` 会重复写入，值一致，保留一条）。
/// - OpenCode：`message.id`。
pub fn dedup_key_note(client: Client) -> &'static str {
    match client {
        Client::Codex => "response_id | token_count:<ordinal>",
        Client::ClaudeCode => "message.id",
        Client::WorkBuddy => {
            "sessionId + item.id; providerData.usage and message.usage are mirrors"
        }
        Client::Copilot => {
            "SDK session id + apiCallId; CLI usage:<row id>; host totals by user event id"
        }
        Client::OpenCode => "message.id",
        Client::GeminiCli => "sessionId + final gemini message id after journal replay",
        Client::Grok => "sessionId + turnNumber; turn-level aggregate, not individual model call",
        Client::Mcode => "local_runtime_sessions.session_id + local_runtime_token_usage.id",
        Client::MimoCode => "native session id + message id; exclude imported session copies",
        Client::PiDesktop => {
            "physical operationId; legacy session + message id; expand aggregates once"
        }
        Client::KimiCode => "session id + agent folder + physical wire line number",
        Client::Dsh => {
            "turn + step (chunk sample replaced by completed message, excluding inherited seed)"
        }
    }
}

/// 会话主键：`<client>:<客户端会话 id>`。
pub fn session_id(client: Client, client_session_id: &str) -> String {
    format!("{}:{}", client.as_str(), client_session_id)
}

/// 消息主键：`<client>:<客户端会话 id>:<去重标识>`。
pub fn message_id(client: Client, client_session_id: &str, dedup_key: &str) -> String {
    format!("{}:{}:{}", client.as_str(), client_session_id, dedup_key)
}

/// 把未知项列表序列化成数据库列（逗号分隔，保持空值语义清晰）。
pub fn encode_unknown_relations(relations: &[&str]) -> Option<String> {
    if relations.is_empty() {
        None
    } else {
        Some(relations.join(","))
    }
}

/// 反序列化未知项列表。
pub fn decode_unknown_relations(value: Option<&str>) -> Vec<String> {
    match value {
        Some(value) if !value.is_empty() => value.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

// Compatibility exports; new callers should use usage::time.
pub use super::time::{now_ms, parse_iso8601_utc_ms};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_and_unknown_relation_encoding_round_trip() {
        assert_eq!(session_id(Client::Codex, "abc"), "codex:abc");
        assert_eq!(
            message_id(Client::ClaudeCode, "abc", "msg_1"),
            "claude_code:abc:msg_1"
        );
        let encoded = encode_unknown_relations(&["a", "b"]);
        assert_eq!(encoded.as_deref(), Some("a,b"));
        assert_eq!(decode_unknown_relations(encoded.as_deref()), vec!["a", "b"]);
        assert_eq!(encode_unknown_relations(&[]), None);
        assert!(decode_unknown_relations(None).is_empty());
    }

    #[test]
    fn enum_string_mapping_round_trips() {
        for client in Client::ALL {
            assert_eq!(Client::parse(client.as_str()), Some(client));
            assert!(!dedup_key_note(client).is_empty());
        }
        for basis in [
            SessionTotalBasis::ClientCumulative,
            SessionTotalBasis::ClientReported,
            SessionTotalBasis::SumOfMessages,
        ] {
            assert_eq!(SessionTotalBasis::parse(basis.as_str()), Some(basis));
        }
        for coverage in [
            MessageCoverage::Complete,
            MessageCoverage::Partial,
            MessageCoverage::NoMessages,
        ] {
            assert_eq!(MessageCoverage::parse(coverage.as_str()), Some(coverage));
        }
        for kind in [
            SourceKind::Measured,
            SourceKind::Derived,
            SourceKind::Estimated,
        ] {
            assert_eq!(SourceKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(
            CostSource::parse(CostSource::ClientReported.as_str()),
            Some(CostSource::ClientReported)
        );
        assert_eq!(CostSource::parse("nope"), None);
        assert!(now_ms() > 0);
    }
}
