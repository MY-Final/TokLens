# 架构规范

本文约束 TokLens 的模块划分、依赖方向与前后端契约。目标是让"从哪个客户端的哪个文件读出什么数据、在哪一层变成指标"
始终可定位，避免统计口径散落到 React 组件里。

## 1. 现状

已落地：Rust 侧的统一用量数据模型与本地 SQLite 存储（`src-tauri/src/usage/`）、Codex 采集器
（`src-tauri/src/collectors/codex.rs`）、Tauri command 层（`src-tauri/src/commands/`），以及前端总览界面
（`src/pages/`，侧边导航：总览 / 用量分析 / 会话明细 / 数据源管理）。
**Claude Code 与 OpenCode 采集器未实现（界面显示「等待采集」）；费用模块未实现（不展示任何金额）**。
新增架构组件时按第 8 节的 ADR 登记。

## 2. 分层与依赖方向

```
React 呈现层 (src/)
    │  invoke(DTO)          ← 唯一跨端方式
Tauri command 层 (src-tauri/src/commands/：状态 + DTO + 查询/同步命令)
    │
领域/聚合层 (聚合、去重、口径计算，纯函数，可单测)
    │
归一化层 (每个客户端一套规则 → 统一的 SessionRecord / MessageRecord)
    │
客户端适配器层 (src-tauri/src/collectors/：每个客户端一个模块，只读；当前已实现 codex)
    │
本地用量库 (TokLens 自有 SQLite：sessions / messages / source_files，只有 store.rs 写)
    │
本地数据源 (客户端自己的会话/日志文件)
```

依赖只能自上而下，禁止反向或跨层跳步：

- 前端不得直接读文件、不得自行推算统计口径；只消费 command 返回的 DTO。
- command 层只做参数校验、调用领域层、组装 DTO；不得内联解析逻辑。
- 适配器只负责"文件 → 归一化事件"，不得计算 UI 指标、不写数据库、不发网络请求。
- 领域层不依赖 Tauri / 文件系统 API，保持纯函数以便单测。
- 存储层（`src-tauri/src/usage/store.rs`）只接受已归一化的记录，不解析客户端原始格式；它是唯一允许写
  TokLens 本地库的模块，客户端数据库一律只读。
- 统一数据模型、Token 口径与去重键见 `docs/data-model.md`。

## 3. 前后端契约

- 跨端调用统一使用 `invoke<Dto>("command_name")`，命令名 `snake_case`，与 Rust 函数名一致。
- DTO 使用 `serde::Serialize` / `Deserialize`，字段名与前端 `type` 逐字对应；可选字段在两侧都标可选，
  不要用"空字符串/0"代替缺失值。
- 失败时返回 `Result<T, E>`，`E` 必须可序列化且对用户可读；前端负责把错误渲染成可理解的提示。
- DTO 变更必须同时更新两侧，并在提交说明中标注破坏性变更。
- Rust 侧 DTO 用 `#[serde(rename_all = "camelCase")]`，命令参数用 `#[tauri::command(rename_all = "camelCase")]`；
  前端类型定义在 `src/lib/api.ts`，字段与 Rust 逐字对应。
- 命令失败返回 `{ message: string }`，前端用 `toErrorMessage` 渲染成可读提示。
- 时间统一使用 ISO 8601 字符串（含时区偏移），单位在字段名中体现（如 `total_tokens`、`cost_usd`）。

### 3.1 命令清单（2026-10-11）

| 命令                     | 参数                                              | 返回                                                       |
| ------------------------ | ------------------------------------------------- | ---------------------------------------------------------- |
| `get_overview_stats`     | `fromMs` / `toMs` / `todayFromMs`                 | 区间与今日的会话数、明细数、token 分桶、未知/不完整计数    |
| `get_daily_token_trend`  | `fromMs` / `dayCount` / `client`                  | 每日分桶（缺失日期补 0）                                   |
| `get_client_usage`       | `fromMs` / `toMs`                                 | 三个客户端的会话/明细/token（未接入的 `supported: false`） |
| `get_rankings`           | `fromMs` / `toMs` / `client` / `limit`            | 模型与项目排行                                             |
| `get_sessions`           | `fromMs` / `toMs` / `client` / `limit` / `offset` | 会话摘要列表（只含元数据与用量）                           |
| `get_data_source_status` | —                                                 | 客户端接入状态、同步状态、库路径、数据时间范围             |
| `sync_codex`             | `force`                                           | 触发后台同步；完成后广播 `usage-sync` 事件                 |
| `app_info`               | —                                                 | 应用名与版本（连通性检查）                                 |

## 4. 客户端适配器规范

每个客户端一个独立模块（`codex`、`claude_code`、`opencode`，后续新增同理），并遵守：

1. **发现（discover）与解析（parse）分离**：发现函数定位候选文件；解析函数接收文件内容/路径，输出归一化事件，便于用 fixture 单测。
2. **只读**：以只读方式打开文件，绝不写入、移动或删除客户端数据。
3. **前向兼容**：未知字段忽略即可，不要因为新增字段而解析失败；已知字段缺失时按"缺失"处理，不猜测。
4. **可降级**：无法识别的文件计入"未解析文件数"并上报给 UI，不 panic、不静默丢弃。
5. **不落正文**：解析过程中不保存消息正文，只保留统计所需的元数据。
6. **大文件安全**：JSONL 类日志按行流式处理，避免整文件读入内存。
7. **fixture 驱动**：每个适配器都应有去敏 fixture（放 `src-tauri/tests/fixtures/<client>/`），格式如下：会话文件、期望的事件数、期望的 token/模型。

### 4.1 已实现的采集器

**Codex**（`src-tauri/src/collectors/codex.rs`）：扫描 `%USERPROFILE%\.codex\sessions` 下的 `rollout-*.jsonl`，
增量判定用 `(size_bytes, mtime_ms, parser_version)`，变化时整文件重新解析，并以单事务覆盖写回
（水位 + 会话累计值 + 该文件的请求明细）。新旧两套 Token 记录、累计值取值与隐私白名单见
`docs/data-model.md` 第 8 节。Claude Code 与 OpenCode 尚未实现。

新增客户端时的检查清单：

- [ ] 新增 `<client>` 适配器模块，只依赖归一化层
- [ ] 补充去敏 fixture 与解析单测（含未知字段、字段缺失、空文件）
- [ ] 在 `docs/data-metrics.md` 登记该客户端新增/变化的指标与来源等级
- [ ] 确认没有新增 Tauri 权限、没有新增网络访问
- [ ] 在 UI 上确认客户端维度可筛选、未解析文件可见

## 5. 持久化与状态

**已确定（2026-10-11）**：Rust 侧使用 `rusqlite`（`bundled` 特性，自带 SQLite，不依赖系统库）保存 TokLens 自有数据；
客户端自身的数据库/文件只读，绝不写入。表结构与迁移见 `docs/data-model.md`。硬性要求：

- 本地单文件/本地嵌入式存储，不引入远端服务；
- schema 带版本号与迁移路径，升级失败可回退；
- 存储内容仅限统计元数据，不含会话正文；
- 提供一键清除应用数据的能力（不影响客户端原始数据）。

前端状态管理仍待定：优先使用 React 内置能力；引入状态管理库或数据请求库前先登记决策。
本地库路径：`<应用数据目录>/toklens-usage.sqlite3`（Windows 为 `%APPDATA%\com.toklens.app\`），
由 `AppState::initialize` 在启动时创建并迁移；客户端数据目录只读。

## 6. 错误处理与日志

- 用户可见错误：一句话说明发生了什么 + 可以怎么做，不暴露文件正文与绝对路径中的隐私信息。
- 详细日志：默认仅本地、可关闭；禁止写入会话正文、token 明文、用户主目录完整路径。
- 统计类失败（如某个客户端目录不存在）视为"无数据"而非错误，UI 用空状态呈现。

## 7. 性能约束

- 首次扫描不得阻塞界面：UI 先渲染骨架屏，扫描结果异步返回。
- 增量扫描优先：按文件 `mtime` / 大小跳过未变化的文件，避免每次全量重扫。
- 聚合在 Rust 侧完成，前端只做展示层格式化；单次返回给前端的数据量应可控（分页或预聚合）。

## 8. 架构决策记录（ADR）

新增架构组件（持久化、状态管理、测试框架、错误处理库、调度/后台任务等）必须在下表登记，
一次决策一行，包含日期、决策、状态、理由/影响。只记录已决定的，不要预填未定项。

| 日期       | 决策                                                                              | 状态   | 理由 / 影响                                                             |
| ---------- | --------------------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------- |
| 2026-10-11 | 前后端边界使用 Tauri command，前端不直接访问文件系统                              | 已采纳 | 权限可控、口径集中在 Rust、便于单测                                     |
| 2026-10-11 | 客户端适配器与统计逻辑全部放在 Rust 侧                                            | 已采纳 | 避免在 UI 层复制口径，便于 fixture 化测试                               |
| 2026-10-11 | 持久化使用 `rusqlite`（bundled SQLite）写 TokLens 自有库                          | 已采纳 | 单文件、零外部依赖、可离线迁移；客户端库一律只读                        |
| 2026-10-11 | `sessions` 是统计总量唯一事实来源，`messages` 只做明细                            | 已采纳 | 避免累计值与明细求和重复计算                                            |
| 2026-10-11 | 迁移用 `schema_migrations` 表版本化，只追加不修改                                 | 已采纳 | 支持历史导入与重启恢复，升级可追溯                                      |
| 2026-10-11 | Rust 口径逻辑用内置 `#[cfg(test)]` 单测 + 合成数据                                | 已采纳 | 不引入额外测试依赖；真实会话数据不入库不入测试                          |
| 2026-10-11 | 采集器独立成 `src-tauri/src/collectors/`，只读客户端数据、只写 TokLens 库         | 已采纳 | 与数据模型解耦；采集器只做发现/解析/增量调度                            |
| 2026-10-11 | Codex 增量同步按文件水位整文件重解析（覆盖式写入）                                | 已采纳 | 无内容指纹列时仍能正确处理追加/截断/替换；实现简单且幂等                |
| 2026-10-11 | 命令层独立成 `src-tauri/src/commands/`（状态 / DTO / 查询分离）                   | 已采纳 | 前端只消费 DTO，统计口径仍集中在 `usage`                                |
| 2026-10-11 | 启动时后台执行增量同步，并使用独立数据库连接                                      | 已采纳 | WAL 下同步不阻塞 UI 查询；同步状态经 `usage-sync` 事件广播              |
| 2026-10-11 | 本地库放在应用数据目录（`%APPDATA%\com.toklens.app`）                             | 已采纳 | 不写项目目录，客户端数据只读                                            |
| 2026-10-11 | 界面不展示费用金额（`costAvailable` 恒为 false）                                  | 已采纳 | 本机数据没有可靠成本来源，不用 token 反推金额                           |
| 2026-10-11 | UI 采用统一 DashboardCard / ClientIdentity，图表采用 shadcn Chart + 已有 Recharts | 已采纳 | 统一外观与客户端配色，保留 Tauri DTO 和 Rust 统计边界，不新增运行时依赖 |

待登记（引入前补一行）：前端状态管理、前端测试框架、错误类型库（当前 `UsageError` 为模块内自定义枚举）、价格表来源。

### UI 组件结构（2026-10-11）

业务组件和页面采用 PascalCase 文件名；`DashboardCard` 组合 shadcn Card 统一面板外观，`ClientIdentity` 提供稳定客户端标识，`StatCard`、`TokenTrendChart`、`ClientUsageCard`、`RankingCard`、`SessionTable` 分别负责呈现。图表采用 shadcn Chart + 已有 Recharts，无新增运行时依赖。统计查询与 DTO 保持不变，费用来源未落地前 Tooltip 显示暂不可用，不反推金额。
