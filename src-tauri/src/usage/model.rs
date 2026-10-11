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
    /// 客户端提供累计值（Codex `thread_token_usage`）：取该会话最新累计值，**禁止跨文件相加**。
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
        Client::OpenCode => "message.id",
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

/// 解析客户端时间戳（ISO 8601 UTC，`YYYY-MM-DDTHH:MM:SS[.fff]Z`）。
///
/// 只支持本机实际出现的 UTC `Z` 形式；其它形式返回 `None`（不猜测时区偏移）。
pub fn parse_iso8601_utc_ms(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if *bytes.last()? != b'Z' {
        return None;
    }
    let year: i64 = value.get(0..4)?.parse().ok()?;
    let month: i64 = value.get(5..7)?.parse().ok()?;
    let day: i64 = value.get(8..10)?.parse().ok()?;
    let hour: i64 = value.get(11..13)?.parse().ok()?;
    let minute: i64 = value.get(14..16)?.parse().ok()?;
    let second: i64 = value.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis: i64 = if bytes.len() > 20 {
        if bytes[19] != b'.' {
            return None;
        }
        let fraction = value.get(20..value.len() - 1)?;
        if fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut digits = fraction.to_string();
        while digits.len() < 3 {
            digits.push('0');
        }
        digits.parse().ok()?
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000) + millis)
}

/// Howard Hinnant 的 `days_from_civil`：返回 1970-01-01 以来的天数。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// 当前时间（Unix 毫秒）。取不到系统时间时返回 0，不做 panic。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso8601_utc_with_and_without_millis() {
        assert_eq!(parse_iso8601_utc_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso8601_utc_ms("2026-10-11T07:37:14.576Z"),
            Some(1_791_704_234_576)
        );
        assert_eq!(
            parse_iso8601_utc_ms("2026-05-24T12:55:06.571Z"),
            Some(1_779_627_306_571)
        );
        assert_eq!(
            parse_iso8601_utc_ms("2024-02-29T00:00:00Z"),
            Some(1_709_164_800_000)
        );
    }

    #[test]
    fn rejects_unsupported_timestamp_forms() {
        for value in [
            "",
            "2026-10-11",
            "2026-10-11T07:37:14+08:00",
            "2026-10-11T07:37:14.123456Z",
            "2026-13-01T00:00:00Z",
            "2026-10-11T25:00:00Z",
            "not-a-date-at-all-here",
        ] {
            assert_eq!(parse_iso8601_utc_ms(value), None, "should reject {value}");
        }
    }

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
