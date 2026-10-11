//! opencode record normalization.

use crate::usage::model::{
    CostSource, MessageCoverage, MessageRecord, SessionRecord, SessionTotalBasis, SourceKind,
};
use crate::usage::tokens::{Client, RawTokenCounts, normalize_usage, semantics_for};

/// OpenCode 消息行（`message` 表 `data` 的字段子集）。
#[derive(Debug, Clone)]
pub struct OpenCodeMessageRow<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub model_id: Option<&'a str>,
    pub provider_id: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub created_ms: Option<i64>,
    pub completed_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub usage: RawTokenCounts,
}

/// OpenCode 请求级归一化：去重键取 `message.id`。
pub fn opencode_message(row: &OpenCodeMessageRow<'_>) -> MessageRecord {
    let semantics = semantics_for(Client::OpenCode);
    let model = match (row.model_id, row.provider_id) {
        (Some(model), Some(provider)) => Some(format!("{provider}/{model}")),
        (Some(model), None) => Some(model.to_string()),
        (None, _) => None,
    };
    let cost_source = if row.cost_usd.is_some() {
        CostSource::ClientReported
    } else {
        CostSource::None
    };
    MessageRecord {
        client: Client::OpenCode,
        client_session_id: row.session_id.to_string(),
        dedup_key: row.id.to_string(),
        seq: None,
        created_at_ms: row.created_ms,
        completed_at_ms: row.completed_ms,
        model,
        agent: row.agent.map(str::to_string),
        usage: normalize_usage(semantics, &row.usage),
        is_sidechain: false,
        semantics_id: semantics.id.to_string(),
        source_kind: SourceKind::Measured,
        source_ref: Some(format!("message_id={}", row.id)),
        cost_source,
        client_cost_usd: row.cost_usd,
    }
}

/// OpenCode 会话行（`session` 或 `session_v2`）。
#[derive(Debug, Clone)]
pub struct OpenCodeSessionRow<'a> {
    pub id: &'a str,
    pub parent_id: Option<&'a str>,
    pub directory: Option<&'a str>,
    pub model: Option<&'a str>,
    pub created_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub usage: RawTokenCounts,
}

/// `session` 与 `session_v2` 双表兼容：按 `time_updated` 取更新的行；时间相同或缺失时取 `session_v2`。
///
/// 依据：本机 47 行重叠且 token 列完全一致，仅 3 行独有，无法判定哪张表更权威，
/// 因此采用"更新时间优先、平局取新表"的确定性规则，并把两表视为同一会话的两个版本，
/// **绝不把两表的值相加**。
pub fn pick_opencode_session<'a>(
    legacy: Option<&'a OpenCodeSessionRow<'a>>,
    current: Option<&'a OpenCodeSessionRow<'a>>,
) -> Option<&'a OpenCodeSessionRow<'a>> {
    match (legacy, current) {
        (Some(legacy), Some(current)) => {
            let legacy_updated = legacy.updated_ms.unwrap_or(i64::MIN);
            let current_updated = current.updated_ms.unwrap_or(i64::MIN);
            if legacy_updated > current_updated {
                Some(legacy)
            } else {
                Some(current)
            }
        }
        (Some(legacy), None) => Some(legacy),
        (None, Some(current)) => Some(current),
        (None, None) => None,
    }
}

/// OpenCode 会话归一化：总量使用客户端聚合列（与消息求和一致，二者不可相加）。
pub fn opencode_session(row: &OpenCodeSessionRow<'_>) -> SessionRecord {
    let semantics = semantics_for(Client::OpenCode);
    let parent = row.parent_id.filter(|parent| !parent.is_empty());
    let cost_source = if row.cost_usd.is_some() {
        CostSource::ClientReported
    } else {
        CostSource::None
    };
    SessionRecord {
        client: Client::OpenCode,
        client_session_id: row.id.to_string(),
        parent_client_session_id: parent.map(str::to_string),
        project_path: row.directory.map(str::to_string),
        model: row.model.map(str::to_string),
        started_at_ms: row.created_ms,
        ended_at_ms: row.updated_ms,
        semantics_id: semantics.id.to_string(),
        total_basis: SessionTotalBasis::ClientReported,
        // 明细由 `message` 表提供；完整度由采集器按实际导入结果给出，这里默认完整。
        coverage: MessageCoverage::Complete,
        usage: normalize_usage(semantics, &row.usage),
        source_kind: SourceKind::Measured,
        cost_source,
        client_cost_usd: row.cost_usd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_message_merges_provider_and_model_and_keeps_client_cost() {
        let row = OpenCodeMessageRow {
            id: "msg_abc",
            session_id: "ses_ee3e1b96",
            model_id: Some("deepseek-v4.1-flash"),
            provider_id: Some("opencode-go"),
            agent: Some("build"),
            created_ms: Some(1_786_371_183_576),
            completed_ms: Some(1_786_371_183_900),
            cost_usd: Some(0.0123),
            usage: RawTokenCounts {
                input: Some(288_249),
                output: Some(13_418),
                cache_read: Some(756_224),
                cache_write: Some(0),
                reasoning: Some(1_006),
                client_total: Some(288_249 + 13_418 + 756_224 + 1_006),
            },
        };
        let message = opencode_message(&row);
        assert_eq!(message.dedup_key, "msg_abc");
        assert_eq!(
            message.model.as_deref(),
            Some("opencode-go/deepseek-v4.1-flash")
        );
        assert_eq!(message.agent.as_deref(), Some("build"));
        assert_eq!(message.cost_source, CostSource::ClientReported);
        assert_eq!(message.client_cost_usd, Some(0.0123));
        // reasoning 独立于 output，不能被扣减。
        assert_eq!(message.usage.buckets.output, Some(13_418));
        assert_eq!(message.usage.buckets.reasoning, Some(1_006));
        assert!(!message.usage.client_total_mismatch);
    }

    #[test]
    fn opencode_message_without_cost_stores_no_cost() {
        let row = OpenCodeMessageRow {
            id: "msg_def",
            session_id: "ses_1",
            model_id: None,
            provider_id: None,
            agent: None,
            created_ms: None,
            completed_ms: None,
            cost_usd: None,
            usage: RawTokenCounts::default(),
        };
        let message = opencode_message(&row);
        assert_eq!(message.cost_source, CostSource::None);
        assert!(message.client_cost_usd.is_none());
        assert_eq!(message.model, None);
    }

    #[test]
    fn opencode_session_picks_newest_row_and_never_adds_the_two_tables() {
        let legacy = OpenCodeSessionRow {
            id: "ses_1",
            parent_id: None,
            directory: Some("D:\\Repos\\a"),
            model: Some("{}"),
            created_ms: Some(1_786_371_183_576),
            updated_ms: Some(1_791_473_749_139),
            cost_usd: None,
            usage: RawTokenCounts {
                input: Some(288_249),
                output: Some(13_418),
                cache_read: Some(756_224),
                cache_write: Some(0),
                reasoning: Some(1_006),
                client_total: Some(1_058_897),
            },
        };
        let current = OpenCodeSessionRow {
            updated_ms: Some(1_791_351_562_546),
            ..legacy.clone()
        };
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&legacy), Some(&current)).unwrap(),
            &legacy
        ));
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&current), Some(&legacy)).unwrap(),
            &legacy
        ));
        // 时间相同时取新表（session_v2）。
        let tie = OpenCodeSessionRow {
            updated_ms: legacy.updated_ms,
            ..legacy.clone()
        };
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&legacy), Some(&tie)).unwrap(),
            &tie
        ));
        // 只有一张表时用存在的那个。
        assert!(std::ptr::eq(
            pick_opencode_session(None, Some(&current)).unwrap(),
            &current
        ));
        assert!(std::ptr::eq(
            pick_opencode_session(Some(&legacy), None).unwrap(),
            &legacy
        ));
        assert!(pick_opencode_session(None, None).is_none());

        let session = opencode_session(&legacy);
        assert_eq!(session.total_basis, SessionTotalBasis::ClientReported);
        assert_eq!(session.usage.total_tokens, Some(1_058_897));
        assert_eq!(session.ended_at_ms, Some(1_791_473_749_139));
    }

    #[test]
    fn opencode_child_session_keeps_parent_for_dedup_review() {
        let row = OpenCodeSessionRow {
            id: "ses_child",
            parent_id: Some("ses_parent"),
            directory: None,
            model: None,
            created_ms: Some(1),
            updated_ms: Some(2),
            cost_usd: Some(0.5),
            usage: RawTokenCounts::default(),
        };
        let session = opencode_session(&row);
        assert_eq!(
            session.parent_client_session_id.as_deref(),
            Some("ses_parent")
        );
        assert_eq!(session.cost_source, CostSource::ClientReported);
        assert_eq!(session.client_cost_usd, Some(0.5));
    }
}
