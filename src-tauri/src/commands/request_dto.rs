//! Request-time statistics and privacy-preserving session detail contracts.
use serde::Serialize;

use super::dto::{SessionSummaryDto, TokenTotalsDto};
use crate::usage::analytics::{
    RequestActivity, RequestAnalysis, RequestAnalysisGroup, RequestAnalysisMetrics,
    RequestCoverage, RequestCoverageDetail, RequestDailyBucket, RequestDetail, RequestTotals,
    SessionDetail,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestAnalysisMetricsDto {
    pub totals: RequestTotalsDto,
    pub average_tokens: Option<f64>,
    pub share_bps: Option<i64>,
    pub cache_hit_bps: Option<i64>,
    pub cache_known_requests: i64,
    pub known_token_fields: Vec<String>,
}
impl From<RequestAnalysisMetrics> for RequestAnalysisMetricsDto {
    fn from(value: RequestAnalysisMetrics) -> Self {
        Self {
            totals: value.totals.into(),
            average_tokens: value.average_tokens,
            share_bps: value.share_bps,
            cache_hit_bps: value.cache_hit_bps,
            cache_known_requests: value.cache_known_requests,
            known_token_fields: value.known_token_fields,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestAnalysisGroupDto {
    pub key: Option<String>,
    pub metrics: RequestAnalysisMetricsDto,
}
impl From<RequestAnalysisGroup> for RequestAnalysisGroupDto {
    fn from(value: RequestAnalysisGroup) -> Self {
        Self {
            key: value.key,
            metrics: value.metrics.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestAnalysisDto {
    pub summary: RequestAnalysisMetricsDto,
    pub clients: Vec<RequestAnalysisGroupDto>,
    pub models: Vec<RequestAnalysisGroupDto>,
    pub projects: Vec<RequestAnalysisGroupDto>,
    pub coverage: RequestCoverageDto,
}
impl From<RequestAnalysis> for RequestAnalysisDto {
    fn from(value: RequestAnalysis) -> Self {
        Self {
            summary: value.summary.into(),
            clients: value.clients.into_iter().map(Into::into).collect(),
            models: value.models.into_iter().map(Into::into).collect(),
            projects: value.projects.into_iter().map(Into::into).collect(),
            coverage: value.coverage.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestTotalsDto {
    pub sessions: i64,
    pub requests: i64,
    pub requests_with_usage: i64,
    pub requests_without_usage: i64,
    pub measured_requests: i64,
    pub derived_requests: i64,
    pub totals: TokenTotalsDto,
}
impl From<RequestTotals> for RequestTotalsDto {
    fn from(value: RequestTotals) -> Self {
        Self {
            sessions: value.sessions,
            requests: value.requests,
            requests_with_usage: value.requests_with_usage,
            requests_without_usage: value.requests_without_usage,
            measured_requests: value.measured_requests,
            derived_requests: value.derived_requests,
            totals: value.sums.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDailyClientDto {
    pub client: String,
    pub totals: RequestTotalsDto,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDailyBucketDto {
    pub day_start_ms: i64,
    pub totals: RequestTotalsDto,
    pub clients: Vec<RequestDailyClientDto>,
}
impl From<RequestDailyBucket> for RequestDailyBucketDto {
    fn from(value: RequestDailyBucket) -> Self {
        Self {
            day_start_ms: value.day_start_ms,
            totals: value.totals.into(),
            clients: value
                .clients
                .into_iter()
                .map(|entry| RequestDailyClientDto {
                    client: entry.client.as_str().into(),
                    totals: entry.totals.into(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestCoverageDto {
    pub recorded_requests: i64,
    pub timed_requests: i64,
    pub untimed_requests: i64,
    pub estimated_requests: i64,
    pub sessions_partial: i64,
    pub sessions_without_requests: i64,
}
impl From<RequestCoverage> for RequestCoverageDto {
    fn from(value: RequestCoverage) -> Self {
        Self {
            recorded_requests: value.recorded_requests,
            timed_requests: value.timed_requests,
            untimed_requests: value.untimed_requests,
            estimated_requests: value.estimated_requests,
            sessions_partial: value.sessions_partial,
            sessions_without_requests: value.sessions_without_requests,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestActivityDto {
    pub range: RequestTotalsDto,
    pub daily: Vec<RequestDailyBucketDto>,
    pub coverage: RequestCoverageDto,
}
impl From<RequestActivity> for RequestActivityDto {
    fn from(value: RequestActivity) -> Self {
        Self {
            range: value.range.into(),
            daily: value.daily.into_iter().map(Into::into).collect(),
            coverage: value.coverage.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetailDto {
    pub dedup_key: String,
    pub seq: Option<i64>,
    pub created_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub is_sidechain: bool,
    pub unknown_relations: Vec<String>,
}
impl From<RequestDetail> for RequestDetailDto {
    fn from(value: RequestDetail) -> Self {
        Self {
            dedup_key: value.dedup_key,
            seq: value.seq,
            created_at_ms: value.created_at_ms,
            completed_at_ms: value.completed_at_ms,
            model: value.model,
            agent: value.agent,
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            reasoning_tokens: value.reasoning_tokens,
            cache_read_tokens: value.cache_read_tokens,
            cache_write_tokens: value.cache_write_tokens,
            total_tokens: value.total_tokens,
            source_kind: value.source_kind,
            source_ref: value.source_ref,
            is_sidechain: value.is_sidechain,
            unknown_relations: value.unknown_relations,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestCoverageDetailDto {
    pub total: i64,
    pub timed: i64,
    pub untimed: i64,
    pub with_usage: i64,
    pub without_usage: i64,
    pub estimated: i64,
    pub partial: bool,
}
impl From<RequestCoverageDetail> for RequestCoverageDetailDto {
    fn from(value: RequestCoverageDetail) -> Self {
        Self {
            total: value.total,
            timed: value.timed,
            untimed: value.untimed,
            with_usage: value.with_usage,
            without_usage: value.without_usage,
            estimated: value.estimated,
            partial: value.partial,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetailDto {
    pub session: SessionSummaryDto,
    pub requests: Vec<RequestDetailDto>,
    pub request_coverage: RequestCoverageDetailDto,
}
impl From<SessionDetail> for SessionDetailDto {
    fn from(value: SessionDetail) -> Self {
        Self {
            session: value.session.into(),
            requests: value.requests.into_iter().map(Into::into).collect(),
            request_coverage: value.request_coverage.into(),
        }
    }
}
