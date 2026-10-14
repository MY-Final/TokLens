# 统一用量数据模型

本文描述 TokLens 的本地用量数据模型、统计口径与 SQLite 表结构。实现位于
`src-tauri/src/usage/`（本文件与代码必须保持一致，改口径时两边同改）：

| 模块       | 文件                                      | 职责                                                                                                                                      |
| ---------- | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 客户端标识 | `src-tauri/src/usage/client.rs`           | 客户端枚举与字符串映射                                                                                                                    |
| Token 模型 | `src-tauri/src/usage/tokens/counts.rs`    | 原始计数、规范分桶与归一化结果                                                                                                            |
| Token 口径 | `src-tauri/src/usage/tokens/semantics.rs` | 字段关系、版本化客户端口径与证据                                                                                                          |
| Token 换算 | `src-tauri/src/usage/tokens/normalize.rs` | 去重叠换算、规范总量选择与未知项标注                                                                                                      |
| 领域模型   | `src-tauri/src/usage/model.rs`            | 会话/消息结构、去重标识、口径枚举                                                                                                         |
| 归一化     | `src-tauri/src/usage/normalize/`          | Codex / Claude Code / OpenCode / DSH / Kimi Code / MiMoCode / Pi Desktop / WorkBuddy / Copilot / Gemini CLI / Grok CLI / MCode 的独立规则 |
| 存储       | `src-tauri/src/usage/store/`              | 表结构、版本化迁移、幂等导入、增量同步水位                                                                                                |
| 时间工具   | `src-tauri/src/usage/time.rs`             | ISO 8601 UTC 解析与 Unix 毫秒系统时间                                                                                                     |
| 统计查询   | `src-tauri/src/usage/analytics/`          | 查询类型、区间/趋势/客户端汇总、排行与摘要                                                                                                |
| 统计报告   | `src-tauri/src/usage/report.rs`           | 报告元数据白名单与 CSV / JSON / Markdown 序列化                                                                                           |

边界：本模块负责"数据模型 / 统计口径 / 存储 / 查询"，不包含客户端文件读取与 UI；
客户端采集器在 `src-tauri/src/collectors/`（见第 8 节）。采集数据来源与字段证据见 [`data-source-research.md`](./data-source-research.md)。

目录入口保持 `usage::tokens` / `normalize` / `store` / `analytics` 的公开调用路径，
内部职责与可见性约定见 `architecture.md`。`store/initial-schema.sql` 是 v1 原始迁移，
后续变更只能追加迁移，不编辑已发布 SQL。`model` 的时间函数重导出仅用于兼容，新增代码使用 `usage::time`。

## 1. Token 字段语义（禁止重复求和）

字段关系来自本机实测与客户端源码契约（证据见 `data-source-research.md`），代码中每条口径都带 `basis` 说明。

| 客户端      | 口径 id                          | `cache_read` 相对 `input` | `reasoning` 相对 `output` | `cache_write` 相对 `input`   | 客户端是否提供总量                                      |
| ----------- | -------------------------------- | ------------------------- | ------------------------- | ---------------------------- | ------------------------------------------------------- |
| Codex       | `codex.usage.v1`                 | 包含（子集）              | 包含（子集）              | 未知                         | 是（`total = input + output`）                          |
| Claude Code | `claude_code.anthropic_usage.v1` | 独立                      | 客户端不提供              | 独立                         | 否                                                      |
| OpenCode    | `opencode.message.v1`            | 独立                      | 独立                      | 未知                         | 是（`total = input + output + reasoning + cache.read`） |
| DSH         | `dsh.token_usage.v1`             | 独立                      | 包含（子集）              | 独立                         | 否（按 token-meter 四个独立桶求和）                     |
| Kimi Code   | `kimi_code.usage_record.v1`      | 独立                      | 客户端不提供              | 独立                         | 否（四桶求和）                                          |
| MiMoCode    | `mimocode.message.v1`            | 独立                      | 独立                      | 独立                         | 是（旧记录无 total 时由五桶求和）                       |
| Pi Desktop  | `pi_desktop.operation.v1`        | 独立                      | 包含（子集）              | 独立                         | 是（input + output + 两个缓存桶）                       |
| WorkBuddy   | `workbuddy.item_usage.v1`        | 包含（子集）              | 包含（子集）              | 未知                         | 是（total = input + output）                            |
| Copilot CLI | `copilot.assistant_usage.v1`     | 原生包含；归一化后独立    | 包含（子集）              | 原生包含；归一化后独立、计入 | 否（原生 input + output 直接换算）                      |
| Gemini CLI  | `gemini_cli.response.v1`         | 包含（安装包契约）        | 独立                      | 不提供                       | 是（保留客户端 total；tool 关系未知）                   |
| Grok CLI    | `grok.turn.v1`                   | 包含（正样本与展示契约）  | 包含（正样本与展示契约）  | 未知、不计入                 | 是（会话与轮级 totalTokens）                            |
| MCode       | `mcode.request.v1`               | 独立                      | 未知                      | 独立、计入                   | 否（reasoning 明确为 0 时四桶换算）                     |

由此得到的**互不重叠分桶**（任意两项相加都不会重复计入同一批 token）：

| 分桶                        | Codex                       | Claude Code                  | OpenCode                    | DSH                              | Kimi Code                        | MiMoCode              | Pi Desktop                       |
| --------------------------- | --------------------------- | ---------------------------- | --------------------------- | -------------------------------- | -------------------------------- | --------------------- | -------------------------------- |
| `input`（未命中缓存的输入） | `input − cache_read`        | `input`                      | `input`                     | `inputTokens`                    | `inputOther`                     | `input`               | `inputTokens`                    |
| `output`（不含推理）        | `output − reasoning`        | `output`                     | `output`                    | `outputTokens − reasoningTokens` | `output`                         | `output`              | `outputTokens − reasoningTokens` |
| `reasoning`                 | `reasoning`                 | 不提供（NULL）               | `reasoning`                 | `reasoningTokens`                | 不提供（NULL）                   | `reasoning`           | `reasoningTokens`                |
| `cache_read`                | `cache_read`                | `cache_read`                 | `cache.read`                | `cacheReadTokens`                | `inputCacheRead`                 | `cache.read`          | `cacheReadTokens`                |
| `cache_write`               | `cache_write`（不计入总量） | `cache_creation`（计入总量） | `cache.write`（不计入总量） | `cacheWriteTokens`（计入总量）   | `inputCacheCreation`（计入总量） | `cache.write`（计入） | `cacheWriteTokens`（计入）       |

规范总量 `total_tokens`：

新增客户端规范分桶：Gemini input=input−cached、output=output、reasoning=thoughts、cache_read=cached、cache_write=NULL；tool 没有已验证对应桶，不冒充缓存写入。Grok input=inputTokens−cachedReadTokens、output=outputTokens−reasoningTokens；两者缺少拆分字段或扣除矛盾时为 NULL；缓存写入只保留自报值且不计入总量。MCode input/两缓存保持独立；reasoning 明确为 0 时保留 output，否则 output=NULL，不能把未知输出关系画成可相加的桶。三者均保留缺失与实测零的区别。

1. 客户端提供总量时（Codex、OpenCode、MiMoCode、Pi Desktop、WorkBuddy、Gemini CLI、Grok CLI）直接采用客户端值（`total_basis = client_reported`），
   并与分桶求和做一致性校验，不一致时置 `client_total_mismatch = 1`，不静默修正。
2. 原始记录未提供总量时（例如 Claude Code / DSH / Kimi Code / MCode）由完整必需分桶求和得出（`total_basis = derived_from_buckets`）；MCode 仅在 reasoning 明确为 0 时可换算总量。
3. 必需分桶缺失（NULL）时总量为 `NULL`（`total_basis = unavailable`）并记录未知项，**不把缺失当 0**。
4. `cache_write` 是否计入总量由口径决定：Codex/OpenCode 的客户端总量本身不含它，故不计入且标注未知；
   Claude Code / DSH / Kimi Code / MiMoCode / Pi Desktop 的 cache_write 与 input 独立，故计入。Copilot CLI 原生缓存已含于输入，拆分后计入；DSH 缓存字段省略时按协议默认 0，详见第 10 节。

## 2. 未确认的语义（明确标注，不推断）

| 未知项标识                                                    | 含义                                                                                   | 影响                                                   |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `cache_write.relation_to_input`                               | Codex / OpenCode 的 `cache_write` 与 `input` 的关系无法判定（本机取值全为 0 或缺样本） | 不计入总量；记录在 `unknown_relations` 列，UI 需可标注 |
| `cache_read.missing`                                          | 客户端未提供 `cache_read`，无法把 input 拆成"未命中缓存"部分                           | `input` 桶保留原始值，总量按客户端值                   |
| `reasoning.missing`                                           | 客户端未提供 `reasoning`，无法把 output 拆成"非推理"部分                               | 同上                                                   |
| `total.unavailable_due_to_missing_fields`                     | 字段缺失导致总量未知                                                                   | `total_tokens` 为 NULL                                 |
| `anomaly.cache_read_gt_input` / `anomaly.reasoning_gt_output` | 与声明的包含关系矛盾的数据                                                             | 分桶取 0 并标注，不 panic、不回绕                      |

## 3. 去重标识

| 客户端      | 会话键                                              | 消息键（`dedup_key`）                                         | 说明                                                                                                  |
| ----------- | --------------------------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Codex       | `thread_id`（唯一键 `(client, client_session_id)`） | `response_id` → `token_count:<ordinal>` → 累计快照 → 文件行号 | 同一 thread 的多个 rollout 文件（续接/分页）只取累计值最大值，**绝不相加**；新旧格式混排规则见第 8 节 |
| Claude Code | `sessionId`                                         | `message.id`                                                  | 同一 `(sessionId, message.id)` 会被重复写入（本机最多 15 次，值一致），只保留一行                     |
| OpenCode    | `session.id` / `session_v2.id`                      | `message.id`                                                  | `session` 与 `session_v2` 按 `time_updated` 取新、平局取 `session_v2`，**绝不把两表相加**             |
| DSH         | header `id`                                         | `step:<turn>:<step>[:attempt:<n>]`                            | 同一尝试最终 usage 替换早期样本；重试分开统计，排除继承 seed                                          |
| Kimi Code   | state `id` / session_* 目录名                       | `agent:<folder>:line:<physical-line>`                         | 增量 usage 按 agent 与行号区分；快照替换幂等，正文相同不代表同一次调用                                |
| MiMoCode    | session.id                                          | message.id                                                    | 排除导入消息，保留本地续聊，按数据库主键去重，不读取 event / part 镜像                                |
| Pi Desktop  | header.sessionId                                    | operationId；旧版 message.id                                  | 全客户端物理操作去重，汇总 operations 只展开一次；更完整的总量优先                                    |
| WorkBuddy   | `sessionId`                                         | 原生输出事件 `id`                                             | usage 镜像不相加；跨文件按事件时间取新；不使用复用的 provider / 对话 ID                               |
| Copilot CLI | `sessions.id`                                       | `usage:<assistant_usage_events.id>`                           | 数据库主键标识单次用量，不以 turn_index 或 agent_id 合并请求                                          |
| Gemini CLI  | `sessionId`                                         | 最终 `gemini.id`                                              | 日志回放全记录替换、$set.messages 全量替换与 $rewindTo 后去重；会话镜像择一                           |
| Grok CLI    | `usage.sessionId`                                   | `turnNumber`                                                  | 轮级快照，不把 modelCalls 展开为虚构独立调用；session 与 turns 不相加                                 |
| MCode       | `local_runtime_sessions.session_id`                 | `local_runtime_token_usage.id`                                | 主键标识物理请求，同 turn 多次调用不合并；不读取正文 revision                                         |

会话总量的取值依据记录在 `sessions.total_basis`：

- `client_cumulative`：Codex，取该线程的累计值最大值（新旧两个累计计数器取最大，见第 8 节）。
- `client_reported`：OpenCode / Grok CLI，直接使用会话聚合列；Copilot Agent Host 在 CLI 求和基础上覆盖为 `client_reported`。
- `sum_of_messages`：Claude Code / DSH / Kimi Code / MiMoCode / Pi Desktop / WorkBuddy / Copilot CLI / Gemini CLI / MCode，由去重后的消息求和得出。

`sessions.message_coverage` 记录请求明细完整度：Codex 的
分页/续接文件（`ordinal` 不从 0 开始）标记为 `partial`，明细不完整但会话总量仍取累计值；Claude Code / DSH / Kimi Code / MiMoCode / Pi Desktop / WorkBuddy / Copilot CLI / Gemini CLI / MCode 按完整扫描汇总；OpenCode 由会话聚合与请求明细的已知总量校验、Grok CLI 由轮次计数对应关系校验，无法确认时标记 `partial`；Copilot Agent Host 明细始终为 `partial`，会话总量与请求求和可以不同。

## 4. SQLite 表结构（v1）

库位置：`<当前存储目录>/toklens-usage.sqlite3`，初始默认在 Windows 为 `%APPDATA%\com.toklens.app\`，可从设置迁移到用户指定目录。
系统应用目录保留小型 `storage-location.json`，统计数据库与 `settings.json` 迁移后在新目录使用。位置迁移不改变 schema、统计口径、去重键或采集水位。
与客户端自身的数据库分离：只读客户端库、只写 TokLens 自有文件，不向客户端来源目录写入。
`settings.json` 的 `panels` 数组只存用户自定义面板的定义与布局（纯数据），不含会话正文与任何用量原始数据；面板规格与统计库互不写入，清空统计不删除面板。
`settings.json` 的 `llm` 只存面板 AI 生成的非机密配置（`baseUrl` / `model`）；API Key 绝不落此文件，仅存 OS 凭据库（keyring，write-only）；未配置即完全离线，不出网。
迁移由 `schema_migrations` 记录，`PRAGMA user_version` 同步冗余。连接统一设置
`foreign_keys=ON`、`journal_mode=WAL`、`synchronous=NORMAL`、`busy_timeout=5000`。

### `source_files`（增量同步水位）

| 列                        | 说明                                                   |
| ------------------------- | ------------------------------------------------------ |
| `client` / `path`         | 唯一约束 `(client, path)`                              |
| `kind`                    | `jsonl`（文本 JSON 家族，含 state sidecar） / `sqlite` |
| `size_bytes` / `mtime_ms` | 上次扫描时的文件特征，用于判断是否变化                 |
| `content_offset`          | 已消费字节偏移；Codex 当前用于变化判定，不用于追加续读 |
| `parser_version`          | 解析器版本，变化时应重新解析                           |
| `unparsed_records`        | 未识别记录数，供 UI 提示（不静默丢弃）                 |

### `sessions`（统计总量的唯一事实来源）

关键列：`id`（`<client>:<会话 id>`）、`client`、`client_session_id`、`parent_client_session_id`、
`project_path`、`model`、`started_at_ms` / `ended_at_ms`、`semantics_id`、`total_basis`、
`message_coverage`、`source_kind`、五个分桶列、`total_tokens`、`client_total_tokens`、
`client_total_mismatch`、`unknown_relations`、`cost_source`、`client_cost_usd`、
`imported_at_ms` / `updated_at_ms`。唯一约束 `(client, client_session_id)`。

### `messages`（请求级明细，不参与总量求和）

关键列：`id`（`<client>:<会话 id>:<去重键>`）、`session_id`（外键，`ON DELETE CASCADE`）、
`dedup_key`、`seq`、`created_at_ms` / `completed_at_ms`、`model`、`agent`、分桶列、`total_tokens`、
`client_total_mismatch`、`unknown_relations`、`is_sidechain`、`source_file_id`、`source_ref`、
`cost_source`、`client_cost_usd`。唯一约束 `(client, session_id, dedup_key)`。

索引：`(client, started_at_ms)`、`(project_path, started_at_ms)`、`(session_id, created_at_ms)`、
`(client, created_at_ms)`、`model`。

## 5. 导入、幂等与增量

- **原子导入**：`UsageStore::import_session(file, session, messages)` 在单事务内写水位 + 会话 + 明细，
  中断不留半截数据。
- **绝对值快照**：所有写入是覆盖而不是累加，因此重复导入同一份数据不改变行数与统计结果。
- **陈旧保护**：累计值口径下新的累计值小于库中值（或时间更早）时，会话快照被忽略（`SkippedStale`），
  避免用旧文件/备份回退总量；明细是否重建由导入 API 决定，见第 8 节。
- **增量同步**：先按 `(client, path)` 读回水位；`size_bytes`/`mtime_ms` 未变化且 `parser_version` 相同时跳过，
  变化时整文件重解析并通过 `replace_file_import` 覆盖该文件的明细（见第 8 节）。
- **重启恢复**：水位与数据都在本地库中，进程重启后继续按水位判断是否重解析，不需要重建索引。

## 6. 费用与 Token 解耦

- 本轮**不做任何价格换算**：`cost_source` 只有 `client_reported`（OpenCode 自算）与 `none`。
- 未提供成本时 `client_cost_usd` 为 NULL，不用 token 反推、不写 0。
- 价格表与成本推导属于后续工作，届时需要按 `docs/data-metrics.md` 第 9 节登记口径与版本。

## 7. 测试

`src-tauri/src/usage/` 内的 `#[cfg(test)]` 单测使用**合成数据**（不含任何真实会话内容），覆盖：

- 去重叠：Codex 子集扣除、Claude/OpenCode 独立字段、`reasoning` 不重复计入；
- 重复记录：Claude 同 `message.id` 多次写入只落一行；重复导入不新增行；
- 累计值：Codex 累计值不回退、分页/续接文件不把多份累计值相加；
- 缺失字段：NULL 与 0 区分、总量未知而非 0；
- 存储：迁移幂等、重启后水位保留、级联删除、按客户端/时间范围统计；
- 双表兼容：`session` / `session_v2` 取新规则与会话总量等于去重后明细求和。

运行方式见 `AGENTS.md`：`pnpm rust:test`（或 `pnpm rust:check` 一并跑 fmt + clippy + test）。

## 8. Codex 采集器（已实现）

实现：`src-tauri/src/collectors/codex/`；扫描 `%USERPROFILE%\.codex\sessions` 下的 `rollout-*.jsonl`（只读）。

### 8.1 新旧格式混排规则（实测口径）

同一个文件可能同时包含新版 `token_usage_record` 与旧版 `event_msg.token_count`：

| 情况                                                | 处理                                                                                                                         |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 新格式窗口内的请求                                  | 旧格式会写一条数值相同的**镜像事件**：按顺序配对后丢弃，避免同一请求统计两次                                                 |
| 新格式开始之前的旧格式历史（81/252 个混合文件存在） | 保留为请求明细（去重键 `token_count:<ordinal>`）                                                                             |
| 线程累计值                                          | 取 `thread_token_usage.total_tokens` 与 `total_token_usage` 的**最大值**；两个计数器起点不同，既不能相加也不能固定偏向新格式 |
| 明细完整度                                          | 该线程只要有任一文件从 `ordinal 0` 开始即为 `complete`，否则 `partial`                                                       |

### 8.2 增量同步与恢复

两种用量记录均支持缺失 `ordinal`（2026-10-12，口径 2.5）。请求键优先非空 `response_id`、其次真实 `ordinal`，已有键不变。
无两者时，累计 `total_tokens` 已知则使用 `token_count:cumulative:<input>:<output>:<cache_read>:<cache_write>:<reasoning>:<total>`，未知字段编码为 `?`；
否则使用 `token_count:file:<绝对路径>:line:<从 1 开始的物理行号>`。相同累计通知只保留一次，同文件保留首次时间；跨文件由现有 store 去重。
新旧镜像配对前先排除无序号旧格式中的重复通知，配对规则本身不变。文件行号回退仅保证同文件幂等，不能识别跨文件复制。
累计快照回退无法识别计数器重置或相同快照的零用量请求，完整度仍按真实 ordinal 判断；无序号时 `seq` 为 NULL，来源引用记录累计字段或文件行号。
`info` / `last_token_usage` 为 null 的旧格式状态通知不生成请求。parser_version 为 2；升级同步自动重读旧文件，规则与影响见 `data-metrics.md` 2.5。

- 决策依据：`(size_bytes, mtime_ms, parser_version)`。未变化 → 跳过；变化 → 整文件重新解析。
- 写入是"覆盖式"的：先删除该源文件此前的明细，再写入本次结果，单事务提交（`replace_file_import`）。
  因此重复导入不会重复计数，文件被截断/替换后旧明细也不会残留。
- 中断安全：解析失败或进程被杀时水位不前进，下次运行重新解析该文件。
- 累计值不回退：新快照的累计值更小或时间更早时会话快照不被覆盖（`SkippedStale`）。
  `import_session` 同时跳过明细写入；`replace_file_import` 仍覆盖对应源文件的明细与水位，保证截断后明细可重建。
- 已知限制：大小与 mtime 都没变的"同尺寸内容替换"无法察觉（rollout 为追加写入，视为异常）；
  `SyncOptions::force_rescan` 提供显式全量重扫。

### 8.3 隐私

只读取白名单字段（见 `codex::FIELD_WHITELIST`）；正文、提示词、工具输出、凭据不取出、不落库、不写日志，
客户端文件全程只读。单测 `privacy_sentinel_content_never_reaches_the_database` 会校验数据库文本列中不含正文哨兵。

### 8.4 准确性验证

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example codex_usage_report
```

导出每个线程的 `total_tokens` 与完整度，可与 Codex 自身的 `%USERPROFILE%\.codex\state_5.sqlite`
（`threads.tokens_used`）逐线程对账。本机 2026-10-11 实测：309 个有数据的线程中 307 个完全一致，
差值合计 331,155（占总量 0.006%）；`sessions/` 合计 5,341,311,990，与 Codex 自身同一线程集合的
5,340,980,835 相差 0.006%。

## 9. 统计查询口径（analytics）

本节原有汇总表与每日趋势描述属于保留的“会话累计”模式；默认“请求时间”模式见文末“请求时间与会话详情查询”2.3 补充。

实现：`src-tauri/src/usage/analytics/`；命令层只做参数校验与 DTO 转换（`src-tauri/src/commands/`）。

| 规则       | 说明                                                                                                       |
| ---------- | ---------------------------------------------------------------------------------------------------------- |
| 事实来源   | 会话累计 token 汇总取自 `sessions` 表（该模式唯一事实来源）；`messages` 只用于计数与时间序列，二者绝不相加 |
| 区间过滤   | 按会话的 `started_at_ms` 过滤；边界由前端按本地日历传入（`fromMs`/`toMs`，`toMs` 为排他上界）              |
| 每日趋势   | 按会话开始时间归属到本地日期，缺失日期补 0；桶按 24 小时整除，含夏令时的时区会有偏差（待引入时间库修正）   |
| 模型排行   | 按会话主模型（`sessions.model`）聚合；Codex 的逐请求模型未落库，故不按请求维度排行                         |
| 项目排行   | 按 `sessions.project_path` 聚合，跨客户端合并                                                              |
| 未知值     | `total_tokens` 为 NULL 的会话不计入任何合计，单独计数为 `sessions_without_usage`（界面显示 “—”）           |
| 不完整数据 | `message_coverage = partial`（源文件被分页/续接）单独计数，界面加标注                                      |
| 口径冲突   | 客户端自报总量与其分项之和不一致时按客户端值计入，并计数为 `sessions_total_mismatch`                       |
| 费用       | 当前恒不展示（`costAvailable: false`），避免虚构金额                                                       |

分桶之和应等于规范总量；唯一例外是上一行的 `client_total_mismatch` 会话（总量取客户端值，分项按其原始字段计算）。

首版热力图复用每日趋势，不新增表或迁移。`DailyBucket` 增加 `sessions_without_usage`
（当天 `total_tokens IS NULL` 的会话数）与 `sessions_partial`（当天 `message_coverage = partial` 的会话数）。
跨端 `DailyTokenBucket` 使用 `sessionsWithoutUsage` / `sessionsPartial`，`clients` 每个日条目增加同样的两项与 `sessions`。
这些计数彼此独立：明细不完整的会话仍可能有已知累计 Token 总量。
缺失日期仍返回零会话、零计数与零求和；有会话但全部总量未知时，数值求和仍为 0，展示应为 `—`。
已知与未知混合时保留已知求和并标注已知子集及未知会话数，不把它当作完整总量。
请求数仍按所属会话开始日期计数，不按请求自身时间重分桶；本地零点起固定 24 小时桶的夏令时偏差保持原有限制。

## 10. 新采集器（2026-10-11）

### Claude Code

`collectors/claude_code/` 扫描 `CLAUDE_CONFIG_DIR/projects`（未配置时 `~/.claude/projects`），
只读所有 JSONL，包括 `subagents/agent-*.jsonl`。只保留会话标识、时间、cwd、模型、sidechain 标志和 usage。
忽略 `<synthetic>` 模型；用户记录只提供会话元数据，不计为请求。
按 `(sessionId, message.id)` 跨文件去重，时间较新的记录覆盖，平局按稳定文件 / 行顺序取最后一条。
消息定位保存真实源文件关联；子代理归属记录自身的 sessionId，不从路径推断另一会话。

任一文件追加、截断、替换、消失或解析版本变化时，重新聚合整个文件集合。
必需字段在任一请求中缺失，该会话对应字段与总量保持未知，已知请求数仍可展示。
完整扫描后以 `replace_client_snapshot` 原子替换该客户端快照与水位；因此删除或修正源记录会反映到统计中。
读取失败、目录枚举失败、末尾半行或扫描期间文件变化时不提交；完整但损坏的行计数并标记快照明细不完整。
根目录缺失不会清除历史。未变化的文件集合跳过；同大小同 mtime 替换需要强制重扫。

### OpenCode

`collectors/opencode/` 只读 `XDG_DATA_HOME/opencode/opencode.db`
（未配置时 `~/.local/share/opencode/opencode.db`）。使用 SQLite READ_ONLY 与一次只读事务，
只选择 session 元数据、token 聚合列和 message 的允许 JSON 路径，禁止读取 part / credential / account。
每次扫描元数据快照，避免主库大小或 mtime 未变化时漏掉 WAL 内容；不依赖不完整的 event 表作为水位。

`session` / `session_v2` 按 id 合并，更新时间较新的优先，平局取 `session_v2`；不把两份聚合相加。
消息只导入 assistant，按 message.id 去重，兼容扁平与嵌套模型字段；缺失聚合列保持未知。
聚合表无总量列时，由完整聚合分桶确定性换算总量；保留客户端自报成本，但不视作统一 USD 定价依据。
成功读取后原子替换完整客户端快照；读取 / schema 失败保留原快照。
当前支持 SQLite 格式；旧 `storage/session|message` JSON 格式未经验证，未接入。

### DSH / deepseek-hermes

`collectors/dsh/` 读取 `DSH_SESSIONS_DIR`（TokLens 的显式扫描覆盖）或 `~/.dsh/sessions`，
递归发现规范文件名 `session.jsonl[.zstd]`（v0）与 `session.vN.jsonl[.zstd]`（v1–v4），支持串联 Zstandard 帧。
版本化文件是正式日志，不是可忽略的备份。同一会话目录只选最高版本；同版本优先纯文本日志，不叠加压缩副本。
最高版本不支持或损坏时不回退旧代。不同目录的同一 header.id 按结束时间取新、平局按稳定顺序取最后一份，不相加。
不读临时 / 非规范备份文件、归档插件、usage 插件账单、凭据或 settings。
Serde 白名单仅包含 header、请求模型、usage、turn / step / seq / time 与继承 / 重试标记，不反序列化正文。
v0/v1 的 assistant/chunk 样本由最终 assistant/message usage 替换；v2–v4 优先最终 usage，否则取内嵌 stream 最后一个 usage chunk，兼容 assistant/attempt。
去重键首尝试为 `step:<turn>:<step>`；`llm/retry-started` 关闭当前替换槽，后续尝试追加 `:attempt:<n>`，避免漏算真实重试。
v0/v1 排除 `seq < header.seedLength`；v2–v4 根据 `isSeeded` 与最后一个 `session/end-seed.data.inherited = true` 的 seq 排除继承前缀。
继承标记缺失或与 header 矛盾时拒绝该文件，保留 parentSession 与 subagent 标记。

DSH 的 `inputTokens` 已由 DeepSeek 适配器扣除缓存读取；`outputTokens` 来自 completion_tokens，包含 reasoning。
口径 id 为 `dsh.token_usage.v1`：input / cache_read 独立，reasoning 从 output 拆分；cache_write 与 input 独立，计入总量。
已知总量为 `inputTokens + outputTokens + cacheReadTokens + cacheWriteTokens`，来源为 derived。
DSH token-meter / pi-ai 适配器明确把省略的可选缓存字段按 0 编码和统计，解析按该协议默认值处理（非估算）；显式 null / 非数值缓存字段拒绝并计数。必需 input / output 或整个 usage 缺失仍保持未知；推理缺失时保留原输出并标注无法拆分。
口径依据 DSH 的客户端无关 token-meter 投影契约，同时核对 DeepSeek 与 pi-ai 两个适配器；未验证的新持久化格式仍需独立接入。
解析器版本提升为 2，已有水位会触发重扫。完整成功扫描原子替换客户端快照；部分文件失败时事务替换成功会话，保留失败及未确认删除的历史会话与水位。
下一次仍重试失败文件，后续完整扫描再清理已删除来源；扫描期间文件变化仍拒绝整次发布。不修改源文件。

### 手动对账

运行 `cargo run --manifest-path src-tauri/Cargo.toml --example client_usage_report`，仅输出聚合元数据，使用内存库；追加参数 `-- dsh` 可只对账 DSH。
2026-10-11 本机对照独立只读解析：Claude Code 1721 个去重请求，已知总量 223724672；
OpenCode 50 个合并会话、5490 个请求，总量 1304431888；
DSH 旧解析器时点快照为 8 个会话、643 个请求，已知总量 182506906（只覆盖当时 v0 文件，不能用作 v1–v4 修复后的对账基线）。各旧版已知字段与独立解析一致。
Claude Code 47 个会话中 18 个只有用户元数据，另 1 个 assistant 会话缺少完整 usage，总量未知；
DSH 2 个会话因无请求或必需 usage 缺失，总量未知。它们不贡献总量，不做估算。
这些数字是本机时点快照，不作为自动化测试输入。

v0–v4 修复后的本机只读对账（2026-10-11）：13 份规范候选日志按目录选出 11 份最高代，导入 11 个会话、1357 个请求；
已知总量 362803932，3 个会话总量未知。独立只读解析得到相同会话数、请求数、已知总量与未知会话数。
当前正式 v4 的 system/message、model/selection、workspace/changes、delivery-accepted、compaction/prune 属非用量事件，明确忽略；未知事件仍计数。
该结果只验证本机数据，用户公司电脑尚未进行实际数量对账。

## 11. Kimi Code 采集器

`collectors/kimi_code/` 读取 `KIMI_CODE_HOME/sessions` 或 `~/.kimi-code/sessions`。
只接受 `sessions/<workspace>/session_*/state.json` 与 `agents/<agent>/wire.jsonl`；
不读取配置、凭据、插件状态、历史正文缓存、服务端事件镜像。
state 支持 version 2 和旧版无版本格式，createdAt / updatedAt 接受 Unix 毫秒或 ISO UTC。
项目取 cwd / 旧版 workDir；旧版无 id 时使用 session_* 目录名。wire 必须以协议 1.4 / 1.5 的 metadata 开头。
只归一化 `usage.record`，usageScope 为 turn / session（省略按客户端默认 session）；
`token_counting.measured` / `token_counting.turn_recorded` 是上下文长度，不是调用用量。
inputOther 已排除缓存；output 不拆分 reasoning；四个 usage 字段均需真实存在才提供规范总量。
agent 目录决定身份，main 之外标记 sidechain，保留每条请求对应的 wire 来源。

每次来源变化时重建完整客户端快照，state / wire 均登记水位；提交前再次验证目录列表与文件指纹。
追加、缩短、删除通过原子替换纠正总量；根目录缺失、缺少 state、未知版本、读取失败或截断尾行保留上次快照和水位。
完整非法行 / 未知事件计入 unparsed，并将会话覆盖标为 partial；无 usage 的会话保留未知，不展示为真实零用量。
目前不支持未验证的 state / wire 协议，客户端成本未提供，不推算费用。

## 12. MiMoCode / Pi Desktop 采集器

MiMoCode 的 `collectors/mimocode/` 默认只读 ~/.local/share/mimocode/mimocode.db（XDG_DATA_HOME 覆盖 data 根），
MIMOCODE_DB 为绝对路径时直接使用、相对路径时位于 mimocode data 根。SQLite query_only + READ_ONLY，
每次同步重读事务快照以覆盖仅 WAL 变化，不对客户端 checkpoint 或写入。
仅投影 session.id / parent_id / directory / time_created / time_updated，以及 message.id / session_id / agent_id 和 data 的 role、modelID、providerID、time、tokens。
claude_import / external_import 仅读 session_id / message_ids，排除导入消息副本，保留本地续聊请求；纯导入会话不占 MiMo 会话数；缺少导入表兼容旧版。
不读取 part、event、memory、snapshot、auth、installation_id、账户/凭据表。
按原始消息 ID 计数，包括 main 之外 agent（sidechain）；没有调用用量的会话保持未知。
getUsage 持久化前已扣缓存和 reasoning，五桶独立；旧消息无 total 时按完整桶求和，缺失必需字段不估算。
其 cost 由价格换算，不保存为客户端实测费用；本次不改变成本模型。

Pi Desktop 的 `collectors/pi_desktop/` 默认只读 ~/.pi-desktop/sessions/**/*.jsonl；PI_DESKTOP_SESSIONS_DIR 为 TokLens 扫描覆盖。
支持 schema 1 session header 和 message.meta.usage，只提取时间、身份、模型与 usage 白名单，跳过 blocks、工具正文、modelSystem.messageJson。
不扫描 pi.sqlite、secret、logs、cache。会话来源没有 cwd 时项目保持未知，不从正文或路径猜测。
inputTokens 与 cacheReadTokens / cacheWriteTokens 独立；reasoningTokens 属 outputTokens 子集，缺失时不猜推理。
缓存字段在该协议中为可选：省略按 native ledger 默认 0；显式 null、负数、溢出视为非法记录。
operationId 是全客户端物理请求键；重复及父汇总 operations 中的操作取完整总量最大者，不再次叠加外层汇总。
aggregate 的未归属 legacy 残差按父 message id 保留，差值必须非负；无 operationId 的旧消息按 sessionId + message.id 去重。
不同会话重复 operationId 按总量最大者归属（平局按排序首次发现的来源），不复制到多个会话；跨文件重复 sessionId 拒绝以免误归属。
当前接受 usageOrigin pi / legacy / 缺省；未知来源计入 unparsed，不当作实测调用。
costStatus estimated / unknown 的 cost 不导入，当前所有 Pi cost 均不持久化，不估价。

会话总量对每个请求选择的已知总量求和（允许客户端 total 与由完整分桶换算的旧记录混合），不因混合格式覆盖客户端自报总量；任一请求总量未知时会话总量未知。
两者使用完整快照原子替换；源缺失、失败、未知格式保留历史与水位，非法完整行计数且覆盖标记 partial。
Pi 提交前复核文件集合/指纹，截断尾行保留上次快照；重复、追加、缩短、删除、重启与隐私用合成 fixture 验证。

### 每日查询的精确范围（2026-10-11）

`daily_token_trend_in_range(day_start_ms, day_count, from_ms, to_ms, client)` 独立接收显示桶起点与可选会话时间边界。
会话 Token / 完整度查询与请求计数查询均按 `sessions.started_at_ms >= from_ms AND sessions.started_at_ms < to_ms` 过滤，并同时限制在显示桶范围内。
请求自身时间不参与区间过滤或分桶。缺失日期、未知总量及 partial 完整度字段保持原语义。
`daily_token_trend` 保留为不额外筛选精确范围的兼容包装；固定 24 小时桶与夏令时限制保持不变。

### 请求时间与会话详情查询（口径 2.3，2026-10-11）

`usage/analytics/requests.rs` 提供 `request_activity(filter, day_boundaries)`，
`session_detail.rs` 提供 `session_detail(filter, client_session_id, limit, offset)`。
两者只查询现有 `messages` / `sessions` 表，无迁移；command 参数和 DTO 见 `architecture.md`。

请求范围以 `messages.created_at_ms >= from_ms AND messages.created_at_ms < to_ms` 过滤，
同时支持客户端 / 项目条件，仅保留 `source_kind IN ('measured', 'derived')` 且创建时间非 NULL 的记录。
`RequestTotals` 返回区间去重会话数、请求数、总量已知 / 未知请求数、measured / derived 请求数与规范 Token 求和。
总量直接求和已记录 `messages.total_tokens`；各分项独立对已知值求和，不重算总量，不从 `sessions` 补请求缺口。
SQL 求和空集合返回 0；UI 必须结合请求数与未知总量计数区分无活动、真实零、全部未知与已知子集。
`countedCacheWriteTokens` 仍按已有客户端语义计入，原始缓存写入不因此被重复叠加。

`RequestDailyBucket` 使用调用方传入的本地日历零点边界（最多 401 个递增边界 / 400 日），
每个半开日桶还受完整查询范围约束，可支持夏令时 23 / 25 小时日。
`range` 不受显示桶截断；`daily` 补全空桶并提供逐客户端值。
每日独立 `COUNT(DISTINCT session_id)` 不具可加性，不能由每日会话数推算区间去重会话数。
`RequestCoverage` 忽略日期，只应用客户端 / 项目条件，返回全部已导入记录计数；
`recordedRequests` 包含估算，`timedRequests` / `untimedRequests` 仅计非估算记录，估算另列。
`sessionsPartial` 与 `sessionsWithoutRequests` 是独立会话计数，不是完整率或未知 Token 比例。

会话详情必须指定客户端与客户端会话 ID，不应用列表的日期 / 项目条件。
`SessionDetail` 包含 `SessionSummary`、当前页 `RequestDetail[]` 和全会话 `RequestCoverageDetail`。
默认 100 条、上限 200 条，按 `created_at_ms IS NULL, created_at_ms, seq IS NULL, seq, id` 稳定排序，NULL 时间最后。
请求字段限定为去重键、序号、创建 / 完成时间、模型、agent、归一化 Token、`source_kind`、`source_ref`、
sidechain 和 unknown relations；不包含正文，详情查询不回读源文件。未知 Token 保持 NULL，覆盖计数不随分页改变。

### 请求用量分析查询（口径 2.4，2026-10-12）

`usage/analytics/requests.rs` 提供 `request_analysis(filter)`：复用已有请求时间筛选，完整区间客户端比较与选中客户端的分项、模型 / 项目归因共用读取事务快照。
均量仅用已知总量请求作为分母；缓存命中率只统计 input / cache_read 成对已知的请求，分母为零保持 NULL。
提供 `knownTokenFields` 区分没有已知字段与实测零；分项不补总量。客户端、全部模型分组与项目 Top 8 占比以完整区间已知总量作为分母。
模型使用会话主模型，项目使用会话目录，不回填逐请求模型。无表结构、迁移、去重或原始计数变化；公式与边界见 `data-metrics.md` 2.4。

### 查询与采集契约补充（口径 2.6，2026-10-12，纳入 v0.1.4）

SQLite schema 不变。RangeTotals / StatsBucket 增加成对已知缓存样本会话数与可空缓存命中率基点；ClientUsage / GroupUsage 与对应 DTO 增加 sessionsWithoutUsage，不能仅凭求和结果 0 判断真实零。排行 messages 为组内所有会话明细合计。
Codex 失败读取不提交 SourceFile 水位；合法无换行末行允许导入，未完成 JSON 尾行下次重试。CODEX_HOME 或用户 home 只在采集发现层解析，来源 DTO 的 sourcePath 是本地展示元数据，不默认对外复制或上传。

## 13. WorkBuddy（口径 2.7，2026-10-12，纳入 v0.1.5）

实现：collectors/workbuddy/{parser,rollup,sync}.rs，复用 usage/normalize/request_ledger.rs。只读 WORKBUDDY_CONFIG_DIR/projects 或 ~/.workbuddy/projects；不使用 CodeBuddy 的独立目录。现有 SQLite 表结构不变，client 新增 workbuddy。

口径 id 为 workbuddy.item_usage.v1：inputTokens 含缓存读取，outputTokens 含推理；互不重叠桶为 input − cached_tokens、output − reasoning_tokens、cached_tokens、reasoning_tokens。缓存写入关系未知且不计总量；总量沿用客户端 totalTokens。蛇形镜像字段来自 message.usage，只补缺，不与 providerData.usage 相加。详情数组 / 对象均支持，缺字段保持未知；超大值与 aggregate requests != 1 不当作单次请求导入。

会话键 sessionId，请求键原生输出事件 id。一次模型调用的 usage 仅挂在最后一个输出事件；无 usage 的片段不生成明细。多个调用可复用 providerData.messageId / conversationRequestId / traceId；跨文件按 (sessionId, id) 和事件时间选新，不按内容去重。时间使用日志的 Unix 毫秒，completed_at 保持 NULL，不将落盘时间伪装为完成时间；模型和 agent 取 providerData 元数据。没有旁链标识时不从代理名称推断旁链。

会话总量 sum_of_messages，完整成功扫描为 complete；有损坏行或未知记录时保守标记 partial，没有用量为 none。请求缺失总量时会话总量保持未知。同步仅在文件集合及指纹稳定后替换完整客户端快照；失败保留历史、水位，下次重试。记录只落统计元数据，费用保持 none。

## 14. Copilot CLI（口径 2.8，2026-10-12；Agent Host 扩展见 2.9）

实现：collectors/copilot/{queries,sync,tests}.rs 与 usage/normalize/copilot.rs。只读 COPILOT_HOME/session-store.db 或 ~/.copilot/session-store.db；client=copilot，无 schema 迁移。
会话键 sessions.id；消息键 usage:<assistant_usage_events.id>，每行一个用量请求，agent_id / parent_tool_call_id 仅用于子代理标记；不查询 turns 正文、summary、token_details_json、账号或日志。
口径 copilot.assistant_usage.v1：客户端输入包含缓存读写，领域层先扣除两者，规范桶的缓存均独立；reasoning 属 output 子集。
总量按原生 input + output 换算，领域标记 derived_from_buckets，请求保存 derived 来源等级；会话按完整快照中去重请求求和。缓存写入参与 countedCacheWriteTokens。
缺失输入 / 输出使总量未知；缺失缓存使输入分桶未知，保留输入输出直接换算的已知总量。缓存和大于输入时不回绕、输入分桶保持未知并标记 anomaly.copilot_caches_gt_input。
SQL 每次在只读事务中读取 WAL，完整快照原子替换，失败保留旧水位。无用量表的旧库只导入未知会话并标记 partial。
时间读取 created_at / updated_at；ISO UTC 和 SQLite UTC datetime 默认形式可解析，非法 / 缺失保持未知；无时间请求只在全会话详情中显示，不按会话时间补造请求时间。
没有客户端原生总量、金额和费用换算；premium requests、AIU 和上下文占用不冒充 Token。真实目录对账限制见 data-metrics.md 2.8。

### Copilot Agent Host（口径 2.9，2026-10-12）

client 仍为 copilot，展示名 GitHub Copilot，parser_version=2；无 schema 迁移。新增 collectors/copilot/{events,agent_host,agent_host_tests}.rs，复用共享 collectors/sqlite_snapshot.rs（copilot/mod.rs 中以 `database` 为别名），职责见 architecture.md。
默认 Windows Code 数据目录来自 APPDATA，测试显式注入路径；不在测试中发现真实客户端目录。Code Insiders、便携版与普通 workspace chatSessions 暂不发现。
events.jsonl 通过白名单读取 session.start.data.sessionId / startTime / context.cwd、session.model_change.data.newModel、user.message.id、assistant.message.data.apiCallId / model、外层 ISO timestamp；无 apiCallId 不虚构模型调用。
Agent Host 的 session_metadata.defaultChatProviderData（兼容镜像 agentHost.chatProviderData）提供 sdkSessionId；turns.event_id 关联 SDK user.message.id。外层目录 UUID、turns.id 和 SDK turnId 均不能当作跨源关联键。
会话以 SDK ID 归一，调用键 api:<apiCallId>；同 SDK 会话 CLI 有用量时不叠加 SDK 空用量消息，关联到 Host 时整轮总量优先。Host 镜像以 (sdkSessionId, turns.event_id) 去重，primary defaultChatProviderData 优先，等优先级不一致则放弃本次快照。
turn_usage.usage._meta.turnTokenTotals 的模型桶只求和一次，用于 SessionRecord（total_basis=client_reported 指客户端提供轮级聚合依据，total_tokens 本身仍 derived_from_buckets）。不与 directTurnTokenTotals、顶层计数或 vscode.modelCall 相加。
顶层 inputTokens/outputTokens/cacheReadTokens 仅属于该轮最后一次调用，按 _meta.vscode.modelCall.apiCallId 关联已有 SDK 调用。其他调用保留身份、模型、时间，Token 为 NULL。明细始终 partial；会话总量与请求求和可以不同。
口径 copilot.agent_host.turn.v1：仅已验证零缓存记录可由 input + output 得到总量，推理与缓存写入仍 NULL，不补零；非零或未知缓存不猜包含关系，输入分桶与总量保持未知并标记 cache_relation_unverified。缺任意已发现用户轮的聚合时会话总量 NULL，已知分项仍可显示。
SQLite 原文件 / WAL 仅文件读取，在内存校验 WAL checksum、salt、commit 后合成只读快照，忽略未提交尾帧；不打开 shm、不生成客户端侧文件，不持久化数据库镜像。
所有来源文件集合 / 主库及 WAL 指纹在扫描前后复核；失败、未知 schema、残缺 JSONL 尾行或关联缺失时保留旧数据和水位。缺 CLI 库但存在 SDK + Host 时可导入，使用 merge 保留未确认的旧来源；完整发现时原子替换。暂不采集只存在 Host 而 SDK 事件已丢失的记录，避免伪造请求或时间。

## 15. Gemini CLI（口径 2.10，2026-10-13）

`collectors/gemini_cli/{discovery,parser,sync,tests}.rs` 与 `usage/normalize/gemini_cli.rs`。默认 ~/.gemini；安装 Storage 契约将 GEMINI_CLI_HOME 当用户 home 后追加 .gemini，不把不存在的 GEMINI_DIR 环境契约当真。sandbox-exec 的 .cache 根与旧 .json 格式暂未支持。
只读 tmp/<项目>/chats 下 JSONL（含子代理），必要项目目录来自 .project_root 或会话 directories；排除 antigravity。流式只反序列化统计白名单，跳过 content/thoughts 正文。
追加记录是 journal，先完整回放同 ID 替换、$set.messages 全量替换、$rewindTo 删除目标及后续；未找到 rewind 目标时按已安装客户端契约清空。之后只统计最终 gemini 记录，会话 SumOfMessages，total_basis 为 derived_from_buckets；不叠加同会话镜像。
prompt 包含 cached，普通输入做减法；candidates 与 thoughts 独立，cache_write 未提供。tool 非零/缺失标 gemini_cli.tool_relation_unverified，有客户端 total 仍可保留；缺 total 且 tool 不明确为0不推算总量。缺缓存或关系矛盾使普通input NULL。
水位记录文件与项目marker指纹；失败文件或同更新时间镜像冲突全源不提交并报告files_failed；读取缺失/源变化保留历史。无新表、迁移、费用或估算。

## 16. Grok CLI（口径 2.11，2026-10-13）

`collectors/grok/{discovery,parser,sync,tests}.rs` 与 `usage/normalize/grok.rs`。GROK_HOME 配置根下 sessions/<编码工作区>/<会话>/usage.json 为权威，summary.json 只取身份、cwd、时间；不读对话、密文、events、updates、prompt_context。
session.totalTokens 是会话client_reported；turnNumber 是轮级去重键。整轮总量不平均分配给modelCalls，也不重复加到会话。重复相同轮仅保留一次，重复冲突拒绝；同会话镜像按更新时间择新、相同时间冲突保留历史。
原生 inputTokens 包含缓存，outputTokens 包含reasoning；缺失/不符合子集时相应规范桶为NULL，自报总量仍measured。cacheCreationTokens关系unknown，不计入规范总量。turns六计数合计、turnCount与modelCalls均对应才确认轮明细完整；否则partial，不表示已取得逐调用明细。
轮时间仅有endedAt：记账事件在轮结束时发生，created_at_ms与completed_at_ms均保存此事件时间，source_ref标timeBasis=turnEndedAt；不称它为单次调用开始时间。请求时间图按轮结束日归属，界面直接说明。时间解析支持实测UTC纳秒Z/+00:00，截断到毫秒；不猜其它时区。缺endedAt轮保持无时间，仅在全会话详情可见。
指纹覆盖usage及summary，稳定快照原子替换。失败文件数可见并不推进水位。costUsdTicks丢弃，无费用、估算、迁移或新依赖。

## 17. MCode（口径 2.12，2026-10-13）

`collectors/mcode/{discovery,parser,sync,tests}.rs` 与 `usage/normalize/mcode.rs`。MINIMAX_DATA_DIR > MAVIS_DATA_DIR > ~/.minimax，数据库为v2/sqlite/runtime-state.sqlite；配置覆盖来自已安装data-dir契约。
只投影local_runtime_sessions身份、parent、workspace、毫秒时间和local_runtime_token_usage身份、agent、model、ts、五桶；不查询raw/cost/record_json，不扫描manifest/messages/history-catalog/llm-call或正文多代revision。
请求按用量id、会话按session_id；同turn不同id保持独立。cache_read/write独立计入规范总量；reasoning=0且四必需桶完整才derived求和，非零/缺失reasoning标mcode.reasoning_relation_unverified、output与total NULL。会话按完整请求求和，坏行使完整总量未知，NULL模型不回填。
复用collectors/sqlite_snapshot.rs校验主库/WAL已提交帧并在内存只读反序列化；不打开客户端SHM，不checkpoint，不落盘镜像。前后指纹一致才replace_client_snapshot；坏SQLite/schema报告files_failed，失败与源变化保留旧历史水位。无用量表的旧库保留unknown会话，partial，不造零。
计入缓存写入的查询同时支持MCode会话和请求；客户端fresh/goal summary排除缓存，不能用作TokLens完整总量对账依据。无schema变化、依赖、权限、费用或网络新增。

### 查询筛选与小时展示补充（2026-10-13）

会话摘要查询增加 `SessionListOptions`，默认仍按开始时间；请求模式使用现有 `messages` 的非估算、有时间记录做 EXISTS 筛选，返回的 Token 保持整会话累计。元数据搜索、精确模型 / 项目筛选、NULL 组与排序不改写源字段或存储数据。请求时间统计新增可选小时边界与小时桶，沿用日桶的来源过滤、半开范围和未知值语义；无 schema、迁移、Token 公式或去重变更。来源诊断从现有表查询未知总量会话数及最近非估算记录时间，扫描结果只在应用内存保存。
