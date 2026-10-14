//! 生成式面板（P1 手动自定义，零 AI）：规格模型、白名单目录、只读分发器与持久化。
//!
//! 边界（见 `docs/generative-panel-spec.md`）：本模块只处理"声明式面板规格"这一纯数据形态，
//! 绝不生成代码、不含 SQL/路径/提示词；所有数值经 [`query::run_panel_query`] 分发到
//! `usage::analytics` 的既有查询。默认离线、只读、无状态；面板定义复用 `settings::Document`。
//!
//! 依赖方向：`panels` → `usage::analytics` / `settings`；`usage` 不反向依赖 `panels`。
//! 子模块公开给 crate 根以完整路径注册命令（`#[tauri::command]` 生成的宏位于定义模块）。

pub(crate) mod catalog;
pub(crate) mod query;
mod spec;
pub(crate) mod store;

pub use catalog::{PanelCatalog, get_panel_catalog};
pub use query::{PanelQueryArgs, PanelQueryResultDto, run_panel_query};
pub use spec::{PanelSpec, SUPPORTED_SPEC_VERSION};
pub use store::{
    StoredPanelDto, delete_panel_spec, get_panel_specs, reorder_panels, reset_panel_specs,
    save_panel_spec,
};
