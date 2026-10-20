//! Cline 的会话与请求换算：会话累计值优先，逐轮明细负责模型与时间分布。
//!
//! Cline 在会话目录里写两份文件：清单（`<sessionId>.json`）给出该会话自身的累计用量，
//! 消息文件（`<sessionId>.messages.json`）给出每一轮落盘的 `metrics`。
//! 两者是同一批计数的两种视图：明细逐轮求和应等于清单累计值。
//! 不一致时保留清单累计值（客户端更权威），并把明细标记为不完整。
//!
//! 清单里的 `aggregateUsage` 还包含子代理用量，而子代理本身有独立的会话目录；
//! 若再把 `aggregateUsage` 加进父会话就会重复计数，因此这里只使用 `usage`。

use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::normalize::request_ledger::{self, RequestMeta};
use crate::usage::tokens::{
    Client, RawTokenCounts, UNKNOWN_TOTAL_INCOMPLETE, UsageTotalBasis, normalize_usage,
    semantics_for,
};

/// 明细逐轮求和与会话累计值不一致：会话总量仍取客户端累计值，明细视为不完整。
pub const UNKNOWN_DETAIL_MISMATCH: &str = "cline.detail_mismatch";

/// 落盘文件里出现推理计数：说明"输出已扣掉推理"的关系不再成立。
pub const UNKNOWN_REASONING_UNVERIFIED: &str = "cline.reasoning_unverified";

/// 一轮用量记录：Cline 把该轮的 `metrics` 写在最后一条 assistant 消息上。
///
/// `reasoning` 是落盘文件里的推理计数。当前格式不写该字段；一旦出现，
/// 就说明字段关系已变，总量不再可确认，按未知处理而不是猜测。
pub fn request(
    session_id: &str,
    meta: RequestMeta,
    raw: &RawTokenCounts,
    reasoning: Option<u64>,
) -> MessageRecord {
    let mut message = request_ledger::request(Client::Cline, session_id, meta, raw);
    if reasoning.is_some_and(|value| value > 0) {
        message.usage.total_tokens = None;
        message.usage.total_basis = UsageTotalBasis::Unavailable;
        message
            .usage
            .unknown_relations
            .push(UNKNOWN_REASONING_UNVERIFIED);
        message
            .usage
            .unknown_relations
            .push(UNKNOWN_TOTAL_INCOMPLETE);
    }
    message.usage.unknown_relations.sort_unstable();
    message.usage.unknown_relations.dedup();
    // Cline 不自报总量：规范总量由互不重叠的分桶换算而来。
    message.source_kind = if message.usage.total_tokens.is_some() {
        SourceKind::Derived
    } else {
        SourceKind::Measured
    };
    message
}

/// 会话级输入：清单累计值与逐轮明细是同一批计数的两种视图。
pub struct SessionInput<'a> {
    pub id: &'a str,
    pub parent: Option<&'a str>,
    pub project: Option<&'a str>,
    pub model: Option<&'a str>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    /// 清单里的会话累计用量（不含子代理）。
    pub cumulative: Option<RawTokenCounts>,
    /// 逐轮明细求和；没有明细文件时为 `None`。
    pub detail: Option<RawTokenCounts>,
    pub messages: &'a [MessageRecord],
}

/// 明细与累计值是否一致：只有两边都有数据时才比较。
fn detail_matches(cumulative: Option<&RawTokenCounts>, detail: Option<&RawTokenCounts>) -> bool {
    match (cumulative, detail) {
        (Some(cumulative), Some(detail)) => cumulative == detail,
        _ => true,
    }
}

pub fn session(input: SessionInput<'_>) -> SessionRecord {
    let semantics = semantics_for(Client::Cline);
    // 累计值优先；缺失时退回明细求和。
    let raw = input.cumulative.or(input.detail);
    // 没有任何用量数据时，normalize_usage 会给出不可用总量与未知项。
    let mut usage = normalize_usage(semantics, &raw.unwrap_or_default());
    let matched = detail_matches(input.cumulative.as_ref(), input.detail.as_ref());
    if !matched {
        usage.unknown_relations.push(UNKNOWN_DETAIL_MISMATCH);
    }
    usage.unknown_relations.extend(
        input
            .messages
            .iter()
            .flat_map(|message| message.usage.unknown_relations.iter().copied()),
    );
    usage.unknown_relations.sort_unstable();
    usage.unknown_relations.dedup();
    let coverage = if input.messages.is_empty() {
        MessageCoverage::NoMessages
    } else if !matched {
        MessageCoverage::Partial
    } else {
        MessageCoverage::Complete
    };
    let last_ms = input
        .messages
        .iter()
        .filter_map(|message| message.completed_at_ms.or(message.created_at_ms))
        .max();
    let model = input.model.map(str::to_owned).or_else(|| {
        input
            .messages
            .iter()
            .filter(|message| message.model.is_some())
            .max_by_key(|message| message.created_at_ms)
            .and_then(|message| message.model.clone())
    });
    SessionRecord {
        client: Client::Cline,
        client_session_id: input.id.to_string(),
        parent_client_session_id: input.parent.map(str::to_owned),
        project_path: input.project.map(str::to_owned),
        model,
        started_at_ms: input.start_ms,
        ended_at_ms: input.end_ms.max(last_ms),
        semantics_id: semantics.id.to_string(),
        total_basis: if input.cumulative.is_some() {
            SessionTotalBasis::ClientCumulative
        } else {
            SessionTotalBasis::SumOfMessages
        },
        coverage,
        source_kind: if usage.total_basis == UsageTotalBasis::DerivedFromBuckets {
            SourceKind::Derived
        } else {
            SourceKind::Measured
        },
        usage,
        cost_source: CostSource::None,
        client_cost_usd: None,
    }
}
