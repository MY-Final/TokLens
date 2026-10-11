//! 统一本地用量数据模型。
//!
//! 组成：
//!
//! - [`client`]：客户端标识与稳定字符串映射。
//! - [`tokens`]：Token 字段语义与去重叠规则（每个客户端的关系都有本机实测依据）。
//! - [`model`]：客户端无关的会话/消息结构、去重标识、口径枚举。
//! - [`normalize`]：各客户端的归一化规则（累计值、重复消息、双表兼容）。
//! - [`time`]：UTC 时间戳解析与系统时间读取。
//! - [`analytics`]：区间、趋势、排行与会话查询。
//! - [`store`]：SQLite 表结构、版本化迁移、幂等导入与增量同步水位。
//!
//! 边界：本模块只包含数据模型 / 统计口径 / 存储，不含采集器（文件发现与读取）与 UI 呈现。
//! 采集器实现时只允许调用这里的公开 API。

pub mod analytics;
pub mod client;
pub mod model;
pub mod normalize;
pub mod store;
pub mod time;
pub mod tokens;
