//! 客户端标识与稳定字符串映射，不包含 Token 统计口径。

use std::fmt;

/// TokLens 支持的客户端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Client {
    Codex,
    ClaudeCode,
    OpenCode,
    Dsh,
    KimiCode,
}

impl Client {
    pub const ALL: [Client; 5] = [
        Client::Codex,
        Client::ClaudeCode,
        Client::OpenCode,
        Client::Dsh,
        Client::KimiCode,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Client::Codex => "codex",
            Client::ClaudeCode => "claude_code",
            Client::OpenCode => "opencode",
            Client::Dsh => "dsh",
            Client::KimiCode => "kimi_code",
        }
    }

    pub fn parse(value: &str) -> Option<Client> {
        match value {
            "codex" => Some(Client::Codex),
            "claude_code" => Some(Client::ClaudeCode),
            "opencode" => Some(Client::OpenCode),
            "dsh" => Some(Client::Dsh),
            "kimi_code" => Some(Client::KimiCode),
            _ => None,
        }
    }
}

impl fmt::Display for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ids_round_trip_and_unknown_ids_are_rejected() {
        for client in Client::ALL {
            assert_eq!(Client::parse(client.as_str()), Some(client));
            assert_eq!(client.to_string(), client.as_str());
        }
        assert_eq!(Client::parse("nope"), None);
    }
}
