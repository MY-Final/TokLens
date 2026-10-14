//! 面板持久化：复用 `settings::Document` 的独立 `panels` 数组，原子写、版本化、随存储迁移。
//!
//! 存活语义（规格 ⑩①）：`clear_statistics` 与 `reset_settings` 均不触碰面板；
//! `reset_panel_specs` 为独立重置入口。
//!
//! 契约：写命令（save / delete / reorder / reset）与 `get_panel_specs` 一律返回**更新后的完整
//! 列表**；`get_panel_specs` 返回原始列表，由前端逐项校验（`readable=false` 表示高版本只读）。

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use super::spec::{
    SUPPORTED_SPEC_VERSION, migrate_spec, parse_spec, read_spec_version, validate_spec,
};
use crate::commands::CommandError;
use crate::settings::SettingsStore;
use crate::usage::client::Client;

/// 一条已保存面板；`readable=false` 表示 `specVersion` 高于本构建，仅原样保留（只读）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPanelDto {
    pub spec_version: u32,
    pub readable: bool,
    pub spec: Value,
}

fn stored(spec: &Value) -> StoredPanelDto {
    let spec_version = read_spec_version(spec).unwrap_or(0);
    let readable = spec_version <= SUPPORTED_SPEC_VERSION;
    let normalized = if readable {
        migrate_spec(spec.clone()).unwrap_or_else(|_| spec.clone())
    } else {
        spec.clone()
    };
    StoredPanelDto {
        spec_version,
        readable,
        spec: normalized,
    }
}

fn list_from(panels: &[Value]) -> Vec<StoredPanelDto> {
    panels.iter().map(stored).collect()
}

fn panel_id(spec: &Value) -> Option<&str> {
    spec.get("id").and_then(Value::as_str)
}

pub(crate) fn list(store: &SettingsStore) -> Result<Vec<StoredPanelDto>, CommandError> {
    Ok(list_from(&store.panels().map_err(CommandError::new)?))
}

/// 保存（相同 `id` 原位替换，否则追加）并返回更新后的完整列表。
pub(crate) fn save(
    store: &SettingsStore,
    enabled: &[Client],
    raw: Value,
) -> Result<Vec<StoredPanelDto>, CommandError> {
    let parsed = parse_spec(raw).map_err(CommandError::new)?;
    validate_spec(parsed.clone(), enabled).map_err(CommandError::new)?;
    let id = parsed.id.clone();
    let normalized =
        serde_json::to_value(&parsed).map_err(|_| CommandError::new("无法序列化面板规格"))?;
    let panels = store
        .transform_panels(|panels| {
            match panels
                .iter_mut()
                .find(|panel| panel_id(panel) == Some(id.as_str()))
            {
                Some(slot) => *slot = normalized.clone(),
                None => panels.push(normalized.clone()),
            }
            Ok(())
        })
        .map_err(CommandError::new)?;
    Ok(list_from(&panels))
}

pub(crate) fn delete(store: &SettingsStore, id: &str) -> Result<Vec<StoredPanelDto>, CommandError> {
    let id = id.trim().to_string();
    let panels = store
        .transform_panels(|panels| {
            panels.retain(|panel| panel_id(panel) != Some(id.as_str()));
            Ok(())
        })
        .map_err(CommandError::new)?;
    Ok(list_from(&panels))
}

pub(crate) fn reorder(
    store: &SettingsStore,
    ids: &[String],
) -> Result<Vec<StoredPanelDto>, CommandError> {
    let panels = store
        .transform_panels(|panels| {
            if ids.len() != panels.len() {
                return Err("面板顺序与现有面板数量不一致".into());
            }
            let mut positions = Vec::with_capacity(ids.len());
            for id in ids {
                let position = panels
                    .iter()
                    .position(|panel| panel_id(panel) == Some(id.as_str()))
                    .ok_or_else(|| "面板顺序包含未知标识".to_string())?;
                if positions.contains(&position) {
                    return Err("面板顺序包含重复标识".into());
                }
                positions.push(position);
            }
            let mut ordered: Vec<Value> = positions
                .iter()
                .map(|&position| panels[position].clone())
                .collect();
            for (order, panel) in ordered.iter_mut().enumerate() {
                set_order(panel, order as u32);
            }
            *panels = ordered;
            Ok(())
        })
        .map_err(CommandError::new)?;
    Ok(list_from(&panels))
}

pub(crate) fn reset(store: &SettingsStore) -> Result<Vec<StoredPanelDto>, CommandError> {
    let panels = store
        .transform_panels(|panels| {
            panels.clear();
            Ok(())
        })
        .map_err(CommandError::new)?;
    Ok(list_from(&panels))
}

/// 写入 `layout.order`；高版本（只读）面板不改写。
fn set_order(panel: &mut Value, order: u32) {
    if read_spec_version(panel).unwrap_or(0) > SUPPORTED_SPEC_VERSION {
        return;
    }
    let Some(object) = panel.as_object_mut() else {
        return;
    };
    let layout = object.entry("layout").or_insert_with(|| json!({}));
    match layout.as_object_mut() {
        Some(layout) => {
            layout.insert("order".to_string(), Value::from(order));
        }
        None => *layout = json!({ "order": order }),
    }
}

#[tauri::command]
pub fn get_panel_specs(app: AppHandle) -> Result<Vec<StoredPanelDto>, CommandError> {
    let store = app.state::<SettingsStore>();
    list(&store)
}

#[tauri::command]
pub fn save_panel_spec(app: AppHandle, spec: Value) -> Result<Vec<StoredPanelDto>, CommandError> {
    let enabled = super::query::enabled_clients(&app)?;
    let store = app.state::<SettingsStore>();
    save(&store, &enabled, spec)
}

#[tauri::command]
pub fn delete_panel_spec(app: AppHandle, id: String) -> Result<Vec<StoredPanelDto>, CommandError> {
    let store = app.state::<SettingsStore>();
    delete(&store, &id)
}

#[tauri::command]
pub fn reorder_panels(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<StoredPanelDto>, CommandError> {
    let store = app.state::<SettingsStore>();
    reorder(&store, &ids)
}

#[tauri::command]
pub fn reset_panel_specs(app: AppHandle) -> Result<Vec<StoredPanelDto>, CommandError> {
    let store = app.state::<SettingsStore>();
    reset(&store)
}
