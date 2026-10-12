//! Deserialize only native identity, time, model and usage metadata.
use crate::collectors::files::valid_counts;
use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Record {
    #[serde(rename = "type")]
    kind: Option<String>,
    pub id: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<i64>,
    pub cwd: Option<String>,
    role: Option<String>,
    #[serde(default)]
    pub is_sidechain: bool,
    pub provider_data: Option<ProviderData>,
    message: Option<Message>,
}

#[derive(Deserialize)]
pub(super) struct ProviderData {
    pub model: Option<String>,
    pub agent: Option<String>,
    usage: Option<Usage>,
}
#[derive(Deserialize)]
struct Message {
    usage: Option<Usage>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    requests: Option<u64>,
    #[serde(alias = "input_tokens")]
    input_tokens: Option<u64>,
    #[serde(alias = "output_tokens")]
    output_tokens: Option<u64>,
    #[serde(alias = "total_tokens")]
    total_tokens: Option<u64>,
    #[serde(alias = "cache_read_input_tokens")]
    cache_read_input_tokens: Option<u64>,
    #[serde(alias = "cache_creation_input_tokens")]
    cache_creation_input_tokens: Option<u64>,
    #[serde(alias = "reasoning_tokens")]
    reasoning_tokens: Option<u64>,
    input_tokens_details: Option<Details>,
    output_tokens_details: Option<Details>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Details {
    One(Detail),
    Many(Vec<Detail>),
}
#[derive(Deserialize)]
struct Detail {
    cached_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
}
impl Details {
    fn valid(&self) -> bool {
        let values: &[Detail] = match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        };
        [
            |v: &Detail| v.cached_tokens,
            |v: &Detail| v.reasoning_tokens,
        ]
        .into_iter()
        .all(|field| {
            values
                .iter()
                .filter_map(field)
                .try_fold(0u64, u64::checked_add)
                .is_some_and(|sum| sum <= i64::MAX as u64)
        })
    }
    fn sum(&self, field: fn(&Detail) -> Option<u64>) -> Option<u64> {
        match self {
            Self::One(value) => field(value),
            Self::Many(values) if !values.is_empty() => values
                .iter()
                .try_fold(0u64, |sum, value| sum.checked_add(field(value)?)),
            Self::Many(_) => None,
        }
    }
}
impl Usage {
    fn counts(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            client_total: self.total_tokens,
            cache_read: self
                .cache_read_input_tokens
                .or_else(|| self.input_tokens_details.as_ref()?.sum(|v| v.cached_tokens)),
            reasoning: self.reasoning_tokens.or_else(|| {
                self.output_tokens_details
                    .as_ref()?
                    .sum(|v| v.reasoning_tokens)
            }),
            cache_write: self.cache_creation_input_tokens,
        }
    }
}
impl Record {
    pub(super) fn counts(&self) -> Option<RawTokenCounts> {
        // The transcript message.usage mirrors providerData.usage; never add them.
        let original = self.provider_data.as_ref().and_then(|p| p.usage.as_ref());
        let mirror = self.message.as_ref().and_then(|m| m.usage.as_ref());
        original.or(mirror)?;
        let native = original.map_or(RawTokenCounts::default(), Usage::counts);
        let fallback = mirror.map_or(RawTokenCounts::default(), Usage::counts);
        Some(RawTokenCounts {
            input: native.input.or(fallback.input),
            output: native.output.or(fallback.output),
            client_total: native.client_total.or(fallback.client_total),
            cache_read: native.cache_read.or(fallback.cache_read),
            cache_write: native.cache_write.or(fallback.cache_write),
            reasoning: native.reasoning.or(fallback.reasoning),
        })
    }
    fn has_aggregate_usage(&self) -> bool {
        self.provider_data
            .as_ref()
            .and_then(|p| p.usage.as_ref())
            .or_else(|| self.message.as_ref().and_then(|m| m.usage.as_ref()))
            .is_some_and(|u| u.requests.is_some_and(|count| count != 1))
    }
    fn has_invalid_details(&self) -> bool {
        self.provider_data
            .as_ref()
            .and_then(|p| p.usage.as_ref())
            .into_iter()
            .chain(self.message.as_ref().and_then(|m| m.usage.as_ref()))
            .any(|u| {
                u.input_tokens_details.as_ref().is_some_and(|d| !d.valid())
                    || u.output_tokens_details.as_ref().is_some_and(|d| !d.valid())
            })
    }
}
pub(super) struct ParsedFile {
    pub records: Vec<Record>,
    pub unparsed: usize,
}
pub(super) fn read(path: &Path) -> std::io::Result<ParsedFile> {
    read_from(BufReader::new(std::fs::File::open(path)?))
}
pub(super) fn read_from(mut reader: impl BufRead) -> std::io::Result<ParsedFile> {
    let mut parsed = ParsedFile {
        records: Vec::new(),
        unparsed: 0,
    };
    let mut line = String::new();
    let mut recognized = false;
    let mut nonempty = false;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        nonempty = true;
        let record: Record = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(_) if !line.ends_with('\n') => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "incomplete WorkBuddy transcript record",
                ));
            }
            Err(_) => {
                parsed.unparsed += 1;
                continue;
            }
        };
        match record.kind.as_deref() {
            Some(
                "message"
                | "assistant"
                | "user"
                | "function_call"
                | "reasoning"
                | "function_call_result"
                | "function_call_output",
            ) => {
                recognized = true;
                if record.session_id.as_deref().is_none_or(str::is_empty) {
                    parsed.unparsed += 1;
                    continue;
                }
                if let Some(counts) = record.counts()
                    && (record.id.as_deref().is_none_or(str::is_empty)
                        || !matches!(
                            record.kind.as_deref(),
                            Some("message" | "assistant" | "function_call")
                        )
                        || record.role.as_deref() == Some("user")
                        || record.has_aggregate_usage()
                        || record.has_invalid_details()
                        || !valid_counts(&counts))
                {
                    parsed.unparsed += 1;
                    continue;
                }
                parsed.records.push(record);
            }
            Some("file-history-snapshot" | "ai-title" | "custom-title" | "summary") => {
                recognized = true;
            }
            _ => parsed.unparsed += 1,
        }
    }
    if nonempty && (!recognized || (parsed.unparsed > 0 && parsed.records.is_empty())) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported WorkBuddy transcript format",
        ));
    }
    Ok(parsed)
}
