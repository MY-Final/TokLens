# 生成式面板（Generative UI）可行性规格

> 草案 v1（2026-10-14）· 待实施排期
>
> 本文档合并第 1 轮产品草案、前端（体检-1）PanelSpec/渲染器方案与 Rust（体检-3）查询映射/持久化/校验方案，
> 为三方共同基线。**核心红线：AI 只产"声明式面板规格（纯 JSON 数据）"、不产代码；所有数值 100% 来自本地 Rust 查询；默认离线。**
>
> 落地前须按 AGENTS.md §3/§8 先在 `docs/architecture.md` 登记相关 ADR（新目录、settings schema 升版、任何网络/拖拽依赖）。

---

## ① 需求与红线

### 用户故事

1. **自然语言查维度**：「看上周每个模型用了多少 token」→ 生成排行/表格面板。"看什么"由自然语言决定，**数值一律本地查询**。
2. **收藏常用**：把「本机 Codex 日趋势 + 客户端占比」保存为命名面板，一键切换，无需重设筛选。
3. **导出一致**：生成的表格面板导出，与现有导出同口径、同字段、同估算排除规则。
4. **无 AI 也完整可用**：P1 手动从白名单挑面板 + 参数，全程离线。
5. **可信任**：每个面板保留来源等级（measured/derived/estimated）与「未知 / 零 / 不完整」语义，沿用现有口径标注与 tooltip。

### 红线（不可协商）

1. 默认离线；AI 功能 **opt-in、默认关**。
2. spec **纯数据**、类型/字段为**封闭白名单**，**拒绝未知字段**；不含 SQL / 字段名 / 公式 / 提示词 / 文件路径。
3. **数值 100% 来自本地 Rust**，AI 不参与取数/聚合；会话累计与请求合计永不混算（`docs/data-model.md` 铁律）。
4. 保留 measured/derived/estimated 与 unknown/zero 语义，沿用现有口径标注。
5. BYO Key 仅存 OS keyring，不入库/不入日志/不写明文设置；write-only；可一键清除。
6. 最小权限：不新增 `fs` / `shell` / 任意 `http`；自定义命令不需 `capabilities` 条目。
7. 不引第二套图表库；继续复用现有面板组件与 Recharts 封装。

---

## ② PanelSpec schema

设计原则：纯数据、白名单、可校验、无表达式/SQL/提示词；时间复用 `lib/ranges.ts` 语义；客户端必须是 `ClientKey`。

```jsonc
{
  "specVersion": 1, // 必填；迁移依据
  "id": "p_2f1a", // 前端生成，稳定标识
  "type": "token_trend", // 枚举，见 ③
  "title": "每日 Token 趋势", // 可选；≤80；缺省用类型默认名
  "query": {
    "range": { "kind": "followPage" },
    // kind: "followPage"(页面顶部日期) | "preset"(today/7d/30d/90d/all/week/month) | "custom"({fromMs,toMs})
    "timeBasis": "session", // "session"|"request"；仅支持双口径的类型有意义，见 ③；缺省按类型默认
    "client": null, // ClientKey | null
  },
  "typeOptions": {}, // 类型专属，见 ③
  "layout": { "colSpan": 12, "rowSpan": 1, "order": 0 },
  "source": { "origin": "manual" }, // "manual" | "ai"(P2)；仅用于 UI 标注来源，不参与取值
}
```

**通用字段**：`specVersion / id / type / title? / query.range / query.timeBasis? / query.client / layout{colSpan,rowSpan,order} / source.origin`。

**类型专属字段**（`typeOptions`，全部有枚举/数值边界）：
`metric?`（`tokens`|`requests`）、`dimension?`（`model`|`project`）、`limit?`、`sort?`、`requestTime?`、`stack?`、`fields?`、`shortenKeys?`、`search?`、`model?`、`projectPath?`。

> 字段命名统一 camelCase（与 `src-tauri/src/commands/dto.rs` 的 `rename_all="camelCase"` 对应）。Rust 草案中的 `source{timeBasis,client,projectPath,model}` 语义已并入 `query`（`source` 仅保留 `origin` 溯源），避免与前端命名冲突。

### 示例

```jsonc
// 1) 会话累计日趋势
{
  "specVersion": 1,
  "id": "p_t1",
  "type": "token_trend",
  "title": "每日 Token 趋势",
  "query": { "range": { "kind": "followPage" }, "timeBasis": "session", "client": null },
  "typeOptions": { "stack": "client" },
  "layout": { "colSpan": 12, "rowSpan": 1, "order": 0 },
  "source": { "origin": "manual" },
}
```

```jsonc
// 2) 模型排行 Top 5（请求时间口径）
{
  "specVersion": 1,
  "id": "p_r1",
  "type": "ranking",
  "title": "模型排行 Top 5",
  "query": {
    "range": { "kind": "preset", "key": "30d" },
    "timeBasis": "request",
    "client": "claude_code",
  },
  "typeOptions": { "dimension": "model", "limit": 5, "shortenKeys": false },
  "layout": { "colSpan": 6, "rowSpan": 1, "order": 1 },
  "source": { "origin": "manual" },
}
```

```jsonc
// 3) 最近会话表
{
  "specVersion": 1,
  "id": "p_s1",
  "type": "session_table",
  "title": "最近会话",
  "query": { "range": { "kind": "followPage" }, "timeBasis": "session", "client": null },
  "typeOptions": {
    "limit": 10,
    "sort": "newest",
    "requestTime": false,
    "search": null,
    "model": null,
    "projectPath": null,
  },
  "layout": { "colSpan": 12, "rowSpan": 1, "order": 2 },
  "source": { "origin": "manual" },
}
```

---

## ③ 面板类型目录（type ↔ 组件 ↔ 命令 ↔ props 四方对齐）

采用前端**具体粒度**命名（7 类），逐一映射到 Rust 领域查询。**全部复用现有只读命令，P1 零新 SQL。**

| `type`                | 复用组件               | 后端查询（`lib/api.ts` / 命令）                                    | 口径              | spec → 组件 props                                                                                                                                     |
| --------------------- | ---------------------- | ------------------------------------------------------------------ | ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `stat`                | `StatCard`             | `getOverviewStats` (`api.ts:163`) / `get_request_analysis.summary` | session / request | `typeOptions.field ∈ rangeTotal\|todayTotal\|sessions\|messages` → `label/value/hint/tooltip`；未知/零判定在适配器（复用 `OverviewPage.tsx:126-135`） |
| `token_trend`         | `TokenTrendChart`      | `getDailyTokenTrend` (`api.ts:198`)                                | session 日桶      | `buckets=res`；`rangeLabel/selectedRange=ctx`；`typeOptions.stack`                                                                                    |
| `request_trend`       | `RequestActivityPanel` | `getRequestActivity` (`api.ts:169`)                                | request 日/小时   | `data=res`；`typeOptions.metric`；`dayBoundaries/hourlyBoundaries` 由 `lib/ranges.ts` 计算；`client/selectedRange=ctx`                                |
| `client_distribution` | `ClientUsageCard`      | `getClientUsage` (`api.ts:214`)                                    | session / request | `usage=res`；`basis` 按 `timeBasis`；`onOpenClient` → 下钻会话                                                                                        |
| `ranking`             | `RankingCard`          | `getRankings` (`api.ts:218`) / `get_request_analysis`              | session / request | `entries=res.models\|res.projects`；`limit/shortenKeys`；`title/description/empty*` 来自 spec + 默认文案                                              |
| `session_table`       | `SessionTable`         | `getSessions` (`api.ts:229`)                                       | session / request | `sessions=res`；`limit/sort/requestTime/search/model/projectPath`                                                                                     |
| `activity_calendar`   | `CalendarHeatmap`      | `getDailyTokenTrend`（≤365 天）                                    | session 日桶      | `buckets=res`；`rangeLabel/selectedRange`；`onSelectDay` → 下钻                                                                                       |

补充：

- `RequestActivityPanel` 较重（内含 trend/calendar/clients 三 Tab），P1 整块复用；若产品要更"小而专"的请求趋势，P2 再加组件可选 `compact` 变体，**不在 P1 拆**。
- `request_trend` 与 `token_trend` 都是趋势，但口径不同（request vs session），故保留为两个具体类型而非合并的 `trend`。
- 所有类型映射到现有命令，`run_panel_query` 分发到 `usage::analytics` 同源领域函数（见 ④）。

### 渲染器架构

```
<PanelRenderer spec ctx>
  └ PanelShell(spec, state, skeletonByType, explanation)
      └ registry[spec.type].Body(data, spec, ctx)   // 映射到现有组件 props
```

- **取数层** `usePanelData(spec, ctx) → AsyncData<T>`（`src/hooks/use-panels/use-panel-data.ts`），内部用 `useAsyncData(loader, key, scope)`：
  - `key = "${spec.id}:${stableHash(query, typeOptions)}:${ctx.refreshToken}"`；
  - `scope = hash(query)`（**口径变化才隐藏旧数据**，后台刷新保留数据，与 `use-async-data.ts` 既有语义一致）。
- **适配器纪律**：`panelRegistry[type]` 只做**字段搬运与展示格式化**（复用 `lib/format.ts`、`UsageTooltip`），绝不做聚合/去重/估算；`metricValue`、`knownTotal` 等"未知 vs 零"展示判定集中在此一处，禁止下放 spec。
- **三态**：`error → ErrorBlock`；`data==null && (loading||syncing) → skeletonByType[type]`；`data!=null && empty → EmptyBlock`；否则渲染 body。骨架复用 `StateBlocks.tsx` 对应形状。

---

## ④ 命令映射与参数上限

`run_panel_query(spec)`：**Rust 内白名单映射 + 单一入口**，按 `spec.type`/`timeBasis` 分发到 `usage::analytics` 既有领域查询（`range_totals` / `daily_token_trend_in_range` / `request_activity_with_hourly` / `client_usage` / `model_ranking` / `project_ranking` / `session_summaries` / `session_detail` / `request_analysis`）。**只读、无状态；不落库、不写文件、不发网络。** 未来新聚合必须进 `usage/analytics` 并带 `#[cfg(test)]`，禁止在分发器内写 SQL。

| panel type            | 口径            | 后端查询                | 参数白名单                                                                                                                                | 上限                                                        |
| --------------------- | --------------- | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `stat`                | session         | `get_overview_stats`    | `fromMs?`,`toMs?`,`todayFromMs?`,`client?`                                                                                                | `fromMs<toMs`                                               |
| `stat`                | request         | `get_request_analysis`  | `fromMs?`,`toMs?`,`client?`                                                                                                               | `fromMs<toMs`                                               |
| `token_trend`         | session         | `get_daily_token_trend` | `fromMs`,`dayCount`,`dayStartMs?`,`toMs?`,`client?`                                                                                       | `dayCount 1..=400`                                          |
| `request_trend`       | request         | `get_request_activity`  | `fromMs?`,`toMs?`,`client?`,`dayBoundaries`,`hourlyBoundaries?`                                                                           | 日桶 `2..=401` 严格递增；小时 `≤27` 且端点==`[fromMs,toMs]` |
| `ranking`             | session         | `get_rankings`          | `fromMs?`,`toMs?`,`client?`,`limit?`,`groupBy∈{model,project}`                                                                            | `limit 1..=50`（默认 8）                                    |
| `ranking`             | request         | `get_request_analysis`  | 同左                                                                                                                                      | 同上                                                        |
| `client_distribution` | session         | `get_client_usage`      | `fromMs?`,`toMs?`                                                                                                                         | 固定 12 客户端                                              |
| `client_distribution` | request         | `get_request_analysis`  | `fromMs?`,`toMs?`,`client?`                                                                                                               | —                                                           |
| `session_table`       | session/request | `get_sessions`          | `SessionQuery`（`client?`,`limit?`,`offset?`,`search?`,`projectPath?`,`sort?`,`model?`,`modelUnknown?`,`projectUnknown?`,`requestTime?`） | `limit 1..=500`（默认 20）；`offset ≥0`（建议 ≤1_000_000）  |
| `activity_calendar`   | session         | `get_daily_token_trend` | 同 `token_trend`                                                                                                                          | `dayCount 1..=400`；日历建议 ≤365                           |

**参数上限（全部复用现有行为，不新造阈值）**：时间戳 i64 Unix ms 且 `fromMs<toMs`；`sort ∈ newest/oldest/tokens_desc/requests_desc`；`client` 为 12 值 `ClientKey` 枚举且须 ∈ 已启用集合；`search/projectPath/model` 为字符串（建议 ≤200，`search` 用 `instr` 字面匹配，无 LIKE 通配）；`title ≤80`。**越界在校验层拒绝（不改写用户 spec）；查询边界保留现有 `clamp` 作为纵深防御。**

`metric` 只影响前端展示（`tokens`/`requests`），后端一次性返回完整 DTO，不按 metric 裁剪。口径由 `timeBasis` 决定走哪一列，**永不混算**。

---

## ⑤ 呈现与布局

- **不在 208px 侧栏渲染图表**（红线）。侧栏仅新增「自定义」导航项；面板渲染在主内容区新页。
- **自定义页 = 主内容区新页**：
  - `AppShell.tsx` 增加 `PageKey "custom"`、`NAV_ITEMS` 项（图标如 `LayoutGrid`）、`PAGE_TITLES`；`App.tsx` 增加 `page==="custom"` 渲染分支（传 `range/nowMs/refreshToken/onOpenSessions`）。
  - 页面网格：`grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-6 xl:grid-cols-12`，面板 `colSpan` 走**静态类映射**；`<xl` 时 12→整行、6→半行；整页仍在 `max-w-[1600px]` 内。
- **P1 固定主区 12 列网格**：12 可整除 3/4/6/12，覆盖「统计卡(3)/半宽图(6)/全宽表(12)」。**P1 就在 schema 落 `colSpan`**，避免 P3 改 schema 返工。
- **Tailwind 静态类陷阱**：v4 按源码字面量扫描，`col-span-${n}` 动态拼接**不会生成**；必须用静态映射 `const SPAN = {3:"col-span-3",4:"col-span-4",6:"col-span-6",12:"col-span-12"}`，响应式塌缩同理。
- **`followPage` 面板**复用顶部 `DateRangePicker`；页面自身编辑控件放 `secondaryToolbar` 或页首行。
- **侧栏已保存列表**：P1 放页面内（列表+编辑）；侧栏列表列 P1.5/P3。
- **托盘导航**：`lib/api.ts` 的 `onTrayNavigate` 白名单与 Rust `desktop/tray.rs` 目前仅 overview/analytics/sessions/sources；若「自定义」需托盘可达，需同步改前端白名单 + Rust（跨端小改动，登记为附带项）。
- **P1/P3 边界**：P1 固定 12 列、`colSpan` 来自 spec、上/下移顺序（写 `layout.order`）、增删复制、每面板可选 range、校验 + 持久化，**无拖拽、无 x/y、无 resize**；P3 再谈拖拽/缩放/模板/侧栏列表。

---

## ⑥ 持久化与迁移

**归属裁断：复用 `settings/` 版本化 JSON（`settings::Document`），不新建独立存储、不放统计库。**

理由：

- 面板是**用户配置**，不是统计元数据。统计库会被 `clear_statistics` 清空，放进去会导致"清空统计顺手删用户面板"，语义错误。
- `settings/mod.rs` 已有版本化 JSON、原子保存（临时文件 + `sync_all` + rename）、损坏回退默认 + 可见 warning，正是配置类数据的既有机制。
- **迁移与目录搬迁免费**：`settings.json` 已在 `storage.rs` 的 `OWN_FILES` 中，随存储位置迁移自动处理；另起 `panels.json` 需同步 `OWN_FILES` 与 relocate/cleanup，增加遗漏风险。
- 统计库用 append-only SQL 迁移，适合稳定结构；面板 spec 形状随 P1→P3 演进，JSON + 版本号更轻。

**版本对齐**：

- 文件级：`settings::Document.schema_version` 由 1 升 2，迁移 1→2 仅补 `panels: []`（`#[serde(default)]` 已容忍缺失字段）。
- 单个 spec：`specVersion` 独立自增；加载时 `> 已知` → 标记"暂不支持/只读"并**保留数据**（不静默丢弃），`< 已知` → 走 `migrate_spec`（纯函数，可单测）。
- 破坏性变更须在提交说明标注（与 `docs/architecture.md` 契约一致）。

**命令**：`get_panel_specs` / `save_panel_spec` / `delete_panel_spec` / `reorder_panels`（走 `SettingsStore::transform`）。运行查询 `run_panel_query` 无状态、不持久化。

**面板存活语义**（见 ⑩ 决策 ①）：`clear_statistics` 绝不删面板；`reset_settings` 默认保留面板，另设独立「重置面板」入口。

**导入/导出**：`export_panel_specs(directory)` 复用现有导出守卫 + `storage::validate_target` + `create_new` 唯一文件策略，**只导出 spec JSON**（纯数据，可分享），不导出数据快照。导入必须经 Rust 校验器（永不信任文件），严格模式处理未知字段。

---

## ⑦ 校验与安全

**双层：Rust 权威 + 前端镜像（UX 预校验）**，`get_panel_catalog()` 为单一事实来源（SSOT）。

### 前端镜像（`src/lib/panel-spec.ts`）

纯函数 `validatePanelSpec(unknown): Result`：白名单键、`type` 枚举、`client ∈ CLIENT_KEYS|null`、`range.kind` 枚举；数值边界 `colSpan∈{3,4,6,12}`、`rowSpan∈{1,2}`、`limit∈[1,100]`、`order∈[0,63]`、`custom.fromMs<toMs`；**默认拒绝未知字段**；不接受刷新/表达式/文件路径/提示词。**P2 AI 输出必须过同一 validator。**

### Rust 权威（`run_panel_query` 单一咽喉）

1. `specVersion`：未知高版本 → 拒绝（提示升级）；低版本 → 迁移后执行。
2. `type` ∈ 目录枚举；未知 → 拒绝。
3. **`#[serde(deny_unknown_fields)]`** 于 `PanelSpec` 与 `params`（严格模式）。
4. `client` 用 `Client::parse` 解析，且须 ∈ `settings.clients()`；`timeBasis` 枚举。
5. 数值范围按 ④ 逐项校验，**越界拒绝（不改写）**；查询边界保留现有 `clamp`。
6. 桶边界复用现有判定（`dayBoundaries` 严格递增 `2..=401`；`hourlyBoundaries` 端点 `== [fromMs,toMs]`）。
7. 字符串长度上限；`sort`/`groupBy` 为枚举，**绝不拼接 SQL**（现有 analytics 列名全为常量）。
8. 分发器只读，无写库/文件/网络/状态修改；**避免 `panic!/unwrap`**，错误经 `CommandError` 返回（release `panic="abort"` 下 unwind 兜底仅 debug/test 生效）。

### 白名单目录导出 `get_panel_catalog()`

`panels/catalog.rs` 编译期注册表 → DTO `PanelCatalog { catalogVersion, panelTypes:[{ type, timeBases, params:[{name,kind,required,min,max,values}], fields, query }], globalLimits }`。前端手动构建器据此渲染合法控件；P2 作为允许的 JSON Schema/tool 定义交给 LLM，Rust 入口再校验一次（纵深防御 + 防幻觉）。

### 安全

- **不新增权限**：自定义命令经 `invoke_handler` 注册，无需 `capabilities` 条目；`capabilities/default.json` 保持 `core:default` + 固定 opener URL。
- 分发器输入为受限枚举/标量，天然免疫注入；拒绝未知字段防请求放大。
- **BYO Key（仅 P2 opt-in）**：只存 OS keyring（Windows Credential Manager / macOS Keychain / Linux Secret Service），**write-only**（前端只 `set`/`clear`，后端不回读明文）；建议命令 `set_llm_credential` / `clear_llm_credential` / `has_llm_credential`；删除面板/卸载/关闭时清理。不配置 key 时完全离线。**联网 LLM 形态已由产品负责人拍板：OpenAI 兼容协议（如 DeepSeek、中转站等），用户自行填写 base URL / API Key / 模型名（见 ⑩ ⑦）；依赖与出网路径（keyring / http、CSP）实施前须过 ADR。**

### 测试（Rust）

- 目录 ↔ 分发器一致性：每个 `type` 有分发分支、每个参数约束有对应校验（新增/漏配即失败）。
- 目录上限 == 命令上限的回归测试，防漂移。
- 逐类型 round-trip：合法 spec 通过、越界/未知被拒、spec 无法触达 SQL/路径。
- 全部使用合成数据，不依赖真实会话/网络/当前时间。

---

## ⑧ 分期

- **P1 手动（零 AI）**：schema v1 + validator + registry + `PanelShell`（**薄壳先行**）+ 自定义页 + 静态 12 列 + 持久化接口对接；复用现有 7 类组件。**不引新依赖、不新增权限。**
- **P1.5 组件 bare 化收敛**：把 `ClientUsageCard/RankingCard/TokenTrendChart/CalendarHeatmap/RequestActivityPanel` 增加 `bare` 变体、抽掉各自卡壳/状态块，统一由 `PanelShell` 承担（复用时避免双卡）。属中等重构，**需协调员排期**。
- **P2 NL→spec（先离线）**：**离线规则解析兜底**（关键词→spec，纯前端/确定性）先行；LLM opt-in 默认关闭，产出必须过同一 validator。**默认路径完全不联网。** LLM 采用 **OpenAI 兼容协议**（用户自填 base URL / Key / 模型，如 DeepSeek、中转站；见 ⑩ ⑦），实施前须过 ADR（keyring / http、CSP 与出网评估）。
- **P3 布局自由**：拖拽/resize/模板/侧栏列表面板；引入任何网格/拖拽库前先 ADR（如 `react-grid-layout`）。

`PanelShell` 对外契约在 P1 冻结：`spec` + 统一 `PanelState` + `skeletonByType` + `explanation` slot；P1.5 把组件逐个迁入而不改渲染器。

---

## ⑨ 验收标准

- 7 类 spec 均可渲染；`followPage/preset/custom` range 与 `timeBasis` 生效。
- 三态正确；`空 && 同步中` 显示形状骨架；错误可重试；**未知/零、measured/derived/estimated 语义不引入任何前端重算**（复用 tooltip/说明文案）。
- `colSpan` 塌缩正确，800×600 无页面级横向溢出；无第二套图表库。
- 非法 spec 被整体拒绝（不半渲染）；`specVersion` 迁移可回退；高版本 spec 保留不丢数据。
- **默认离线**；P1 无新依赖、无新权限；分发器只读；
- 导出仅 spec JSON，导入经 Rust 严格校验。
- Rust 侧：目录/分发器一致性、上限回归、逐类型 round-trip 单测通过。

---

## ⑩ 决策与开放问题

### 已裁定（PM）

**① `clear_statistics` / `reset_settings` 是否删除用户面板？**
**裁定：`clear_statistics` 绝不删面板；`reset_settings` 默认保留面板，另设独立「重置面板」入口。** 理由：面板是用户内容、非统计元数据；"清空统计"与"重置偏好"都不应意外销毁用户创作。为此面板放在 `settings::Document.panels`（独立数组），`reset_settings` 只 patch `AppSettings`、从不触碰 `panels`。

**② BYO Key 调用方（前端直连 vs Rust 代发）？**
**裁定：协议形态已由产品负责人拍板（见 ⑦：OpenAI 兼容、用户自填配置）；调用方与依赖选型（前端直连 / Rust 代发、keyring / http、CSP）留给实施前 ADR 评估。** 理由：两条路径都触及"默认离线"红线，必须以 ADR + 人类评审收口后才能开工。

**③ 导入/导出严格度？**
**裁定：仅导出 spec（不含数据快照）；导入走 Rust 严格校验；未知字段拒绝；高版本 `specVersion` 拒绝但不丢数据（标记只读并保留）。** 理由：可分享的是"看什么"的定义，不是本地数据；严格拒绝防止不可信文件注入；高版本保留避免降级时数据丢失。

**④ 持久化归属？**
**裁定：复用 `settings::Document`，`schema_version` 1→2 补 `panels: []`，随存储迁移、原子写（见 ⑥）。** 理由：避免新存储的迁移/清理遗漏；面板生命周期由决策 ① 保护，消除前端"用户内容混入偏好"的担忧（独立数组 + `reset` 不触碰）。

**⑤ P2 联网策略？**
**裁定：P2 默认离线（离线规则解析先行）；联网 LLM 已由产品负责人批准为 opt-in（OpenAI 兼容、用户自填配置，见 ⑦），实施前置 = ADR 通过。** 理由：符合 AGENTS"本地优先 > 最小权限 > 可验证"的取舍优先级。

**⑥ 组件 bare 化重构授权？**
**裁定：批准分阶段——P1 薄壳 `PanelShell` 先行并冻结契约，P1.5 收敛为 `bare` 变体；排期由协调员安排。** 理由：P1 低风险可用；收敛消除双卡与标题/口径来源分裂，但涉及他人持有的共享组件，需协调排期。

**⑦ 联网 LLM 形态（产品负责人拍板，2026-10-14）？**
**裁定：采用 OpenAI 兼容协议（如 DeepSeek、中转站等），用户自行填写 base URL / API Key / 模型名。** 约束：opt-in、默认关闭；Key 仅存 OS keyring（write-only）；AI 产出必须过同一 validator；keyring / http 依赖与 CSP、出网路径实施前必须过 ADR。理由：OpenAI 兼容是生态事实标准，不绑定单一供应商，用户自带凭据以保持"默认离线"。

### 待协调员 / 产品负责人后续确认

- **P1.5 组件 bare 化重构的排期与资源**：待协调员 / 产品负责人排期确认。
- **P3 拖拽/缩放库引入（如 `react-grid-layout`）**：先 ADR，待人类确认。
- **联网 LLM 实施排期与 ADR**：形态已定（⑦）；keyring / http / CSP 的 ADR 评审与实施排期待定。

### 其他登记项

- 新文档 `docs/generative-panel-spec.md` 生效时同步 `docs/README.md` 索引。
- 新目录/模块（`src/components/panels/`、`src/lib/panel-spec.ts`、`src/hooks/use-panels/`、`src-tauri/src/panels/`）与 settings schema 升版，均须先在 `docs/architecture.md` 登记 ADR；若持久化落 `settings::Document`，在 `docs/data-model.md`/ADR 注明"只存面板定义与布局，不含会话正文与任何用量原始数据"。
