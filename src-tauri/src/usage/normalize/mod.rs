//! 各客户端的归一化规则。
//!
//! 这里接收"客户端形状"的输入（与 `docs/data-source-research.md` 记录的字段一一对应），
//! 输出客户端无关的 [`SessionRecord`](crate::usage::model::SessionRecord) / [`MessageRecord`](crate::usage::model::MessageRecord)。本模块不做文件发现、不做读取，
//! 因此可以用合成数据完整覆盖去重、累计值、缺失字段等口径分支。

mod claude_code;
mod codex;
pub mod dsh;
mod opencode;
mod rollup;
pub use rollup::sum_complete_counts;

pub use claude_code::{
    ClaudeMessage, ClaudeSessionRollup, claude_code_message, claude_code_session,
};
pub use codex::{
    CodexThreadSnapshot, CodexUsageRecord, codex_dedup_key, codex_message, codex_session,
};
pub use opencode::{
    OpenCodeMessageRow, OpenCodeSessionRow, opencode_message, opencode_session,
    pick_opencode_session,
};
