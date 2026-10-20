//! Cline 的只读采集器：只读取会话清单与消息文件中的身份、时间和用量字段。
mod discovery;
mod parser;
mod rollup;
mod sync;

pub use sync::ClineCollector;
const PARSER_VERSION: i64 = 1;

#[cfg(test)]
mod tests;
