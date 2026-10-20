//! 把 Cline 的清单与消息文件合成一条会话记录与若干逐轮用量明细。

use super::parser::{Manifest, MessagesFile};
use crate::usage::normalize::cline::{self, SessionInput};
use crate::usage::normalize::request_ledger::RequestMeta;
use crate::usage::normalize::sum_complete_counts;
use crate::usage::store::SessionImport;
use crate::usage::time::parse_iso8601_utc_ms;
use crate::usage::tokens::RawTokenCounts;
use std::collections::BTreeMap;

/// 合成结果：会话导入内容与无法解析的记录数。
pub(super) struct Built {
    pub import: SessionImport,
    /// 缺少标识或字段而无法归属的记录数。
    pub unparsed: usize,
}

/// 合成一个会话。缺少会话标识时返回 `None`（无法归属，不写库）。
pub(super) fn build(
    stem: &str,
    manifest: Option<Manifest>,
    messages: Option<MessagesFile>,
    manifest_path: Option<String>,
    messages_path: Option<String>,
) -> Option<Built> {
    let id = manifest
        .as_ref()
        .and_then(|manifest| manifest.session_id.clone())
        .or_else(|| {
            messages
                .as_ref()
                .and_then(|messages| messages.session_id.clone())
        })
        .unwrap_or_else(|| stem.to_string());
    if id.trim().is_empty() {
        return None;
    }
    let mut unparsed = 0usize;
    let mut records: BTreeMap<String, (RequestMeta, RawTokenCounts, Option<u64>)> = BTreeMap::new();
    // 没有消息文件就没有明细，明细的源文件缺失时按会话源文件回退。
    if let Some(file) = &messages {
        for message in &file.messages {
            let Some(metrics) = &message.metrics else {
                continue;
            };
            let Some(key) = message.id.clone().filter(|id| !id.trim().is_empty()) else {
                unparsed += 1;
                continue;
            };
            let meta = RequestMeta {
                key: key.clone(),
                model: message
                    .model_info
                    .as_ref()
                    .and_then(|info| info.id.clone())
                    .filter(|model| !model.trim().is_empty()),
                agent: file.agent.clone(),
                created_ms: message.ts,
                completed_ms: message.ts,
                sidechain: false,
                source_ref: format!("message:{key}"),
            };
            let raw = metrics.counts();
            // 同一标识重复出现时保留时间较晚的一条。
            let replace = records.get(&key).is_none_or(|(old, _, _)| {
                meta.created_ms.unwrap_or(i64::MIN) >= old.created_ms.unwrap_or(i64::MIN)
            });
            if replace {
                records.insert(key, (meta, raw, metrics.reasoning_token_count));
            }
        }
    }
    let mut message_records = Vec::new();
    let mut detail_counts = Vec::new();
    let mut message_source_paths = BTreeMap::new();
    for (key, (meta, raw, reasoning)) in records {
        let record = cline::request(&id, meta, &raw, reasoning);
        detail_counts.push(raw);
        if let Some(source) = messages_path.as_deref() {
            message_source_paths.insert(key, source.to_string());
        }
        message_records.push(record);
    }
    let detail = if detail_counts.is_empty() {
        None
    } else {
        Some(sum_complete_counts(&detail_counts))
    };
    let cumulative = manifest
        .as_ref()
        .and_then(|manifest| manifest.metadata.as_ref())
        .and_then(|metadata| metadata.usage.as_ref())
        .and_then(|usage| usage.counts());
    let parent = messages
        .as_ref()
        .and_then(|messages| messages.origin.as_ref())
        .and_then(|origin| origin.parent_thread_id.clone());
    let project = manifest.as_ref().and_then(|manifest| {
        manifest
            .cwd
            .clone()
            .or_else(|| manifest.workspace_root.clone())
    });
    let model = manifest
        .as_ref()
        .and_then(|manifest| manifest.model.clone())
        .filter(|model| !model.trim().is_empty());
    let start_ms = manifest
        .as_ref()
        .and_then(|manifest| manifest.started_at.as_deref())
        .and_then(parse_iso8601_utc_ms);
    let end_ms = manifest
        .as_ref()
        .and_then(|manifest| manifest.ended_at.as_deref())
        .and_then(parse_iso8601_utc_ms);
    let session = cline::session(SessionInput {
        id: &id,
        parent: parent.as_deref(),
        project: project.as_deref(),
        model: model.as_deref(),
        start_ms,
        end_ms,
        cumulative,
        detail,
        messages: &message_records,
    });
    let source_path = manifest_path
        .or_else(|| messages_path.clone())
        .unwrap_or_default();
    Some(Built {
        import: SessionImport {
            session,
            messages: message_records,
            source_path,
            message_source_paths,
        },
        unparsed,
    })
}
