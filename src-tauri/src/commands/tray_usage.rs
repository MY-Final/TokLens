//! Today's tray summary reuses request-time analytics without cumulative fallback.
use super::{AppState, CommandError};
use crate::usage::analytics::{AnalyticsFilter, RequestActivity, RequestTotals};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

// Registration and publication share the lock: a newer invocation cannot be
// registered between checking the generation and publishing the summary.
static GENERATION: Mutex<u64> = Mutex::new(0);
const UNAVAILABLE: &str = "今日用量暂不可用";

/// Clearing must invalidate a query that read the previous database snapshot.
pub(super) fn invalidate(app: &AppHandle) {
    let mut latest = GENERATION.lock().unwrap_or_else(|error| error.into_inner());
    *latest = latest.wrapping_add(1);
    crate::desktop::update_usage_summary(app, "今日用量加载中".into());
}

fn valid_bounds(from_ms: i64, to_ms: i64) -> bool {
    matches!(to_ms.checked_sub(from_ms), Some(span) if span > 0 && span <= 26 * 60 * 60 * 1_000)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn refresh_tray_usage(
    app: AppHandle,
    from_ms: i64,
    to_ms: i64,
) -> Result<(), CommandError> {
    let generation = {
        let mut latest = GENERATION.lock().unwrap_or_else(|error| error.into_inner());
        *latest = latest.wrapping_add(1);
        *latest
    };
    let result = if valid_bounds(from_ms, to_ms) {
        let query_app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let state = query_app.state::<AppState>();
            let store = state.store()?;
            let activity = store.request_activity(
                &AnalyticsFilter {
                    from_ms: Some(from_ms),
                    to_ms: Some(to_ms),
                    ..Default::default()
                },
                &[],
            )?;
            let last_scan_ms = store
                .client_source_status()?
                .iter()
                .filter_map(|source| source.last_scan_ms)
                .max();
            Ok::<_, CommandError>((format_activity(&activity), last_scan_ms))
        })
        .await
        .map_err(|_| CommandError::new(UNAVAILABLE))
        .and_then(|result| result)
    } else {
        Err(CommandError::new("今日日期范围无效"))
    };
    let latest = GENERATION.lock().unwrap_or_else(|error| error.into_inner());
    if *latest == generation {
        if let Ok((_, last_scan_ms)) = &result {
            crate::desktop::update_last_scan(&app, *last_scan_ms);
        }
        crate::desktop::update_usage_summary(
            &app,
            result
                .as_ref()
                .map(|(summary, _)| summary.clone())
                .unwrap_or_else(|_| UNAVAILABLE.into()),
        );
    }
    result.map(|_| ())
}

fn grouped(value: i64) -> String {
    let digits = value.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) && character != '-' {
            result.push(',');
        }
        result.push(character);
    }
    result
}

fn format_totals(totals: &RequestTotals) -> String {
    let tokens = if totals.requests == 0 {
        "未记录".into()
    } else if totals.requests_with_usage == 0 {
        "未知".into()
    } else {
        grouped(totals.sums.total_tokens)
    };
    let is_subset = totals.requests_with_usage > 0 && totals.requests_without_usage > 0;
    let label = if is_subset {
        "已知子集Token"
    } else {
        "已知Token"
    };
    let mut summary = format!(
        "今日（本地·按请求时间）\n{label}：{tokens}\n已记录请求：{}",
        totals.requests
    );
    if is_subset {
        summary.push_str(&format!("\n总量未知：{}条", totals.requests_without_usage));
    }
    summary
}

fn format_activity(activity: &RequestActivity) -> String {
    let mut summary = format_totals(&activity.range);
    if activity.coverage.untimed_requests > 0 || activity.coverage.estimated_requests > 0 {
        // Coverage is across all imported dates, not today's missing count.
        let warning = "\n全历史无时间/估算记录不计入";
        if summary.encode_utf16().count() + warning.encode_utf16().count() <= 100 {
            summary.push_str(warning);
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::analytics::TokenSums;

    #[test]
    fn distinguishes_no_records_unknown_zero_and_known_subset() {
        let mut totals = RequestTotals::default();
        assert!(format_totals(&totals).contains("Token：未记录"));
        totals.requests = 1;
        totals.requests_without_usage = 1;
        assert!(format_totals(&totals).contains("Token：未知"));
        totals.requests_with_usage = 1;
        totals.requests_without_usage = 0;
        assert!(format_totals(&totals).contains("Token：0"));
        totals.requests = 2;
        totals.requests_without_usage = 1;
        totals.sums = TokenSums {
            total_tokens: 123_456,
            ..Default::default()
        };
        let summary = format_totals(&totals);
        assert!(summary.contains("Token：123,456"));
        assert!(summary.contains("已知子集Token"));
        assert!(summary.contains("总量未知：1条"));
        totals.requests = i64::MAX;
        totals.requests_without_usage = i64::MAX - 1;
        totals.sums.total_tokens = i64::MAX;
        assert!(format_totals(&totals).encode_utf16().count() <= 100);
    }

    #[test]
    fn accepts_dst_days_and_rejects_invalid_or_overflowing_bounds() {
        assert!(valid_bounds(0, 23 * 60 * 60 * 1_000));
        assert!(valid_bounds(0, 25 * 60 * 60 * 1_000 + 60_000));
        assert!(!valid_bounds(1, 1));
        assert!(!valid_bounds(2, 1));
        assert!(!valid_bounds(i64::MIN, i64::MAX));
        assert!(!valid_bounds(0, 27 * 60 * 60 * 1_000));
    }
}
