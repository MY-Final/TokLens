use crate::usage::client::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Ask,
    Tray,
    Exit,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub close_behavior: CloseBehavior,
    pub autostart: bool,
    pub autostart_hidden: bool,
    pub remember_window: bool,
    pub theme: Theme,
    pub default_range: String,
    pub sync_on_start: bool,
    pub sync_interval_minutes: u32,
    pub collection_paused: bool,
    pub enabled_clients: Vec<String>,
    pub automatic_updates: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::Ask,
            autostart: false,
            autostart_hidden: true,
            remember_window: true,
            theme: Theme::System,
            default_range: "30d".into(),
            sync_on_start: true,
            sync_interval_minutes: 5,
            collection_paused: false,
            enabled_clients: Client::ALL.iter().map(|c| c.as_str().into()).collect(),
            automatic_updates: false,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub close_behavior: Option<CloseBehavior>,
    pub autostart: Option<bool>,
    pub autostart_hidden: Option<bool>,
    pub remember_window: Option<bool>,
    pub theme: Option<Theme>,
    pub default_range: Option<String>,
    pub sync_on_start: Option<bool>,
    pub sync_interval_minutes: Option<u32>,
    pub collection_paused: Option<bool>,
    pub enabled_clients: Option<Vec<String>>,
    pub automatic_updates: Option<bool>,
}

impl AppSettings {
    pub fn patched(&self, patch: SettingsPatch) -> Result<Self, String> {
        let mut next = self.clone();
        macro_rules! apply {
            ($($field:ident),* $(,)?) => { $(if let Some(value) = patch.$field { next.$field = value; })* };
        }
        apply!(
            close_behavior,
            autostart,
            autostart_hidden,
            remember_window,
            theme,
            default_range,
            sync_on_start,
            sync_interval_minutes,
            collection_paused,
            enabled_clients,
            automatic_updates
        );
        next.validate()?;
        Ok(next)
    }

    pub fn validate(&self) -> Result<(), String> {
        if ![0, 1, 5, 15, 30].contains(&self.sync_interval_minutes) {
            return Err("采集频率必须为关闭或 1/5/15/30 分钟".into());
        }
        if !["today", "7d", "30d", "90d", "all"].contains(&self.default_range.as_str()) {
            return Err("不支持的默认统计范围".into());
        }
        let mut clients = std::collections::HashSet::new();
        for client in &self.enabled_clients {
            if Client::parse(client).is_none() || !clients.insert(client) {
                return Err("客户端设置包含未知或重复标识".into());
            }
        }
        Ok(())
    }

    pub fn clients(&self) -> Vec<Client> {
        self.enabled_clients
            .iter()
            .filter_map(|c| Client::parse(c))
            .collect()
    }
}
