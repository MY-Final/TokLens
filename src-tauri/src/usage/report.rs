//! Metadata-only statistics report and safe local renderers.
use crate::usage::store::UsageError;
use serde::{Deserialize, Serialize};

pub const METRICS_VERSION: &str = "2.12";

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Csv,
    Json,
    Markdown,
}
impl ReportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportMetadata {
    pub schema_version: u32,
    pub metrics_version: String,
    pub from_ms: Option<i64>,
    pub to_ms: i64,
    pub client: Option<String>,
    pub include_estimated: bool,
    pub date_basis: String,
    pub model_basis: String,
    pub generated_at_ms: i64,
    pub untimed_requests: i64,
    pub excluded_estimated_requests: i64,
    pub notes: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRow {
    pub date: String,
    pub client: String,
    pub model: Option<String>,
    pub project: Option<String>,
    pub source_kind: String,
    pub requests: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub requests_without_usage: i64,
    pub unknown_token_fields: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct StatisticsReport {
    pub metadata: ReportMetadata,
    pub rows: Vec<ReportRow>,
}
fn json(value: &impl Serialize) -> Result<serde_json::Value, UsageError> {
    serde_json::to_value(value)
        .map_err(|_| UsageError::InvalidInput("Cannot serialize statistics report".into()))
}
fn scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(scalar).collect::<Vec<_>>().join("; "),
        _ => value.to_string(),
    }
}
fn csv_cell(value: &str) -> String {
    let trimmed = value.trim_start_matches(|c: char| c.is_whitespace() || c.is_control());
    let safe = if trimmed.starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.into()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
fn markdown_cell(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '|' => "&#124;".into(),
            '`' => "&#96;".into(),
            '\n' | '\r' => " ".into(),
            '\\' => "&#92;".into(),
            '*' => "&#42;".into(),
            '_' => "&#95;".into(),
            '[' => "&#91;".into(),
            ']' => "&#93;".into(),
            _ if c.is_control() => " ".into(),
            _ => c.to_string(),
        })
        .collect()
}
impl StatisticsReport {
    pub fn render(&self, format: ReportFormat) -> Result<String, UsageError> {
        if matches!(format, ReportFormat::Json) {
            return serde_json::to_string_pretty(self).map_err(|_| {
                UsageError::InvalidInput("Cannot serialize statistics report".into())
            });
        }
        let meta = json(&self.metadata)?;
        let meta = meta
            .as_object()
            .ok_or_else(|| UsageError::InvalidInput("Invalid report metadata".into()))?;
        let fields = [
            "date",
            "client",
            "model",
            "project",
            "sourceKind",
            "requests",
            "inputTokens",
            "outputTokens",
            "reasoningTokens",
            "cacheReadTokens",
            "cacheWriteTokens",
            "totalTokens",
            "requestsWithoutUsage",
            "unknownTokenFields",
        ];
        if matches!(format, ReportFormat::Csv) {
            let mut headers = vec!["recordType".to_string()];
            headers.extend(meta.keys().map(|key| format!("metadata.{key}")));
            headers.extend(fields.iter().map(|s| s.to_string()));
            let mut output = String::from("\u{feff}");
            let mut write_record = |cells: Vec<String>| {
                output.push_str(
                    &cells
                        .iter()
                        .map(|s| csv_cell(s))
                        .collect::<Vec<_>>()
                        .join(","),
                );
                output.push_str("\r\n");
            };
            write_record(headers);
            let metadata_cells = meta.values().map(scalar).collect::<Vec<_>>();
            let mut cells = vec!["metadata".into()];
            cells.extend(metadata_cells.clone());
            cells.extend(vec![String::new(); fields.len()]);
            write_record(cells);
            for row in &self.rows {
                let row = json(row)?;
                let mut cells = vec!["data".into()];
                cells.extend(metadata_cells.clone());
                cells.extend(fields.iter().map(|field| scalar(&row[*field])));
                write_record(cells);
            }
            return Ok(output);
        }
        let mut output = String::from("# TokLens statistics report\n\n");
        for (key, value) in meta {
            output.push_str(&format!(
                "{}: {}\n\n",
                markdown_cell(key),
                markdown_cell(&scalar(value))
            ));
        }
        output.push_str(&format!(
            "| {} |\n| {} |\n",
            fields.join(" | "),
            vec!["---"; fields.len()].join(" | ")
        ));
        for row in &self.rows {
            let row = json(row)?;
            output.push_str(&format!(
                "| {} |\n",
                fields
                    .iter()
                    .map(|field| markdown_cell(&scalar(&row[*field])))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::csv_cell;

    #[test]
    fn spreadsheet_formula_prefixes_are_neutralized_after_whitespace() {
        for prefix in ['=', '+', '-', '@'] {
            let value = format!(" \t\r\n{prefix}payload");
            assert!(csv_cell(&value).starts_with("\"'"));
        }
        assert_eq!(csv_cell("ordinary,\"text\""), "\"ordinary,\"\"text\"\"\"");
    }
}
