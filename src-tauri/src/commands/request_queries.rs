//! Thin query boundary for request activity and session detail.
use super::dto::ClientParam;
use super::request_dto::{RequestActivityDto, SessionDetailDto};
use super::{AppState, CommandError};
use crate::usage::analytics::AnalyticsFilter;
use tauri::State;

#[tauri::command(rename_all = "camelCase")]
pub fn get_request_activity(
    state: State<'_, AppState>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    client: Option<ClientParam>,
    day_boundaries: Vec<i64>,
) -> Result<RequestActivityDto, CommandError> {
    if matches!((from_ms, to_ms), (Some(from), Some(to)) if from >= to)
        || day_boundaries.len() < 2
        || day_boundaries.len() > 401
        || day_boundaries.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(CommandError::new("日期范围无效，最多显示 400 个连续日桶。"));
    }
    let filter = AnalyticsFilter {
        from_ms,
        to_ms,
        client: client.map(Into::into),
        project_path: None,
    };
    Ok(state
        .store()?
        .request_activity(&filter, &day_boundaries)?
        .into())
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_session_detail(
    state: State<'_, AppState>,
    client: ClientParam,
    client_session_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<SessionDetailDto, CommandError> {
    if client_session_id.is_empty() {
        return Err(CommandError::new("请选择一个会话。"));
    }
    let filter = AnalyticsFilter {
        client: Some(client.into()),
        ..Default::default()
    };
    Ok(state
        .store()?
        .session_detail(
            &filter,
            &client_session_id,
            limit.unwrap_or(100).clamp(1, 200),
            offset.unwrap_or(0),
        )?
        .into())
}
