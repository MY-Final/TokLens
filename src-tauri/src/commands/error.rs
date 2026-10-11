//! Serializable command errors.

use super::dto;
use crate::usage::store::UsageError;
use serde::Serialize;

/// 命令错误：可序列化、对用户可读。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub message: String,
}

impl CommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<UsageError> for CommandError {
    fn from(error: UsageError) -> Self {
        CommandError::new(dto::usage_error_message(&error))
    }
}
