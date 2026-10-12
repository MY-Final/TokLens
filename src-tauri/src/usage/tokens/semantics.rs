//! 字段关系与版本化客户端口径登记；证据见 docs/data-source-research.md。

use crate::usage::client::Client;

/// 一个字段相对另一个字段的关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRelation {
    /// 是另一个字段的子集：求和前必须扣除（例如 Codex 的 cache_read 属于 input）。
    IncludedIn,
    /// 与其它字段互不重叠：本机数据已证伪"包含"假设（例如存在 cache_read > input 的记录）。
    Separate,
    /// 客户端不提供该字段：缺失是预期行为，不算数据不完整。
    NotProvided,
    /// 无法判定：不计入总量，并在记录的 `unknown_relations` 中标注。
    Unknown,
}

/// 客户端口径标识与字段关系。
#[derive(Debug, Clone, Copy)]
pub struct TokenSemantics {
    /// 口径标识，随记录写入数据库，便于日后按口径重算。
    pub id: &'static str,
    /// `cache_read` 相对 `input` 的关系。
    pub cache_read: FieldRelation,
    /// `reasoning` 相对 `output` 的关系。
    pub reasoning: FieldRelation,
    /// `cache_write` 相对 `input` 的关系。
    pub cache_write_in_input: FieldRelation,
    /// `cache_write` 是否计入 TokLens 规范总量。
    pub cache_write_counts_in_total: bool,
    /// 客户端是否直接提供总量字段。
    pub client_reports_total: bool,
    /// 判定依据（本机实测数字，写入文档与代码注释）。
    pub basis: &'static str,
}

/// Codex（`token_usage_record` 与旧版 `event_msg.token_count` 语义一致）。
///
/// 依据：本机 19 385 条 `token_usage_record` 中 `total == input + output` 恒成立，
/// 且 `cached_input <= input`、`reasoning_output <= output` 恒成立，说明
/// cache_read 与 reasoning 分别是 input / output 的子集；`cache_write` 非 0 时
/// 总量仍等于 input + output，故不计入总量，其与 input 的关系无法判定。
pub const CODEX_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "codex.usage.v1",
    cache_read: FieldRelation::IncludedIn,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Unknown,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "local measurement: total == input + output for all 19385 records",
};

/// Claude Code（Anthropic `message.usage`）。
///
/// 依据：本机 4 956 条 assistant 记录中，`cache_read_input_tokens > input_tokens` 出现在
/// 3 927 条、`cache_creation_input_tokens > input_tokens` 出现在 112 条，
/// 二者不可能是 input 的子集；客户端不提供 reasoning 与总量字段。
pub const CLAUDE_CODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "claude_code.anthropic_usage.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::NotProvided,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: false,
    basis: "local measurement: cache_read > input on 3927/4956 records",
};

/// OpenCode（`message.data.tokens`）。
///
/// 依据：本机 5 490 条消息中 `total == input + output + reasoning + cache.read` 恒成立，
/// 且 `cache.read > input` 出现在 5 099 条，说明 cache_read 与 reasoning 均为独立字段；
/// `cache.write` 本机全为 0，与 input 的关系无法判定，故不计入总量。
pub const OPENCODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "opencode.message.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::Separate,
    cache_write_in_input: FieldRelation::Unknown,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "local measurement: total == input + output + reasoning + cache.read",
};

pub const DSH_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "dsh.token_usage.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: false,
    basis: "DSH token-meter contract: input/cache read/cache write disjoint; reasoning included in output",
};

/// Native provider adapters and addUsage/usageTotal sum four disjoint buckets.
pub const KIMI_CODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "kimi_code.usage_record.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::NotProvided,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: false,
    basis: "Kimi provider/addUsage contract: inputOther + output + inputCacheRead + inputCacheCreation",
};

/// MiMo's getUsage subtracts cache from input and reasoning from output before persistence.
pub const MIMOCODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "mimocode.message.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::Separate,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: true,
    basis: "installed MiMo getUsage contract: input/output/reasoning/cache are disjoint buckets",
};
/// Pi's usageFromPi/usageToPi sum input + output + both caches; reasoning is in output.
pub const PI_DESKTOP_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "pi_desktop.operation.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: true,
    basis: "Pi Desktop request accounting contract and local total == input + output + cacheRead + cacheWrite",
};

/// WorkBuddy's normalized provider usage keeps cache in input and reasoning in output.
pub const WORKBUDDY_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "workbuddy.item_usage.v1",
    cache_read: FieldRelation::IncludedIn,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Unknown,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "WorkBuddy 5.2.6 last response item contract; 781 local usage records total == input + output; cached_tokens is an input detail",
};

/// Canonical Copilot input excludes both caches after normalize::copilot conversion.
pub const COPILOT_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "copilot.assistant_usage.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: false,
    basis: "Copilot CLI 1.0.91 FJt accounting: native input includes read/write caches; reasoning is an output detail",
};

/// Gemini recorder stores prompt including cache; metrics restoration subtracts cached.
/// Thoughts are separate output, while tool-use tokens have no canonical bucket.
pub const GEMINI_CLI_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "gemini_cli.response.v1",
    cache_read: FieldRelation::IncludedIn,
    reasoning: FieldRelation::Separate,
    cache_write_in_input: FieldRelation::NotProvided,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "Gemini CLI recorder/metrics contract: prompt includes cached; 22 local positive thoughts exceed candidates; tool relation unverified",
};

pub const GROK_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "grok.turn.v1",
    cache_read: FieldRelation::IncludedIn,
    reasoning: FieldRelation::IncludedIn,
    cache_write_in_input: FieldRelation::Unknown,
    cache_write_counts_in_total: false,
    client_reports_total: true,
    basis: "Grok usage display contract: input includes cached/output includes reasoning; three positive local turns total == input + output; cache creation unverified",
};

pub const MCODE_SEMANTICS: TokenSemantics = TokenSemantics {
    id: "mcode.request.v1",
    cache_read: FieldRelation::Separate,
    reasoning: FieldRelation::Unknown,
    cache_write_in_input: FieldRelation::Separate,
    cache_write_counts_in_total: true,
    client_reports_total: false,
    basis: "MCode local-runtime usage API sums four disjoint buckets; 16 local records cache read > input; nonzero reasoning relation unverified",
};

pub fn semantics_for(client: Client) -> &'static TokenSemantics {
    match client {
        Client::Codex => &CODEX_SEMANTICS,
        Client::ClaudeCode => &CLAUDE_CODE_SEMANTICS,
        Client::OpenCode => &OPENCODE_SEMANTICS,
        Client::Dsh => &DSH_SEMANTICS,
        Client::KimiCode => &KIMI_CODE_SEMANTICS,
        Client::MimoCode => &MIMOCODE_SEMANTICS,
        Client::PiDesktop => &PI_DESKTOP_SEMANTICS,
        Client::WorkBuddy => &WORKBUDDY_SEMANTICS,
        Client::Copilot => &COPILOT_SEMANTICS,
        Client::GeminiCli => &GEMINI_CLI_SEMANTICS,
        Client::Grok => &GROK_SEMANTICS,
        Client::Mcode => &MCODE_SEMANTICS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantics_are_registered_for_every_client() {
        for client in Client::ALL {
            let semantics = semantics_for(client);
            assert!(!semantics.id.is_empty());
            assert!(!semantics.basis.is_empty());
        }
    }
}
