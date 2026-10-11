//! Parse v0–v4 metadata, then exclude inherited events before folding request attempts.

use super::{
    Parsed, format,
    metadata::*,
    projection::{self, Event, EventKind},
};
use crate::collectors::files::valid_counts;
use std::io::{BufRead, BufReader};
use std::path::Path;

fn invalid() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unsupported or incomplete DSH session",
    )
}

pub(super) fn parse_file(path: &Path) -> std::io::Result<Parsed> {
    let file = std::fs::File::open(path)?;
    if path.extension().is_some_and(|ext| ext == "zstd") {
        parse_reader(
            BufReader::new(zstd::stream::read::Decoder::new(file)?),
            path,
        )
    } else {
        parse_reader(BufReader::new(file), path)
    }
}

fn parse_reader(mut reader: impl BufRead, path: &Path) -> std::io::Result<Parsed> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let header: Header = serde_json::from_str(&line).map_err(|_| invalid())?;
    if header.kind != "session"
        || header.version > 4
        || header.id.is_empty()
        || format::generation(path) != Some(header.version)
        || (header.version >= 2 && header.is_seeded.is_none())
        || header.seed_length.is_some_and(|value| value < 0)
    {
        return Err(invalid());
    }
    let mut unparsed = 0;
    let mut events = Vec::new();
    let mut seed_cut = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let tag: Tag = match serde_json::from_str(&line) {
            Ok(tag) => tag,
            Err(_) if !line.ends_with('\n') => return Err(invalid()),
            Err(_) => {
                unparsed += 1;
                continue;
            }
        };
        match tag.kind.as_str() {
            "session/end-seed" if header.version >= 2 => {
                let event: Envelope<Seed> = serde_json::from_str(&line).map_err(|_| invalid())?;
                if event.data.inherited == Some(true) {
                    seed_cut = Some(event.seq.filter(|seq| *seq >= 0).ok_or_else(invalid)?);
                }
            }
            "session/end-seed" => {}
            "request/header" => match serde_json::from_str::<Envelope<RequestHeader>>(&line) {
                Ok(event) => {
                    let config = event.data.header.config;
                    let model = config.model.map(|model| {
                        config
                            .provider
                            .map_or_else(|| model.clone(), |provider| format!("{provider}/{model}"))
                    });
                    events.push(Event {
                        seq: event.seq.unwrap_or(0),
                        time: event.time,
                        kind: EventKind::Model(model),
                    });
                }
                Err(_) => unparsed += 1,
            },
            "assistant/message" | "assistant/attempt" | "assistant/chunk" | "llm/retry-started" => {
                let event: Envelope<Assistant> = match serde_json::from_str(&line) {
                    Ok(event) => event,
                    Err(_) => {
                        unparsed += 1;
                        continue;
                    }
                };
                let (Some(seq), Some(turn), Some(step)) = (
                    event.seq.filter(|seq| *seq >= 0),
                    event.data.turn.filter(|value| *value >= 0),
                    event.data.step.filter(|value| *value >= 0),
                ) else {
                    unparsed += 1;
                    continue;
                };
                let kind = if tag.kind == "llm/retry-started" {
                    EventKind::Retry(turn, step)
                } else {
                    let sample = if tag.kind == "assistant/chunk" {
                        let Some(chunk) = event
                            .data
                            .chunk
                            .as_ref()
                            .filter(|chunk| chunk.kind == "usage")
                        else {
                            continue;
                        };
                        chunk.usage.as_ref()
                    } else {
                        event
                            .data
                            .usage
                            .as_ref()
                            .or_else(|| event.data.stream_usage())
                    };
                    let usage = sample.map(Usage::raw);
                    if usage.as_ref().is_some_and(|usage| !valid_counts(usage)) {
                        unparsed += 1;
                        continue;
                    }
                    EventKind::Sample {
                        turn,
                        step,
                        usage,
                        complete: tag.kind == "assistant/message",
                    }
                };
                events.push(Event {
                    seq,
                    time: event.time,
                    kind,
                });
            }
            // These are content, boundary or plugin events, never another source of assistant usage.
            "text-chunks"
            | "reasoning-chunks"
            | "tool-call-chunks"
            | "user/message"
            | "tool/call"
            | "tool/result"
            | "turn/start"
            | "turn/end"
            | "step/start"
            | "step/end"
            | "request/context"
            | "permission/preset"
            | "sandbox/mode"
            | "approval/policy"
            | "agent/inbox/spliced"
            | "session/title"
            | "session/title-llm-request"
            | "web/deepseek-search-llm-request"
            | "llm/retry"
            | "todo/write"
            | "command/run"
            | "command/done"
            | "approval/asked"
            | "approval/decided"
            | "compaction/start"
            | "compaction/summary"
            | "compaction/end"
            | "compaction/prune"
            | "system/message"
            | "model/selection"
            | "workspace/changes"
            | "session-log-deepseek/delivery-accepted"
            | "agent-preset/selected" => {}
            _ => unparsed += 1,
        }
    }
    let cut = if header.version < 2 {
        header.seed_length.unwrap_or(0)
    } else {
        match (header.is_seeded, seed_cut) {
            (Some(true), Some(cut)) => cut,
            (Some(false), None) => 0,
            _ => return Err(invalid()),
        }
    };
    Ok(projection::fold(header, events, cut, path, unparsed))
}
