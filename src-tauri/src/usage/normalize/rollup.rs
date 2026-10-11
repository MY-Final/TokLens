//! Complete-field rollups keep missing fields unknown.

use crate::usage::tokens::RawTokenCounts;

/// A missing field in any request keeps that session field unknown.
pub fn sum_complete_counts(items: &[RawTokenCounts]) -> RawTokenCounts {
    let sum = |field: fn(&RawTokenCounts) -> Option<u64>| {
        if items.is_empty() {
            return None;
        }
        items
            .iter()
            .try_fold(0u64, |sum, item| sum.checked_add(field(item)?))
            .filter(|sum| *sum <= i64::MAX as u64)
    };
    let result = RawTokenCounts {
        input: sum(|raw| raw.input),
        output: sum(|raw| raw.output),
        cache_read: sum(|raw| raw.cache_read),
        cache_write: sum(|raw| raw.cache_write),
        reasoning: sum(|raw| raw.reasoning),
        client_total: sum(|raw| raw.client_total),
    };
    let combined = [
        result.input,
        result.output,
        result.cache_read,
        result.cache_write,
        result.reasoning,
    ]
    .into_iter()
    .flatten()
    .try_fold(0u64, u64::checked_add);
    if combined.is_none_or(|value| value > i64::MAX as u64) {
        RawTokenCounts::default()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_zero_and_overflow_stay_distinct() {
        let full = RawTokenCounts {
            input: Some(1),
            output: Some(2),
            cache_read: Some(0),
            cache_write: Some(0),
            ..Default::default()
        };
        let missing = RawTokenCounts {
            output: None,
            ..full
        };
        assert_eq!(sum_complete_counts(&[full, missing]).input, Some(2));
        assert_eq!(sum_complete_counts(&[full, missing]).output, None);
        assert_eq!(sum_complete_counts(&[full, missing]).cache_read, Some(0));
        let huge = RawTokenCounts {
            input: Some(u64::MAX),
            ..full
        };
        assert_eq!(sum_complete_counts(&[huge, huge]).input, None);
        assert_eq!(sum_complete_counts(&[]), RawTokenCounts::default());
    }
}
