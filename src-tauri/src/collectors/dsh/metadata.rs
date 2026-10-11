//! Serde whitelist: message bodies, prompts and tool arguments are never materialized.

use crate::usage::tokens::RawTokenCounts;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Header {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u64,
    pub id: String,
    pub created_at: Option<i64>,
    pub cwd: Option<String>,
    pub parent_session: Option<String>,
    pub seed_length: Option<i64>,
    pub is_seeded: Option<bool>,
    pub origin: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Tag {
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Deserialize)]
pub(super) struct Envelope<T> {
    pub seq: Option<i64>,
    pub time: Option<i64>,
    pub data: T,
}

#[derive(Deserialize)]
pub(super) struct Assistant {
    pub turn: Option<i64>,
    pub step: Option<i64>,
    pub usage: Option<Usage>,
    pub chunk: Option<Chunk>,
    #[serde(default)]
    pub stream: Vec<StreamRecord>,
}

impl Assistant {
    pub fn stream_usage(&self) -> Option<&Usage> {
        self.stream.iter().rev().find_map(|record| {
            (record.kind == "chunk")
                .then_some(record.chunk.as_ref())
                .flatten()
                .filter(|chunk| chunk.kind == "usage")
                .and_then(|chunk| chunk.usage.as_ref())
        })
    }
}

#[derive(Deserialize)]
pub(super) struct StreamRecord {
    #[serde(rename = "type")]
    pub kind: String,
    pub chunk: Option<Chunk>,
}

#[derive(Deserialize)]
pub(super) struct Chunk {
    #[serde(rename = "type")]
    pub kind: String,
    pub usage: Option<Usage>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "optional_count")]
    cache_read_tokens: Option<u64>,
    #[serde(default, deserialize_with = "optional_count")]
    cache_write_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
}

fn optional_count<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    u64::deserialize(deserializer).map(Some)
}

impl Usage {
    pub fn raw(&self) -> RawTokenCounts {
        RawTokenCounts {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: Some(self.cache_read_tokens.unwrap_or(0)),
            cache_write: Some(self.cache_write_tokens.unwrap_or(0)),
            reasoning: self.reasoning_tokens,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Seed {
    pub inherited: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct RequestHeader {
    pub header: Request,
}
#[derive(Deserialize)]
pub(super) struct Request {
    pub config: Config,
}
#[derive(Deserialize)]
pub(super) struct Config {
    pub provider: Option<String>,
    pub model: Option<String>,
}
