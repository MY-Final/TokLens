//! Expand physical operations and never add their enclosing aggregate twice.
use crate::collectors::files::valid_counts;
use crate::usage::{normalize::sum_complete_counts, tokens::RawTokenCounts};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Usage {
    pub operation_id: Option<String>,
    pub aggregation: Option<String>,
    pub usage_origin: Option<String>,
    pub model_id: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    // Optional fields have protocol-default zero; explicit null/negative is invalid.
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    cache_write_tokens: u64,
    #[serde(default)]
    operations: Vec<Usage>,
}
impl Usage {
    pub fn raw(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            reasoning: self.reasoning_tokens,
            cache_read: Some(self.cache_read_tokens),
            cache_write: Some(self.cache_write_tokens),
            client_total: self.total_tokens,
        }
    }
    fn valid(&self) -> bool {
        valid_counts(&self.raw())
            && self
                .usage_origin
                .as_deref()
                .is_none_or(|origin| matches!(origin, "pi" | "legacy"))
            && self
                .aggregation
                .as_deref()
                .is_none_or(|scope| matches!(scope, "operation" | "aggregate"))
    }
    pub fn expand(
        &self,
        message_id: &str,
    ) -> Option<Vec<(String, Option<String>, RawTokenCounts)>> {
        if !self.valid() {
            return None;
        }
        if self.operations.is_empty() {
            return Some(vec![(
                self.operation_id
                    .as_ref()
                    .filter(|id| !id.is_empty())
                    .map_or_else(
                        || format!("message:{message_id}"),
                        |id| format!("operation:{id}"),
                    ),
                self.model_id.clone(),
                self.raw(),
            )]);
        }
        if self.aggregation.as_deref() != Some("aggregate") {
            return None;
        }
        let mut operations = std::collections::BTreeMap::new();
        for op in &self.operations {
            let id = op.operation_id.as_ref().filter(|id| !id.is_empty())?;
            if !op.valid() || !op.operations.is_empty() {
                return None;
            }
            let item = (op.model_id.clone(), op.raw());
            if operations
                .get(id)
                .is_none_or(|(_, raw): &(Option<String>, RawTokenCounts)| {
                    item.1.client_total > raw.client_total
                })
            {
                operations.insert(id, item);
            }
        }
        let sum =
            sum_complete_counts(&operations.values().map(|(_, raw)| *raw).collect::<Vec<_>>());
        let raw = self.raw();
        let difference = |whole: Option<u64>, part: Option<u64>| -> Option<Option<u64>> {
            match (whole, part) {
                (Some(whole), Some(part)) => Some(Some(whole.checked_sub(part)?)),
                (None, None) => Some(None),
                _ => None,
            }
        };
        let residual = RawTokenCounts {
            input: difference(raw.input, sum.input)?,
            output: difference(raw.output, sum.output)?,
            cache_read: difference(raw.cache_read, sum.cache_read)?,
            cache_write: difference(raw.cache_write, sum.cache_write)?,
            reasoning: difference(raw.reasoning, sum.reasoning)?,
            client_total: difference(raw.client_total, sum.client_total)?,
        };
        let mut result: Vec<_> = operations
            .into_iter()
            .map(|(id, (model, raw))| (format!("operation:{id}"), model, raw))
            .collect();
        // Legacy unattributed usage remains additive, as in Pi's native ledger contract.
        if [
            residual.input,
            residual.output,
            residual.cache_read,
            residual.cache_write,
            residual.reasoning,
            residual.client_total,
        ]
        .into_iter()
        .flatten()
        .any(|n| n > 0)
        {
            result.push((
                format!("message:{message_id}:residual"),
                self.model_id.clone(),
                residual,
            ));
        }
        Some(result)
    }
}
