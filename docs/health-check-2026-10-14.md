# TokLens 全面体检报告（2026-10-14）

本报告由 8 名审查员工（opencode / DeepSeek V4.1 Flash，high 思考档，只读模式）分域并行审查，协调员对账去重并抽验关键结论后汇总。基于 `v0.1.11`（HEAD `b2628d6`，与 `v0.1.11` 标签一致）。**本次体检为只读审查，未修复任何问题**；唯一执行的写操作是 `pnpm check` / `pnpm rust:check` 产生的构建缓存（`git status` 保持干净）。

严重度口径：

- **P0**：数据损坏 / 隐私泄露 / 安全漏洞 / 崩溃或启动失败 / 重复计数或漏计（已触发）。
- **P1**：功能性错误、明确的规范强制条款违反、边界缺陷、文档与实现明确不一致。
- **P2**：质量、一致性、可维护性改进。

## 一、总体结论

| 域                          | 结论               | P0  | P1  | P2  |
| --------------------------- | ------------------ | --- | --- | --- |
| 前端架构                    | 良好               | 0   | 0   | 9   |
| UI/UX 规范                  | 良好（B+）         | 0   | 1   | 12  |
| Rust 核心                   | 良好偏优           | 0   | 1   | 5   |
| 采集器 A（codex 等 6 个）   | 良好（Codex 最弱） | 0   | 4   | 7   |
| 采集器 B（copilot 等 6 个） | 良好偏上           | 0   | 2   | 8   |
| 数据口径与隐私              | 高                 | 0   | 3   | 6   |
| 安全·构建·CI                | 良好               | 0   | 1   | 4   |
| 文档一致性                  | 良好               | 0   | 3   | 7   |

- **无 P0。** 未发现已触发的数据损坏、隐私泄露、安全漏洞或崩溃；`pnpm check` 与 `pnpm rust:check` 全绿（243 单测 + 1 集成测试通过）。
- **P1 共 15 条**（去重后），其中 6 条为工程可靠性问题（建议优先修复），2 条为规范落实，7 条为文档与实现不一致（可批量修复）。
- 核心承诺（本地优先、实测/推算/估算分离、隐私红线、幂等去重）在代码层均有硬实现，且与口径文档 `2.12` 基本对齐。

## 二、校验基线（体检-7 执行的客观结果）

| 命令              | 结果           | 摘要                                                                                   |
| ----------------- | -------------- | -------------------------------------------------------------------------------------- |
| `pnpm check`      | 通过（exit 0） | eslint 无错；tsc（src + node）无错；prettier 全合规                                    |
| `pnpm rust:check` | 通过（exit 0） | rustfmt 通过；clippy `-D warnings` 零告警；243 单测 + `updater_signature` 集成测试全过 |

## 三、P1 发现清单

### 可靠性类（建议优先修复）

**P1-1 [采集器·Copilot] 存在"仅 Host、SDK 事件已丢失"的会话时整客户端同步永久失败**

- 证据：`src-tauri/src/collectors/copilot/sync.rs:157-161` 对残留 `host` 直接 `return Err(SyncError::ChangedDuringScan)`；文档 `docs/data-model.md:432` 要求的是"暂不采集"（跳过该记录、导入其余）。
- 影响：一旦宿主库出现孤立 Host 会话（events 轮转/清理后很可能发生），Copilot 采集每次同步都整体失败，永远停摆。
- 建议：残留 host 改为计数诊断后跳过；仅在扫描期真实变化时使用 `ChangedDuringScan`；补 orphan-host 合成用例。

**P1-2 [Rust·同步线程] 无 panic 兜底，崩溃后退出挂起且同步静默失效**

- 证据：`src-tauri/src/commands/sync.rs:94-133` 线程体以 `finish_sync()` 收尾且全仓无 `catch_unwind`；`src-tauri/src/commands/activity.rs:36-45` `shutdown()` 自旋等待 `syncing`。
- 影响：任一采集器 panic 会使 `activity.syncing` 永久为 true——`shutdown()` 永久阻塞（应用无法退出）、后续同步全部静默失效。
- 建议：`Drop` 守卫或 `catch_unwind(AssertUnwindSafe(...))` 包裹线程体，保证 `finish_sync()` 必执行。

**P1-3 [采集器·Codex] 缺少 `valid_counts` 防护，配合 `opt_i64` 回绕可写入负数 token**

- 证据：`src-tauri/src/collectors/codex/parser.rs:346-359` 无 `valid_counts`（其余 5 个采集器均有）；`src-tauri/src/usage/store/writes.rs:31-33` 的 `opt_i64` 直接 `value as i64`。
- 影响：损坏/异常文件中超过 `i64::MAX` 的计数会静默回绕为负值写库，破坏求和与占比。
- 建议：Codex 两条计数路径统一走 `valid_counts`；`opt_i64` 增加 `<= i64::MAX` 过滤（超出记 NULL/未知），与 `rollup.rs`、`request_ledger.rs` 的既有防护对齐。

**P1-4 [采集器·Codex] 续接文件继承累计值，父子会话同库时重复计数（条件触发）**

- 证据：`src-tauri/src/collectors/codex/thread.rs:39-41` 仅记录 `parent_thread_id`；`src-tauri/src/usage/normalize/codex.rs:90-101` 把累计值直接作为会话总量；调研文档自述风险：`docs/data-source-research.md:113-115`。
- 影响：父线程与子线程同时被导入时，两会话都包含父线程那批 token，按 `sessions` 求和将重复计入（触发即为 P0 级重复计数；本机数据未触发）。
- 建议：用父线程已知累计值做差，或标注继承关系并在口径文档登记合计规则；补"父子同库"合成 fixture。

**P1-5 [采集器·OpenCode/MiMoCode] 直接只读打开客户端活跃 WAL 库，可能触碰 `-shm`（只读边界）**

- 证据：`src-tauri/src/collectors/opencode/sync.rs:44-48`、`src-tauri/src/collectors/mimocode/sync.rs:53-58` 以 rusqlite `READ_ONLY` 直接打开源库；而 `sqlite_snapshot.rs:1` 与 `docs/data-model.md:431/455` 明确"不打开客户端 SHM"。
- 影响：WAL 库的只读连接在 `-shm` 缺失/不可写时可能尝试创建或写入 `-shm`（需目录写权限），与"绝不写入"存在边界冲突；受限目录下还可能直接失败。
- 建议：迁移到"文件读取 + 内存合成快照"路径（与 MCode/Copilot 相同），或在文档中声明该边界。

**P1-6 [数据分级·Pi Desktop/WorkBuddy] 派生总量被标记为 `measured`**

- 证据：`src-tauri/src/usage/normalize/request_ledger.rs:44-45` 固定 `SourceKind::Measured`；对照 Copilot/Gemini/MCode 在 `derived_from_buckets` 时会改标 `Derived`（`normalize/copilot.rs:113-116` 等）。
- 影响：`messages` 无 `total_basis` 列，`source_kind` 是请求级唯一分级标识；Pi 旧记录与 WorkBuddy 缺 `totalTokens` 但分桶完整时属确定性换算，却计入 `measured` 统计，违反 AGENTS.md 第 6.3 节"分级可溯源"。
- 建议：helper 接受 `total_basis`，派生总量时同步置 `Derived`。

### 规范落实类

**P1-7 [UI] 更新面板错误文案违反 `text-destructive` 约定**

- 证据：`src/components/UpdatePanel.tsx:71-75` 使用 `text-muted-foreground`；对照 `docs/ui-guidelines.md:90`"错误用内联提示（`text-destructive`）+ 可重试操作"。
- 建议：改为 `text-destructive`（保留 `role="alert"`）。

**P1-8 [安全] `opener:default` 权限范围超出功能所需**

- 证据：`src-tauri/capabilities/default.json:6` 授予 `opener:default`（含任意 `http(s)/mailto/tel` URL 与 `reveal-item-in-dir`）；前端仅需一个固定链接：`src/lib/api.ts:34`。
- 影响：违反 AGENTS.md 第 5 节最小权限；受严格 CSP 约束实际可利用性低，属纵深防御缺口。
- 建议：改为 `opener:allow-open-url` + scope 限定 `https://github.com/MY-Final/TokLens*`。

### 文档与实现不一致类（可批量修复）

**P1-9 [文档] `date-fns` 未登记，且 ADR 有"不新增日期库/无新依赖"过时表述**

- 证据：`docs/architecture.md:472`、`:399`；`package.json:32` 已引入 `date-fns@^4.4.0` 并在 `src/lib/ranges.ts:2` 等使用。
- 建议：ADR 补登记并修正过时表述。

**P1-10 [文档] `AGENTS.md` 目录表列出不存在的 `public/`**

- 证据：`AGENTS.md` 第 3 节目录表；仓库根目录无 `public/`（已核实）。
- 建议：删除该行或改述实际静态资源位置。

**P1-11 [文档] `data-model.md:423` 引用不存在的 `collectors/copilot/database.rs`，且与同文件 `:412` 自相矛盾**

- 证据：`docs/data-model.md:423` 写 `{database,events,agent_host,agent_host_tests}.rs`，而 `:412` 写 `{queries,sync,tests}.rs`；`collectors/copilot/` 实际无 `database.rs`（`mod.rs:3` 是 `sqlite_snapshot` 别名）。`docs/architecture.md:47` 的同类描述正确。
- 建议：改为 `events` / `agent_host` / `agent_host_tests` + 共享 `collectors/sqlite_snapshot.rs`。

**P1-12 [文档] `ui-guidelines.md:208` 的 counted-cache-write 客户端清单缺 Copilot/MCode**

- 证据：`docs/ui-guidelines.md:208` 列 5 个；实现为 7 个：`src-tauri/src/usage/analytics/sql.rs:7`（已核实）。
- 建议：改为"以 `TokenSemantics::cache_write_counts_in_total` 为准"或补全 7 客户端。

**P1-13 [文档] `data-model.md:92-96` 的 `total_basis` 归属表缺 Gemini/MCode/Grok**

- 证据：`docs/data-model.md:95-96`；实现见 `normalize/gemini_cli.rs:115`、`normalize/mcode.rs:54`（`sum_of_messages`）与 `normalize/grok.rs:142`（`client_reported`）。
- 建议：补齐三行并注明 Copilot 双分支。

**P1-14 [文档] `data-metrics.md:63-64` 字段名 `source`/`confidence` 与实现 `source_kind` 不一致**

- 证据：`docs/data-metrics.md:63-64` vs `usage/model.rs:142,165`、DB 列 `source_kind`（`initial-schema.sql:28,59`）；全仓无 `confidence` 字段。
- 建议：统一为 `source_kind`/`sourceKind`，标注 `confidence` 为预留、当前未落库。

**P1-15 [文档+代码] "未变化跳过"在 Claude Code / Kimi Code 上因排序不一致可能恒不命中**

- 证据：`collectors/claude_code/sync.rs:61-65`、`collectors/kimi_code/sync.rs:61-65` 直接用 PathBuf 顺序比较 SQLite `ORDER BY path`；`collectors/dsh/sync.rs:59-66` 显式 `sort()` 并注释了 Windows 排序差异（已核实）。
- 影响：Windows 上多文件客户端可能每次全量重解析（性能；与 `data-model.md:259/323` 声称的跳过行为不符）。
- 建议：对齐 DSH 的 `source_paths.sort()`。

## 四、P2 发现清单（质量/一致性，共约 58 条）

### 前端（体检-1）

- 客户端 12 键在图表组件硬编码 4 处（新增客户端会被静默遗漏）——`RequestActivityPanel.tsx:31-44`、`TokenTrendChart.tsx:29-67/115-139`、`ClientDateHeatmap.tsx:13-26`。
- 类型级直引 `@tauri-apps/api`（api.ts 未重导 `UnlistenFn`）——`use-data-source-status.ts:3`。
- 死代码：`ClientDateHeatmap` 整组件、`LoadingInline`、`formatCompactOrDash`、`formatPercent`、`formatMonthDay`——`StateBlocks.tsx:19-30`、`format.ts:62-73/88-92`。
- 会话搜索无防抖，每键触发一次查询——`SessionFiltersToolbar.tsx:20-26`、`SessionsPage.tsx:108-127`。
- 启动时对历史空闲同步状态误增 `refreshToken`，首屏重复查询——`use-data-source-status.ts:26-36/44`。
- `useSettings.patch` 保存期间静默丢弃并发补丁——`use-settings.ts:48-63`。
- `UsageTooltip` 用标题字符串 `includes` 推断标签（脆弱契约）——`UsageTooltip.tsx:39`。
- 后端枚举以 `String` 序列化、前端索引无兜底——`dto.rs:324-325`、`request_dto.rs:197`。
- `api.ts` 重复 `SessionSort` 联合与内联 DTO——`api.ts:233/38`。

### UI/UX（体检-2）

- 存储目录占位符反斜杠转义错误（渲染为 `D:TokLensData`）——`DataSettings.tsx:203`（已核实）。
- 后台刷新失败时错误块覆盖仍有效的旧数据（7 处消费方）——`use-async-data.ts:40-45` 及 `TokenTrendChart/CalendarHeatmap/ClientUsageCard/RankingCard/SessionTable/RequestActivityPanel`。
- 采集开关 `aria-label` 使用原始 client key——`SourcesPage.tsx:178`。
- 工具栏首行未与正文 `max-w-[1600px]` 对齐——`AppShell.tsx:164`。
- 4px 网格散值：`pl-5.5`、`pr-1.25`、`text-[10px]`——`RequestAnalysisDetails.tsx:177`、`SettingsFields.tsx:113`、`StatCard.tsx:52`。
- `formatRelative` 使用 `Date.now()` 而非注入时钟——`format.ts:99`。
- 请求矩阵使用原生 `<table>` 而非 shadcn Table——`RequestActivityPanel.tsx:473-521`。
- 亮色主按钮对比度约 3.3:1（AA 需 4.5:1）——`index.css:65-66`。
- 热力图命中区 16px（WCAG 2.5.8 建议 24px）——`HeatmapCell.tsx:47-48`。
- vendor 统一从裸包 `cn` 导入且未登记——22 个 `components/ui/*` 文件。
- `SessionTable.tsx:85` 固定 aria-label"会话明细"用于总览；`dialog.tsx:76` 英文 "Close"。
- `createCustomRange` 在 `OverviewPage.tsx:207-214` 未捕获抛错（理论边界）。

### Rust 核心（体检-3）

- 会话列表未知 client 回退 `Client::Codex`（应跳过）——`analytics/records.rs:74`（已核实）。
- 迁移无跨连接互斥（未来 v2 的潜在竞态）——`connection.rs:19-28`、`migrations.rs:23-53`。
- 同步型 command 在主线程执行 SQL（重查询可能卡 UI）——`commands/queries.rs`、`request_queries.rs` 多处。
- `client_usage` N+1 查询——`analytics/aggregates.rs:194-211`。
- `is_supported` 与 `Client::ALL` 重复、settings 双重 validate、`client_total_tokens` 语义偏差——`commands/dto.rs:130-146`、`settings/mod.rs:101-102`、`store/writes.rs:186/283`。

### 采集器（体检-4/-5 合并）

- Codex 删除 rollout 不清水位/明细——`codex/sync.rs:37-145`。
- Codex 不扫 `archived_sessions`、未接 `state_5.sqlite` 快路径——`codex/sync.rs:20-26/148-163`。
- Codex 新旧格式镜像配对低概率误判——`codex/parser.rs:65-76`。
- Kimi 同 `meta.id` 多目录导致整源失败——`kimi_code/sync.rs:70-101`、`store/snapshot.rs:55-69`。
- Kimi/Claude 单坏文件中止整客户端同步——`kimi_code/sync.rs:72/79`、`claude_code/sync.rs:72`。
- WorkBuddy `partial` 全客户端级污染所有会话——`workbuddy/sync.rs:89-92`（已核实）。
- Gemini 嵌套发现丢弃子级 report——`gemini_cli/discovery.rs:36-43`。
- MCode 忽略 `SyncOptions` 恒全量——`mcode/sync.rs:33-37`。
- `discover_files` 不防 symlink/junction——`collectors/files.rs:28-33`。
- Copilot 同 SDK id 多文件整源失败——`copilot/sync.rs:94-99`。
- Copilot CLI 用量表列缺即整源失败（无列级兼容）——`copilot/queries.rs:71-73`。
- `sqlite_snapshot` 只处理 WAL、不处理 rollback journal——`sqlite_snapshot.rs:51-72`。
- Grok 未知字段静默忽略且不计数——`grok/parser.rs:11-29,72-80`。
- 输入健壮性细节：`home_dir` 未过滤空串、`MIMOCODE_DB` 在无 HOME 时被忽略、时间未拒绝未来/负值、`messages_inserted` 语义为"写入数"——`files.rs:6-10`、`mimocode/sync.rs:17-31`、`time.rs:23-30`、`snapshot.rs:127`。

### 数据口径与隐私（体检-6）

- counted cache-write 集合硬编码两处（潜在漂移点）——`analytics/sql.rs:7`、`analytics/requests.rs:134`。
- `normalize/claude_code.rs:55-56` 注释与 `sum_complete_counts` 实现不符。
- 会话级 `source_kind` 未进列表 DTO（将来估算启用后无法标注）——`analytics/types.rs:100-118`、`commands/dto.rs:310-327`。
- 报告 UTC 日期分组使用浮点除法（理论跨日舍入）——`analytics/report.rs:36`。
- 隐私哨兵测试仅 Codex 命名覆盖（其余走共享 `assert_private`）——`codex/tests/privacy.rs:77-139`。
- Codex 无序号回退把绝对路径写入 `source_ref`/`dedup_key`（已文档化的隐私权衡）——`normalize/codex.rs:53-54/133`。

### 安全·构建·CI（体检-7）

- `.gitignore` 未忽略 `.env`——`.gitignore:1-38`。
- `release.yml` 标签触发 `v*.*.*` 宽于稳定版（非稳定标签会拉起 3 个构建后失败）——`release.yml:4`（已核实）。
- CSP `img-src` 放行未启用的 `asset:` 协议——`tauri.conf.json:25-26`。
- `@lobehub/icons` 传递安装 `antd`（~47MB）等未记入 ADR 体积说明——`architecture.md:447-454`。

### 文档一致性（体检-8）

- `docs/README.md` 索引漏列 `product-review-2026-10-12.md`——`docs/README.md:7-13`。
- `data-source-research.md:3` 自述"三个客户端"（实际 12）。
- `data-model.md` 模块表漏 `usage/report.rs`——`data-model.md:6-16`。
- ADR 未登记 `cmdk`/`radix-ui`/`react-day-picker`；`next-themes`/`sonner` 仅提及——`architecture.md:382-400`。
- `architecture.md` 模块表滞后于 2026-10-11 后新增文件——`architecture.md:353/365`。
- `AGENTS.md` 目录表未列 `desktop/`、`settings/`、`updates/`。
- `product-review-2026-10-12.md:15` "本批未包含…"易被误读为至今未做——`product-review-2026-10-12.md:15`。

## 五、已核验通过（关键项）

- **校验基线**：`pnpm check` 与 `pnpm rust:check` 全绿；clippy 零告警；243 单测 + 1 集成测试通过。
- **架构与契约**：依赖方向单向（前端 `api.ts` 唯一 Tauri 入口、`usage` 不反向依赖）；前后端 DTO 逐字段对齐（12 客户端键、枚举字符串、可选性）；TS 严格项无违规（无 `any`/`@ts-ignore`/`console.*`）。
- **安全**：无遥测依赖、registry 为 npm 官方、lockfile 同步；CSP 严格（无 `unsafe-eval`/远程脚本）；updater 签名 + 固定端点 + 默认离线 + 下载 URL 二次校验；无密钥/正文入库；发布流程（签名矩阵、`--verify-tag --latest`）与 `docs/releases.md` 一致。
- **隐私**：全仓无日志宏；采集器字段白名单 + 只读打开 + 内存快照；错误信息常量化脱敏；一键清除仅删统计表且先暂停采集；导出仅统计元数据（含 CSV 公式注入防护），`METRICS_VERSION=2.12` 与文档一致。
- **数据口径**：实测/推算/估算三级分离并在聚合层过滤；去重键与 SQL 唯一约束逐条对应（`sessions`/`messages`/`source_files` 三级 UNIQUE + UPSERT）；缓存命中率为成对样本口径；分母统一为"已知样本"；成本"未实现"状态与文档一致。
- **测试**：采集器 fixture 全为合成数据，覆盖空/坏行/截断/追加/删除/重启/损坏/溢出/隐私哨兵；跨表写事务原子、失败回滚有测试。
- **协调员抽验**：对 15 条关键断言做了独立复核（含 `records.rs:74`、`writes.rs:31-33`、`capabilities/default.json:6`、`architecture.md:472`、`package.json` date-fns、`data-model.md:423`、`copilot/sync.rs:157-161`、`workbuddy/sync.rs:89-92`、`DataSettings.tsx:203`、`claude_code/kimi_code` 排序、`sql.rs:7`、`request_ledger.rs:44`、`release.yml:4`、`ui-guidelines.md:208`、`data-model.md:92-96`），全部属实；并修正体检-8 的一处归属错误（`database.rs` 引用位于 `data-model.md:423`，而非 `architecture.md:423`）。

## 六、存疑与无法验证（合并）

1. **Codex 父子会话重复计数**的现实触发频率：本机数据中父线程不在 `threads` 表，无法实证（P1-4 由机制成立、条件触发）。
2. **OpenCode/MiMoCode 的 `-shm` 行为**依赖 SQLite 版本与目录权限，未在不动真实客户端目录的前提下复现（P1-5）。
3. **GitHub Action 大版本**（`checkout@v7` 等）无法离线核实是否存在，建议联网跑一次 `workflow_dispatch` 验证。
4. **Tauri 非 async command 主线程执行**为官方模型推断（P2 的 UI 卡顿风险），建议以大数据量实测确认。
5. **估算链路**（E1–E4）只有类型与过滤、无生产者，无法实测；与文档声明一致。
6. **历史验收数字**（测试数、安装包大小）为时点快照，无法在当前只读环境复现。
7. **UI 对比度/滚动行为**为静态推算，无前端测试框架与浏览器实测；运行期表现待验证。
8. **`data-source-research.md` 的采集器层字段映射**未逐客户端全量比对（体检-6 抽样核对 Codex），建议下一轮专项。

## 七、建议修复路线（分批）

- **第 1 批（可靠性，优先）**：P1-1 Copilot 孤立 Host、P1-2 同步线程 panic 兜底、P1-3 Codex `valid_counts` + `opt_i64` 防护；同步评估 P1-4/P1-5 的取舍（重复计数与只读边界）。
- **第 2 批（口径与规范落实）**：P1-6 分级标注；P1-7 错误色；P1-8 opener scope；P2 中 `counted cache-write` 硬编码收敛与断言测试。
- **第 3 批（文档一致性批量修复）**：P1-9 至 P1-14、P2-文档类 7 条，一次性对齐 `AGENTS.md`/`docs/**` 与实现。
- **第 4 批（质量/性能）**：搜索防抖、工具栏对齐、4px 散值、N+1 与主线程查询、死代码清理、客户端键收口等 P2。

## 八、修复实施状态（2026-10-14 加班批次）

8 名员工转入 build 模式，按严格文件归属并行修复，完成后全量门禁复验通过（`pnpm check` 全绿；`pnpm rust:check` 全绿：rustfmt + clippy 零告警 + 259 单测 + 1 集成测试）。**尚未 commit**，改动保留在工作区（66 个文件，+1235 / -544）。

### 已修复（P1，14/15）

| 编号    | 内容                           | 落地                                                                                     |
| ------- | ------------------------------ | ---------------------------------------------------------------------------------------- |
| P1-1    | Copilot 孤立 Host 不再整源失败 | 复用 `files_without_identity` 计数并继续提交 + 合成测试                                  |
| P1-2    | 同步线程 panic 兜底            | `FinishSyncGuard`（Drop 释放，含 `catch_unwind` 单测）                                   |
| P1-3    | Codex 溢出防护 + `opt_i64`     | 两条解析路径 `valid_counts`；`writes.rs` 超 `i64::MAX` 写 NULL + 双测试                  |
| P1-5    | OpenCode/MiMoCode `-shm`       | 迁移到 `sqlite_snapshot` 内存快照 + "不创建 `-shm`" 回归测试（迁移量低于预算，实施成功） |
| P1-6    | Pi/WorkBuddy 分级              | 新增 `classify_request_source`/`classify_session_source`，派生总量标 `Derived` + 测试    |
| P1-7    | UpdatePanel 错误色             | `text-destructive`                                                                       |
| P1-8    | opener 权限收窄                | capability 精确 scope 到单一链接（`rust:clippy` 经 build.rs 校验通过）                   |
| P1-9~14 | 文档一致性 6 项                | architecture / AGENTS / data-model / ui-guidelines / data-metrics / README 全部对齐      |
| P1-15   | Claude/Kimi 未变化跳过         | 比较前对齐 DSH 的 `sort()` 序                                                            |

### 已修复（P2 摘要）

- **前端**：客户端键收口 `CLIENT_KEYS`（删除 4 处硬编码）、删除 `ClientDateHeatmap` 整组件与死代码、`UnlistenFn` 重导出、搜索 250ms 防抖、启动误增 `refreshToken` 修复、设置补丁合并队列（last-write-wins）、tooltip 显式标签、枚举 `?? value` 兜底、请求矩阵改 shadcn Table。
- **UI**：占位符转义、采集开关 aria、工具栏首行对齐、4px 散值 ×3、主色对比度（`#0a8168`，白字 4.82:1）、热力图命中区 24px、会话表 `ariaLabel` prop、`createCustomRange` 守卫。
- **Rust 核心**：未知 client 跳过（不再伪装 codex）、`is_supported` 收口 `Client::ALL`、移除双重 validate、client_usage N+1 合并、counted cache-write 一致性测试（`include_str!` 校验 SQL 字面量）。
- **采集器**：Copilot 重复 SDK 择新、CLI 缺列兼容（PRAGMA 白名单投影）、Gemini 嵌套 report、symlink/junction 防护、`home_dir` 空值过滤、`MIMOCODE_DB` 绝对路径优先、WorkBuddy partial 按文件隔离、Claude 注释修正。
- **安全/构建/文档**：`.env` 忽略、release 非稳定标签门控（新增廉价 validate job）、文档 11 项 + 附带对齐 1 项。

### 缓做（已记录，需口径/设计决策或联调）

- **P1-4** Codex 父子继承重复计数：评估结论为"缺少可信的续接时刻前缀数据"，需口径决策（方案 A：承认客户端语义并在文档说明重叠；方案 B：采集期做差并标 `derived`，需登记 `data-metrics` 并新增 fixture）。
- 镜像配对启发式加强、`archived_sessions`/`state_5.sqlite` 接入、rollout 删除剪枝、Kimi/Claude 单坏文件中止行为、迁移互斥、主线程查询改 `async`、会话级 `source_kind` 进 DTO、Grok 未知字段计数、MCode 未变化短路、`sqlite_snapshot` rollback journal 注释、UI background-refresh 错误覆盖改造、`cn` vendor 导入与 dialog 文案（vendor 不动）、CSP `asset:` 清理。

### 复验记录

- `pnpm check`：eslint / tsc（src + node）/ prettier 全部通过。
- `pnpm rust:check`：rustfmt ✓；clippy `-D warnings` 零告警；259 单测 + `updater_signature` 集成测试全过（较修复前新增 15 个测试用例）。
- 已做 diff 抽验：panic 守卫 Drop 时序、快照迁移、防抖边界、补丁队列、capability scope 与实际建议一致。

## 附录：体检编制与分工

| 员工                  | 范围                                                                    | 备注                      |
| --------------------- | ----------------------------------------------------------------------- | ------------------------- |
| 体检-1 前端架构       | `src/pages`、`components`、`hooks`、`lib`                               | 0 P0/P1                   |
| 体检-2 UI/UX 规范     | 全部界面 + `index.css`，对照 `ui-guidelines.md`                         | 1 P1                      |
| 体检-3 Rust 核心      | `usage/**`、`commands/**`、`desktop/settings/updates`                   | 1 P1                      |
| 体检-4 采集器 A       | codex / claude_code / opencode / dsh / kimi_code / mimocode             | 3 P1                      |
| 体检-5 采集器 B       | copilot / gemini_cli / grok / mcode / pi_desktop / workbuddy + 公共模块 | 2 P1                      |
| 体检-6 数据口径与隐私 | 对照 `data-metrics.md` / `data-model.md` 全量核对                       | 3 P1                      |
| 体检-7 安全·构建·CI   | capabilities / CSP / 依赖 / workflows / release.py；执行两项校验命令    | 2 P1                      |
| 体检-8 文档一致性     | `AGENTS.md`、`docs/**`、README、RELEASE_NOTES                           | 3 P1                      |
| 协调员                | 派单、对账去重、抽验、汇总                                              | 抽验 15 项，修正归属 1 处 |

审查方式：8 个只读（plan 模式）agent 并行完成，产出结构化发现（文件:行 + 证据 + 严重度 + 置信度）；协调员去重合并、独立复核后成文。本报告为一次性审查记录，与 `product-review-2026-10-12.md` 同类，非强制规范。
