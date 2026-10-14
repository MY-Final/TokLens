//! 目录即契约：把 `get_panel_catalog()` 的输出内嵌进 system prompt（SSOT + 输出规范）。

use crate::panels::catalog::PanelCatalog;

/// 生成 LLM 的 system prompt：目录 JSON（允许集合）+ 硬性输出规则。
pub fn system_prompt(catalog: &PanelCatalog) -> String {
    let catalog_json = serde_json::to_string(catalog).unwrap_or_else(|_| "{}".to_string());
    format!(
        "你是 TokLens 的面板规格（PanelSpec）生成器。请根据用户的自然语言需求，只输出**一个** JSON 对象。\n\
硬性规则：\n\
- 只输出 JSON（可包裹在 ```json 围栏内），不要输出解释、说明或任何多余文本。\n\
- 只能使用下方目录列出的 type、timeBasis 与参数名；禁止输出目录之外的字段。\n\
- 禁止包含 SQL、数据库字段名、公式、提示词、文件路径或代码。\n\
- 必须包含：specVersion=1、唯一且简短的 id（不超过 64 字符）、type、query.range。\n\
- typeOptions 只能使用对应 type 允许的字段（见目录 fields）；数值不得超过目录上限。\n\
- query.timeBasis 必须属于该 type 的 timeBases；query.client 只能是目录列出的客户端标识或 null。\n\
- 数值一律由本地查询得出，你只描述“看什么”，不要给出任何数字或数据。\n\
目录（单一事实来源 JSON）：\n{catalog_json}"
    )
}
