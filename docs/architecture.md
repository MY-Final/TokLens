# 架构规范

本文约束 TokLens 的模块划分、依赖方向与前后端契约。目标是让"从哪个客户端的哪个文件读出什么数据、在哪一层变成指标"
始终可定位，避免统计口径散落到 React 组件里。

## 1. 现状

### Copilot Agent Host 补采决策（2026-10-12）

在 `collectors/copilot/` 增加 `events.rs`（SDK JSONL 元数据白名单）、`agent_host.rs`（VS Code 用量关联）和 `database.rs`（内存只读 SQLite/WAL 快照）。
默认同时发现 Copilot 根目录的 `session-state/*/events.jsonl` 与 Windows `%APPDATA%/Code/agentSessionData/*/session.db`。
通过 provider metadata 的 sdkSessionId 合并会话，通过 turns.event_id 对应 SDK user.message.id；镜像按 SDK 会话和原始用户事件去重。
整轮 turnTokenTotals 只用于会话总量；vscode.modelCall 仅补对应 apiCallId 的请求用量，缺少的调用计数保持未知，明细标记 partial。
复用已有统一模型的 client_reported / partial，无新迁移、权限、依赖包或网络能力；启用已有 rusqlite 的 serialize feature，在内存合成已提交 WAL 快照，避免 SQLite 修改客户端 shm。
只反序列化白名单；不保存正文、草稿、工具结果或文件内容。任何读取失败、尾行不完整或扫描中变化均不提交新快照。
非零缓存的包含关系缺乏正样本，暂不推断整轮总量；缓存为零时 input + output 可确定性换算，reasoning/cache_write 仍未知。

### Copilot CLI 采集器决策（2026-10-12）

新增 `collectors/copilot/`（只读 SQLite 白名单查询 / 同步 / 合成测试）与 `usage/normalize/copilot.rs`（包含缓存的输入换算）。
读取 `COPILOT_HOME/session-store.db`，未设置时为 `~/.copilot/session-store.db`；配置目录只能由客户端环境显式指定。
复用已有 rusqlite、请求账本与完整快照事务，不新增依赖、权限、迁移或网络能力。
仅读取 sessions 元数据与 assistant_usage_events 的身份、模型、时间和 Token 字段；不读正文、账户、日志或上下文占用。
每次读取 WAL 可见快照，以用量表主键去重；缺少用量表的旧版本保留会话且标记明细无法确认完整。
输入包含缓存读写，reasoning 包含于输出；归一化拆分缓存且不重复计入。缺失保持未知，费用和 premium requests 不换算。
新安装默认启用九个来源；已有设置保留原选择，新增 Copilot 可从数据源管理开启。
Copilot 使用已登记 LobeHub `GithubCopilot` 本地图标与独立语义色。

### WorkBuddy 采集器决策（2026-10-12）

新增 `collectors/workbuddy/`，按 JSONL 白名单解析、跨文件请求去重与会话聚合、同步拆分；复用 `usage/normalize/request_ledger.rs` 和完整快照事务。
只读 `WORKBUDDY_CONFIG_DIR/projects`，未设置时使用 `~/.workbuddy/projects`，不借用 CodeBuddy 的独立配置目录。
不读取账户、配置正文、blobs、工具输出、记忆、标题、心跳元数据或 session_usage 上下文占用；无新增依赖、权限、迁移或网络能力。
已核对 WorkBuddy 5.2.6 的落盘契约：一次模型调用的 usage 挂在最后一个输出事件，`message.usage` 是 `providerData.usage` 的镜像，不能相加。
请求按 `(sessionId, id)` 去重，不能使用跨多个模型调用复用的 conversationRequestId、traceId 或 providerData.messageId。
总量沿用客户端值，缓存读取是输入子集，推理是输出子集；缓存写入关系未知，不计入总量。缺字段保持未知，异常关系标记可见。
读取失败、不完整尾行、未知文件格式或文件指纹变化保留历史和水位；成功快照才反映来源删除。客户端统一使用 workbuddy 标识和现有 lucide Bot 图标。

### 桌面设置与更新决策（2026-10-11）

新增 `settings/`（版本化 JSON、校验与原子保存）、`desktop/`（托盘、窗口生命周期）、
`updates/`（官方签名更新）与 command 后台调度模块。前端新增设置页与共享设置 Hook，
仍通过 `lib/api.ts` 调用后端，使用 React 内置状态，不引入状态管理库。
主题 Hook 使用系统 matchMedia 与原生设置同步 CSS class，不通过 next-themes 注入脚本
（React 19 原生客户端渲染会对该脚本报错）；next-themes 仅保留已有 vendor Toaster 依赖。
设置位于 TokLens 应用数据目录；统计清除使用 usage/store 事务，不删除源文件。
调度、退出、清除与更新安装共用同步互斥边界，隐藏窗口不影响采集。

引入官方 Tauri 2 updater、autostart、single-instance、window-state 插件以及 tray-icon 特性。
updater 引入原生 HTTPS / 签名校验依赖；默认不发请求，仅显式检查或用户开启自动检查后使用。
其余插件提供系统自启、进程去重及窗口状态恢复，不采集正文、无遥测。具体体积以生产构建为准。
插件从 crates.io 获取；前端不增加 npm 运行时依赖，不开放通用 shell/fs/http 权限。
原生 command 封装固定目录打开和固定 GitHub 更新端点，权限仅授予主窗口；CSP 仅允许本地资源，
样式内联用于现有 shadcn/Recharts，不允许远程脚本。

发布目标为 Windows x64 NSIS、macOS x64/arm64 DMG 与签名更新包；GitHub Actions 在全部平台校验成功并汇总后自动正式发布 Release。
更新私钥仅保存在仓库外和 GitHub Secrets；公钥入配置。首版不使用系统代码签名或 Apple 公证。
发布规则与配置步骤见 `docs/releases.md`；新配置不能改变已有 Token 口径。
`scripts/release.py` 使用 Python 标准库完成版本验证、制品收集与清单汇总；测试使用合成文件与注入日期。
Rust 测试使用已有 updater 的 base64 / minisign-verify 同版本依赖校验制品及篡改拒绝，不增加运行时依赖树。
`updates/` 按 state / policy / check / download / install / errors 拆分；入口仅组织模块与导出。
前端 settings-types 与后端 DTO 对应；不新增通用插件 IPC 权限，所有更新通过主窗口的自有 command。

### MiMoCode / Pi Desktop 采集器决策（2026-10-11）

新增 `collectors/mimocode/`（只读 SQL 白名单 / 同步）与 `collectors/pi_desktop/`（JSONL 白名单 / 操作账本 / 同步），
共享 `usage/normalize/request_ledger.rs` 处理按请求求和与记录元数据，字段关系由各客户端 semantics 决定。不引入依赖、权限、数据库迁移或运行时联网。
MiMoCode 读取 XDG_DATA_HOME/mimocode/mimocode.db 或 ~/.local/share/mimocode/mimocode.db，尊重 MIMOCODE_DB；
只查询 session / message 统计字段和导入会话标识，按 claude_import / external_import.message_ids 排除导入的消息副本；保留导入后 MiMo 自己产生的请求。
不查询 part、event 镜像、memory、auth 或账户数据。每次同步在只读事务中读取 WAL 可见快照。
Pi Desktop 仅读取 ~/.pi-desktop/sessions 的 schema 1 JSONL，不读取同目录的秘密、日志或正文缓存；
按 operationId 合并物理请求，展开 operations 且不再叠加外层聚合；旧版无身份时回退 message id。
两个客户端使用完整快照原子替换；源缺失 / 读取失败 / 不支持的结构保留历史。
MiMoCode 的缓存和 reasoning 均独立；Pi 的缓存独立、reasoning 属 output 子集，两者缓存写入均计入总量。
成本字段现阶段不导入：MiMo 由价格换算，Pi 自报 costStatus=estimated，不能伪装客户端实测费用。
MiMoCode 使用现有 XiaomiMiMo 图标；Pi Desktop 使用 lucide TerminalSquare，不使用代表 Inflection AI 的 LobeHub Pi 标识。

### Kimi Code 采集器接入决策（2026-10-11）

新增 `collectors/kimi_code/`，按发现 / 会话元数据 / wire 白名单解析 / 同步拆分；
`usage/normalize/kimi_code.rs` 负责独立的字段语义与记录归一化。
根目录尊重客户端 `KIMI_CODE_HOME`（默认 `~/.kimi-code`），只读 `sessions` 中会话根的 `state.json`
与 `agents/<agent>/wire.jsonl`。支持已核对的旧会话元数据与 v2，以及 wire 1.4 / 1.5。
使用现有完整快照事务与文件指纹，失败 / 未完成写入 / 未知版本不推进水位；无新增依赖、权限或联网能力。
`source_files.kind = jsonl` 兼容标识文本 JSON 家族，包含用于定位与指纹的 JSON 元数据侧文件，不修改 v1 迁移。
只统计 `usage.record` 增量样本（包含 turn / session scope），不叠加 token_counting 上下文计数或服务端镜像。
输入、缓存读取、缓存创建互不重叠；输出没有单独 reasoning 字段。总量由四桶求和，必需字段缺失保持未知。
会话标识取 state.id，旧格式取客户端 `session_*` 目录名；请求键为 agent 目录 + wire 行序号，完整重扫幂等。
界面使用已有 LobeHub `Kimi` 标识与独立 `client-kimi` 语义色。

### Claude Code / OpenCode / DSH 采集器接入决策（2026-10-11）

新增 `collectors/claude_code/`（发现、白名单解析、会话聚合、同步）与 `collectors/opencode/`
（只读 SQLite 查询、同步），共用 `collectors/types.rs` 的同步选项与结果。
不新增依赖、权限或网络能力。Claude Code 尊重 `CLAUDE_CONFIG_DIR`，OpenCode 尊重 `XDG_DATA_HOME`。
JSONL 用 serde 白名单结构反序列化，忽略正文；SQLite 只查询允许的元数据列及 JSON 路径。

Claude Code 首先检查文件集合与大小 / mtime / 解析版本：任一变化时重读全部转录，跨文件按
`(sessionId, message.id)` 去重，纳入子代理；全部未变化时跳过。OpenCode 每次在只读事务中查询
元数据快照，避免仅按主数据库 mtime 跳过 WAL 更新；双表按更新时间取新、平局选 `session_v2`。
读取失败或发现文件在扫描中变化时保留已有数据和水位。成功后通过 `usage/store/snapshot.rs`
单事务替换该客户端完整元数据快照（含水位），使源数据修正、删除或截断可以反映到统计中；
此 API 只接受完整扫描，不复用 Codex 的累计值陈旧保护，不改变 Codex 导入行为。

后台调度分别执行各客户端采集器，单客户端失败不阻止其他客户端；DTO 增加逐客户端结果，
新命令 `sync_usage` 同步全部客户端，旧 `sync_codex` 保留仅 Codex 的兼容语义。

DSH（deepseek-hermes）新增 `collectors/dsh/`：只读 `~/.dsh/sessions/**/session.jsonl`
及 `session.jsonl.zstd`，并支持 v1–v4 规范版本化日志 `session.vN.jsonl[.zstd]`（失败隔离 ADR 见第 8 节），
同目录只读取最高代。不读配置、凭据或插件账单。默认根可由 `DSH_SESSIONS_DIR` 显式指定。
新增 Rust `zstd`（0.13，MIT，底层 Zstandard BSD）用于流式解压多个压缩帧，关闭非必需默认特性；
无网络、遥测或运行时外部进程。它会引入本地 C 解压器与编译依赖，具体版本以 Cargo.lock 为准。
只提取 header 元数据、请求模型与 usage 样本；chunk 与完成消息的 usage 不重复相加，
按会话 + turn / step / attempt 去重，并按格式版本排除 seedLength 或 inherited end-seed 指定的继承事件。未知格式保留旧数据并报告文件失败。

已落地：Rust 侧的统一用量数据模型与本地 SQLite 存储（`src-tauri/src/usage/`）、Codex 采集器
（`src-tauri/src/collectors/codex/`）、Tauri command 层（`src-tauri/src/commands/`），以及前端总览界面
（`src/pages/`，侧边导航：总览 / 用量分析 / 会话明细 / 数据源管理）。
Claude Code、OpenCode、DSH、Kimi Code、MiMoCode、Pi Desktop、WorkBuddy、Copilot CLI 采集器已接入；费用换算未实现（不展示统一金额）。
新增架构组件时按第 8 节的 ADR 登记。

## 2. 分层与依赖方向

模块依赖（箭头表示调用 / import 方向）：

```text
React pages / hooks ──→ lib/api.ts ──→ Tauri commands
React components ────→ lib/usage-types.ts (纯 DTO 类型)
commands/queries + request_queries ────→ usage/analytics ──→ usage/store
commands/sync ───────→ collectors/sync ──→ codex / claude_code / opencode / dsh / kimi_code / mimocode / pi_desktop / workbuddy / copilot
collectors/codex/sync ──→ discovery / parser / thread
collectors/codex/parser + thread ──→ usage/normalize
collectors/codex/sync ──→ usage/store (只写 TokLens 自有库)
usage/normalize ─────→ usage/model / tokens / time
usage/store ─────────→ usage/model / client / time
usage/tokens ────────→ usage/client
```

- 前端只消费 DTO；不得直接读文件或自行推算统计口径。`api.ts` 是 Tauri 调用和事件订阅入口，
  其余 `src/lib/` 模块不依赖 React 或 Tauri 运行时。
- command 层负责参数校验、状态与后台调度、DTO 转换；不得内联解析或统计规则。
- 采集器的发现与解析模块只读客户端文件、输出元数据；线程聚合选择累计快照；
  同步协调模块调用 `usage/store` 的事务 API，不直接执行 SQL、不写客户端目录、不发网络请求。
- `usage/model`、`tokens` 与 `normalize` 是可用合成数据测试的纯领域逻辑，不依赖 Tauri、采集器或文件系统。
  `time::now_ms` 是明确的系统时间入口，时间解析不依赖当前时间。
- `usage/store/` 负责连接、迁移和本地库写入，只接受统一记录，不解析原始客户端格式。
  `usage/analytics/` 只做读查询；二者不依赖 command 或 collectors，避免反向依赖。
- 数据流是「只读客户端文件 → 解析与归一化 → 本地用量库 → 查询 → DTO → UI」，
  不等同于 Rust 模块的 import 方向。统一数据模型、Token 口径与去重键见 `docs/data-model.md`。

## 3. 前后端契约

- 跨端调用统一使用 `invoke<Dto>("command_name")`，命令名 `snake_case`，与 Rust 函数名一致。
- DTO 使用 `serde::Serialize` / `Deserialize`，字段名与前端 `type` 逐字对应；可缺失的返回值在 Rust 使用 `Option<T>`、前端使用 `T | null`，
  不要用"空字符串/0"代替缺失值。
- 失败时返回 `Result<T, E>`，`E` 必须可序列化且对用户可读；前端负责把错误渲染成可理解的提示。
- DTO 变更必须同时更新两侧，并在提交说明中标注破坏性变更。
- 请求活动 DTO 的 `clients` 返回完整查询范围的客户端分组，与请求分析复用同一聚合逻辑；区间合计、分布、覆盖和日桶在同一只读事务快照内查询。前端不从最多 400 日的展示桶推算完整区间分布。
- Rust 侧 DTO 用 `#[serde(rename_all = "camelCase")]`，命令参数用 `#[tauri::command(rename_all = "camelCase")]`；
  前端类型定义在 `src/lib/usage-types.ts`，字段与 Rust 逐字对应；`api.ts` 保留类型重导出以兼容已有调用方。
- 命令失败返回 `{ message: string }`，前端用 `toErrorMessage` 渲染成可读提示。
- 跨端与数据库时间使用 Unix 毫秒，字段名以 `Ms` / `_ms` 标注；客户端原始 ISO 8601 UTC 字符串由
  `usage/time.rs` 解析。Token 与费用单位由字段名表达（如 `totalTokens`、`clientCostUsd`），不混用秒与毫秒。

### 3.1 命令清单（2026-10-11）

| 命令                     | 参数                                                       | 返回                                                           |
| ------------------------ | ---------------------------------------------------------- | -------------------------------------------------------------- |
| `get_overview_stats`     | `fromMs` / `toMs` / `todayFromMs` / `client`               | 区间与今日的会话数、明细数、token 分桶、未知/不完整计数        |
| `get_daily_token_trend`  | `fromMs` / `dayCount` / `dayStartMs?` / `toMs?` / `client` | 每日分桶（缺失日期补 0）                                       |
| `get_client_usage`       | `fromMs` / `toMs`                                          | 九个客户端的会话/明细/token 与后端占比（支持不等于已发现数据） |
| `get_rankings`           | `fromMs` / `toMs` / `client` / `limit`                     | 模型与项目排行                                                 |
| `get_sessions`           | `fromMs` / `toMs` / `client` / `limit` / `offset`          | 会话摘要列表（只含元数据与用量）                               |
| `get_request_analysis`   | `fromMs` / `toMs` / `client`                               | 完整区间客户端对比、已知请求均量、缓存命中及模型 / 项目归因    |
| `get_request_activity`   | `fromMs` / `toMs` / `client` / `dayBoundaries`             | 完整范围请求汇总、最多 400 日本地日桶与全已导入日期覆盖计数    |
| `get_session_detail`     | `client` / `clientSessionId` / `limit` / `offset`          | 全会话摘要、请求元数据页与全会话覆盖计数（不受全局日期筛选）   |
| `get_data_source_status` | —                                                          | 客户端接入状态、同步状态、库路径、数据时间范围                 |
| `sync_usage`             | `force`                                                    | 同步全部客户端，独立报告各数据源成功 / 缺失 / 失败             |
| `sync_codex`             | `force`                                                    | 仅触发 Codex 后台同步；完成后广播 `usage-sync` 事件            |
| `app_info`               | —                                                          | 应用名与版本（连通性检查）                                     |

桌面扩展命令：`get_settings` / `update_settings` 返回 SettingsSnapshot（AppSettings + 可恢复警告）；
更新使用字段 patch，防止覆盖并发保存的后台字段。`get_desktop_status` / `get_close_pending` / `resolve_close`
处理原生关闭流程。`clear_statistics` 在后台维护边界中暂停并清除统计；`reset_settings` 仅重置偏好；
`open_app_data_dir` 只打开固定 TokLens 目录。`settings-changed` 通知前端重读；统计清除复用 `usage-sync` 刷新事件。
手动与后台同步结束均重新读取完整数据源状态并更新页面查询版本；同一筛选刷新保留已成功数据，切换日期 / 客户端 / 页码时隐藏旧范围数据。订阅先于初始读取，进行中每两秒、空闲每三十秒与窗口聚焦时本地轮询兜底；请求代次丢弃过期响应。

更新命令：`get_update_state`（无网络）、`check_updates`（显式或已开启自动检查）、`download_update`
（只下载已检查版本并验证签名）、`install_update`（只接受内存中已验证的制品）。
`update-state` 广播阶段、版本、纯文本说明、下载字节数和可读错误，不发送本地用量或路径。

## 4. 客户端适配器规范

每个客户端一个独立模块（`codex`、`claude_code`、`opencode`、`dsh`、`kimi_code`、`mimocode`、`pi_desktop`，后续新增同理），并遵守：

1. **发现（discover）与解析（parse）分离**：发现函数定位候选文件；解析函数接收文件内容/路径，输出归一化事件，便于用 fixture 单测。
2. **只读**：以只读方式打开文件，绝不写入、移动或删除客户端数据。
3. **前向兼容**：未知字段忽略即可，不要因为新增字段而解析失败；已知字段缺失时按"缺失"处理，不猜测。
4. **可降级**：无法识别的文件计入"未解析文件数"并上报给 UI，不 panic、不静默丢弃。
5. **不落正文**：解析过程中不保存消息正文，只保留统计所需的元数据。
6. **大文件安全**：JSONL 类日志按行流式处理，避免整文件读入内存。
7. **fixture 驱动**：使用手工构造的合成记录及期望事件数 / token / 模型。小样本可在 `#[cfg(test)]`
   模块内构造；需要共享完整文件时放 `src-tauri/tests/fixtures/<client>/`，不得复制真实会话。

### 4.1 已实现的采集器

**Codex**（`src-tauri/src/collectors/codex/`）：扫描 `%USERPROFILE%\.codex\sessions` 下的 `rollout-*.jsonl`，
增量判定用 `(size_bytes, mtime_ms, parser_version)`，变化时整文件重新解析，并以单事务覆盖写回
（水位 + 会话累计值 + 该文件的请求明细）。新旧两套 Token 记录、累计值取值与隐私白名单见
`docs/data-model.md` 第 8 节。其他采集器的完整快照策略与格式限制见 `data-model.md` 第 10 节。

新增客户端时的检查清单：

- [ ] 新增 `<client>` 适配器模块；解析依赖归一化层，同步协调通过存储 API 导入
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

### 桌面布局、说明分层与更新提醒（2026-10-12）

应用框架复用现有 Sidebar、Popover、DropdownMenu 与 Dialog，采用单工具栏和可折叠图标导航；不手改 vendor 组件。
`AppSettings` / `SettingsPatch` 增加 `sidebarCollapsed`，默认展开，继续通过原生设置存储持久化并广播，不增加另一套偏好存储。
新安装、缺失字段与恢复默认的统计范围为 `today`，保留已有用户保存的范围。设置按六个分类呈现，托盘更新入口选择更新分类。
设置页在应用导航内使用固定的第二级分类侧栏，按应用 / 管理分组；800px 窗口仍保留分类导航，设置内容使用分隔行布局，不引入新的导航库或偏好存储。
图表浮层直接展示已有后端日桶总量、明细计数和完整度，前端不累加系列；详细采集覆盖与口径改为可点击的数据说明。
更新提醒消费已有 `UpdateState`，复用 Sonner；后台发现同一版本每次运行只提醒一次，手动检查每次反馈结果。`UpdatePanel` 统一设置页与更新弹窗的操作、进度和纯文本说明；侧栏保留可返回的更新入口，下载与安装仍各自由用户显式触发。
`use-updates` 统一手动检查反馈与进行中操作，使用同步 ref 防止连续点击并发请求；托盘不重新检查已下载的包。仅更新展示流程，无跨端协议变更。
自动检查仍默认关闭且每天最多一次，无新增依赖、权限、网络端点、数据库迁移或统计公式。

### 托盘状态、快捷操作与今日摘要（2026-10-12）

`desktop/tray.rs` 复用原生菜单、设置存储与采集状态，提供暂停 / 恢复、采集频率、页面入口及显式检查更新。
菜单只通过现有 `desktop/preferences` 更新设置，与设置页共用持久化与 `settings-changed` 广播；暂停不打断已开始的采集写入。
今日摘要由 `commands/tray_usage.rs` 调用已有请求时间查询，读取 TokLens 用量库，不读取客户端文件或会话正文。
`hooks/use-tray-usage.ts` 使用根级本地时钟提供当天零点和排他终点，仅计算日历边界；命令在后台查询并更新原生托盘 tooltip。
摘要随分钟、同步完成和清除事件刷新；页面数据与托盘摘要分离，后台同步不会强制页面重新加载。并发查询仅允许最新请求发布，避免旧结果覆盖新日期或清除后的摘要。
页面与更新快捷操作通过 `lib/api.ts` 的类型校验事件进入 React，更新复用官方签名 updater，只在用户点击后检查，不自动下载或安装。
不新增依赖、权限、网络端点、存储表或统计公式，继续沿用请求时间口径 2.3。

### 请求时间统计与元数据会话详情（2026-10-11）

在现有目录内新增 `usage/analytics/requests.rs` 与 `session_detail.rs`，分别处理请求时间聚合与单会话元数据分页；
`commands/request_queries.rs` 仅校验参数并调用领域查询，`request_dto.rs` 负责跨端转换。
前端 `lib/api.ts` 封装 `get_request_activity` / `get_session_detail`，类型位于 `lib/usage-types.ts`。
默认请求时间与保留的会话累计为独立查询路径：请求聚合不兜底累计值，会话累计仍只使用 sessions Token。
请求日桶接收本地日历零点边界，区间汇总与最多 400 日显示桶分离；覆盖计数忽略日期。
详情按客户端与会话 ID 查询已有统计库，不读取源文件正文；默认 100 条、最多 200 条，稳定排序与 offset 分页。
不增加目录、schema、迁移、依赖、权限、状态管理或网络能力。口径版本 2.3，详见 `data-metrics.md`。

### 首版热力图与逐日完整度（2026-10-11）

日历热力图复用 `get_daily_token_trend` 的总 Token / 会话数 / 请求数，客户端 × 日期热力图复用其 `clients` 的总 Token。
`CalendarHeatmap` / `ClientDateHeatmap` 只呈现 DTO，`HeatmapCell` 共用语义色阶、完整度标记与 tooltip。
总览日历默认查询最近 365 天，也可切换为跟随页面日期；客户端矩阵组件保留；2026-10-12 分析页已收束为请求用量对比与归因，不再显示矩阵。
`usage/analytics` 在已有每日查询中补充 `sessions_without_usage` 与 `sessions_partial`；命令层仅映射为 camelCase DTO。
`DailyTokenBucket` 返回 `sessionsWithoutUsage` / `sessionsPartial`，每个客户端日条目同时返回
`sessions` / `sessionsWithoutUsage` / `sessionsPartial`，前端仅依据这些元数据展示未知与不完整状态。
保留原趋势数值、会话开始日归属、本地零点起固定 24 小时桶及已知夏令时限制。
不新增 API、依赖、权限、数据库 schema 或迁移，不更改采集、去重与 Token 公式。

### DSH 版本化日志与失败隔离（2026-10-11）

DSH JSONL 采集兼容已核对的 v0–v4 统计元数据。`collectors/dsh/format.rs` 识别规范版本化文件名，
同目录只选择最高版本，旧代文件不重复统计；不回退到损坏或未知新版本之前的旧日志。
`metadata.rs` 仅反序列化统计白名单，`parser.rs` 解码文件、`projection.rs` 折叠请求尝试；按 v0/v1 的 seedLength 或 v2–v4 的继承标记排除种子。
新版本嵌入流的 usage 与最终 usage 按请求尝试替换，retry-started 划分独立尝试。
`usage/store` 新增事务合并快照入口：部分文件失败时只替换成功会话，保留失败及未确认删除的历史记录，
不推进失败文件水位；完整扫描仍使用全客户端原子替换。同步报告显式记录失败，界面显示部分失败。
不写入客户端文件，不新增依赖、数据库迁移或联网能力；回归测试全部使用合成日志。

### 持续集成与发布隔离（2026-10-11）

日常 CI 在 main 推送、PR 与手动触发时运行：Ubuntu 检查前端与 Python 发布脚本，
Windows x64、macOS Intel / Apple Silicon 分别执行 Rust 校验与无安装包原生生产构建。
统一使用 Node 22、pnpm 10、Python 3.12 和 Rust stable；缓存按原生目标区分。
日常 CI 只授予 contents: read，不读取签名 Secret、不发布制品；版本标签流水线才使用
更新签名 Secret，全部平台构建、签名校验与清单汇总成功后由独立任务正式发布 Release 并标记 Latest。CI 不改变应用默认离线行为。

### 按职责拆分后端模块（2026-10-11）

保留公开调用路径、DTO、数据库结构与统计口径；目录入口只负责模块组织与显式导出。

| 模块                      | 子模块职责                                                                                                                                                                                                                         |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `usage/normalize/`        | `codex`、`claude_code`、`opencode`、`dsh`、`kimi_code` 独立归一化；MiMoCode / Pi Desktop 共享 `request_ledger` 记录构造；`rollup` 对完整字段安全求和；`request_ledger` 负责 MiMoCode / Pi Desktop 按请求求和，测试随客户端规则组织 |
| `usage/time.rs`           | UTC 时间解析与系统时间读取，记录模型不承担日历计算                                                                                                                                                                                 |
| `usage/store/`            | 连接初始化、错误与存储类型、版本化迁移、单记录写入、事务导入、读查询分别组织；已发布迁移 SQL 保持不变                                                                                                                              |
| `usage/analytics/`        | 查询类型、公共分桶读取、区间/趋势/客户端聚合、排行、会话/数据源查询分别组织                                                                                                                                                        |
| `collectors/codex/`       | 文件发现与水位判断、白名单解析、线程累计聚合、同步协调与报告分别组织；只有同步协调调用存储写入 API                                                                                                                                 |
| `collectors/claude_code/` | JSONL 白名单解析、跨文件去重聚合与完整快照同步                                                                                                                                                                                     |
| `collectors/opencode/`    | 只读 SQLite 元数据查询与完整快照同步；每次读取 WAL 可见的事务快照                                                                                                                                                                  |
| `collectors/dsh/`         | v0 JSONL / Zstandard 解析、步骤 usage 替换与完整快照同步                                                                                                                                                                           |
| `collectors/kimi_code/`   | 会话目录发现、v2 / 旧版元数据、wire 1.4 / 1.5 白名单解析与原子快照同步                                                                                                                                                             |
| `collectors/mimocode/`    | SQLite WAL 只读事务、统计白名单投影、排除外部导入与快照替换                                                                                                                                                                        |
| `collectors/pi_desktop/`  | schema 1 JSONL 白名单、物理操作账本 / legacy 残差、指纹与原子快照                                                                                                                                                                  |
| `collectors/` 公共模块    | `files` 发现与指纹；`types` 同步协议；`sync` 独立调度各客户端；`test_support` 仅共享合成测试目录                                                                                                                                   |
| `commands/`               | `state.rs` 管理应用状态，`sync.rs` 调度后台同步，`error.rs` 封装命令错误；`dto.rs` 统一跨端结构，`queries.rs` 仅查询与参数校验                                                                                                     |
| `src/lib/usage-types.ts`  | 纯跨端类型；`api.ts` 负责调用与事件订阅，并兼容原类型导入路径                                                                                                                                                                      |

较大的测试模块按行为分组，共享合成数据构造器仅在 `cfg(test)` 下编译。不按行数机械拆文件，
优先隔离不同的变更原因；模块内部使用私有或 `pub(super)` 可见性，避免为拆分扩大公开接口。

### Token 领域模块边界（2026-10-11）

`usage/client.rs` 仅定义客户端标识与字符串映射；`usage/tokens/` 内按职责分为
`counts.rs`（原始计数、规范分桶、归一化结果）、`semantics.rs`（字段关系、版本化客户端口径与证据）、
`normalize.rs`（按口径换算分桶、总量选择与异常标注）。`tokens/mod.rs` 只组织模块与导出，
兼容已有 `usage::tokens` 调用路径。测试跟随对应职责，不复制换算算法。
本次为结构重构，不改变字段、口径版本、统计公式、数据库或 DTO。

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
| 2026-10-11 | 日期时间选择器复用 shadcn Popover 与原生分钟输入，根级 Hook 统一滚动预设时间      | 已采纳 | 无新依赖；自定义确认后固定，精确范围与每日显示桶独立                    |
| 2026-10-12 | Token 展示偏好复用原生设置，通过 React 内置 Context 传递给展示组件                | 已采纳 | 默认自动单位，支持用户选择；仅改变格式，无新增状态库或统计计算          |

待登记（引入前补一行）：前端状态管理、前端测试框架、错误类型库（当前 `UsageError` 为模块内自定义枚举）、价格表来源。

### Token 显示偏好（2026-10-12）

`AppSettings.tokenDisplay` 使用已有原生 JSON 设置存储，缺失字段默认 `auto`，支持 `chinese` / `k` / `m` / `b` / `exact`；恢复默认回到自动单位。
`hooks/use-token-format.ts` 使用 React 内置 Context 将根级设置传递给展示组件，`lib/format.ts` 仅格式化显示，不改变原始计数、统计口径、DTO 用量字段或 SQLite schema。
日期预设移至统计页面主工具栏，与标题及同步操作共用一行，复用 `DateRangePicker` 即时解析；精确区间为控件内辅助文字，自定义通过日历按钮确认后仍保持固定。无新增依赖、权限或联网能力。

### AI 品牌图标（2026-10-11）

| 日期       | 决策                                                                  | 状态   | 理由 / 影响                                |
| ---------- | --------------------------------------------------------------------- | ------ | ------------------------------------------ |
| 2026-10-11 | 引入用户指定的 `@lobehub/icons`，品牌标识统一由 `ClientIdentity` 呈现 | 已采纳 | 本地 SVG、统一品牌色；依赖与体积影响见下文 |

按用户指定的 [LobeHub 接入说明](https://icons.lobehub.com/skills.md)，引入 `@lobehub/icons`，
由 `ClientIdentity` 统一使用 `Codex`、`ClaudeCode`、`OpenCode`、`DeepSeek`（DSH）的单色 SVG 组件。
业务组件仍通过 shadcn/ui 组合布局；通用操作与导航继续使用 lucide-react。
品牌图标使用现有 `client-*` 语义颜色，随应用本地打包，不使用 CDN、远程图片或遥测。

依赖来自 npm 官方 registry；LobeHub 开源维护、MIT 授权，采用当前维护版本 `5.23.0`
（npm 最近更新 2026-10-10）。包发布体积约 9.5 MB（未压缩，包含全图标集），标记 `sideEffects: false`；
本次生产 JS 从 822.85 kB 增至 871.44 kB（+48.59 kB），gzip 从 250.52 kB 增至 260.74 kB（+10.22 kB）。
其声明的 `antd` / `@lobehub/ui` peer 依赖由包管理器解析，
仅服务图标库的兼容入口，不作为 TokLens 的业务 UI 组件库使用。

该版本的传递依赖 `@emoji-mart/react@1.1.1` 声明 React 16–18 peer，与项目 React 19 存在安装提示。
TokLens 不使用 emoji picker；保留真实提示，不降级 React 或伪造 peer 兼容规则，升级图标库时复查。

### UI 组件结构（2026-10-11）

`FilterSelectContent` 组合已有 shadcn SelectContent，统一客户端筛选浮层的定位、宽度与选项间距，生成组件保持不变。

业务组件和页面采用 PascalCase 文件名；`DashboardCard` 组合 shadcn Card 统一面板外观，`ClientIdentity` 提供稳定客户端标识，`StatCard`、`TokenTrendChart`、`ClientUsageCard`、`RankingCard`、`SessionTable` 分别负责呈现。图表采用 shadcn Chart + 已有 Recharts，无新增运行时依赖。统计查询与 DTO 保持不变，费用来源未落地前 Tooltip 显示暂不可用，不反推金额。

### 多客户端 DTO 与查询（2026-10-11）

`sync_usage` 为前端统一同步入口；手动与定时采集完成均刷新当前页面数据，同一筛选原位更新并保留页码。`SyncReport.clients` 返回每客户端结果，避免把某个缺失目录当成全部同步失败。
`get_overview_stats` 增加可选 `client`，与趋势 / 排行筛选一致。
`DailyTokenBucket.clients` 返回客户端逐日总量；`TokenTotals.countedCacheWriteTokens` 由领域查询计算，仅包含计入总量的缓存写入（Claude Code / DSH / Kimi Code / MiMoCode / Pi Desktop）。
`ClientUsage.shareBps` 由领域查询按已知规范总量计算，10000 表示 100%，前端只格式化，不重新聚合。
共享 DTO 类型仍在 `usage-types.ts`，`sync_codex` 保留仅 Codex 的语义。数据库 schema 保持 v1，无需迁移。

### 统一日期时间筛选（2026-10-11）

`DateRangePicker` 采用一级 Button 日期预设，以及已有 shadcn Popover、Button、Input 与原生 `datetime-local` 分钟输入组合日期时间选择器，不新增日期库或修改 vendor 组件。
自定义范围在确认后应用；根级时间 Hook 为所有统计页面提供同一当前时间，滚动预设统一刷新，已确认自定义范围保持固定。
`hooks/use-local-clock.ts` 在分钟边界、窗口获得焦点和页面恢复可见时刷新时间，统一由 `App` 解析范围；编辑中的草稿不随时钟刷新。
本周从本地周一零点起、本月从本地月初零点起。全局范围共享给总览、用量分析及会话明细。
`get_daily_token_trend` 的 `fromMs` 是精确过滤开始，可选 `toMs` 为排他结束；`dayStartMs` 单独提供本地零点显示桶起点，省略时回退至 `fromMs`，兼容旧调用。
命令层只校验结束晚于开始并将参数映射给领域查询；领域同时筛选会话总量与所属请求数。显示桶仍固定 24 小时，最多 400 天，无 DTO / schema / 权限 / 依赖变化。
超长选择仅截断每日图表至最近 400 天并注明实际日期，区间指标查询完整范围；日历最近 365 天模式独立保留。

### 用量分析收束（2026-10-12）

用量分析使用独立 `get_request_analysis` 查询：`commands/request_queries` → `usage/analytics/requests` → SQLite。
请求分析复用已有请求时间过滤与分桶语义，返回完整区间客户端对比、已知请求均量、缓存命中率、Token 分项与模型 / 项目排行。
全客户端比较不受下钻客户端影响；分项与排行使用选中客户端。模型按会话主模型归属，不能称为逐请求模型。
所有聚合与比例在 Rust 领域层提供，查询共用读取事务快照；前端只排序、格式化、绘制相对条形。
新增业务展示组件 `RequestAnalysisPanel`、`RequestAnalysisDetails`，复用现有 Card / Table / Progress / Tabs，不新增目录、依赖、权限或联网能力。

### 采集可靠性与查询一致性（2026-10-12）

Codex 根目录优先使用非空 CODEX_HOME，其次 USERPROFILE / HOME 下的 .codex；仅扫描 sessions。路径发现仍位于 collectors，command 为 ClientSourceStatus DTO 附加 sourcePath；不在 usage 领域读取环境或源文件。
解析 I/O 失败或未完成的无换行 JSON 尾行返回失败，整文件放弃提交；读后与汇总提交前复核大小 / 修改时间，失败保留已有统计与成功水位。完整损坏行仍计数并忽略，合法无换行末行可导入。
StatsBucket 增加 cacheHitBps / cacheEligibleSessions；ClientUsage 和 RankingEntry 增加 sessionsWithoutUsage；SyncReport 透传 directoriesFailed。数据库 schema、权限、依赖不变。会话列表复用 offset，50 条一页并多取一条判断下一页；排序补客户端作为同时间同 ID 的稳定次序。
