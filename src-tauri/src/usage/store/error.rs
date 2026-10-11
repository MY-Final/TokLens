//! Recoverable storage errors.

/// 存储层错误。
#[derive(Debug)]
pub enum UsageError {
    Database(rusqlite::Error),
    /// 应用数据目录创建/写入失败。
    Io(std::io::Error),
    InvalidInput(String),
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsageError::Database(error) => write!(f, "usage database error: {error}"),
            UsageError::Io(error) => write!(f, "usage storage io error: {error}"),
            UsageError::InvalidInput(message) => write!(f, "invalid usage input: {message}"),
        }
    }
}

impl std::error::Error for UsageError {}

impl From<rusqlite::Error> for UsageError {
    fn from(error: rusqlite::Error) -> Self {
        UsageError::Database(error)
    }
}

impl From<std::io::Error> for UsageError {
    fn from(error: std::io::Error) -> Self {
        UsageError::Io(error)
    }
}
