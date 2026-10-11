//! Tauri Command 层：应用状态、Codex 同步调度与前后端 DTO。
//!
//! 分层（见 `docs/architecture.md`）：查询调用 `usage`，同步调用 `collectors`；采集器调用归一化与存储 API。
//! 本层只做参数校验、调用领域层、组装 DTO，不内联统计口径。
//!
//! 隐私：只返回元数据与统计值；会话正文、提示词、凭据不进入任何 DTO。

mod activity;
pub mod dto;
mod error;
pub mod queries;
pub mod settings;
mod state;
mod sync;

pub use dto::SyncStateDto;
pub use error::CommandError;
pub use state::{AppState, SyncState};
pub use sync::{SYNC_EVENT, request_sync};
