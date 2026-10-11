# 统一用量数据模型

本文描述 TokLens 的本地用量数据模型、统计口径与 SQLite 表结构。实现位于
`src-tauri/src/usage/`（本文件与代码必须保持一致，改口径时两边同改）：

| 模块       | 文件                                      | 职责                                       |
| ---------- | ----------------------------------------- | ------------------------------------------ |
| 客户端标识 | `src-tauri/src/usage/client.rs`           | 客户端枚举与字符串映射                     |
| Token 模型 | `src-tauri/src/usage/tokens/counts.rs`    | 原始计数、规范分桶与归一化结果             |
| Token 口径 | `src-tauri/src/usage/tokens/semantics.rs` | 字段关系、版本化客户端口径与证据           |
| Token 换算 | `src-tauri/src/usage/tokens/normalize.rs` | 去重叠换算、规范总量选择与未知项标注       |
| 领域模型   | `src-tauri/src/usage/model.rs`            | 会话/消息结构、去重标识、口径枚举          |
| 归一化     | `src-tauri/src/usage/normalize/`          | Codex / Claude Code / OpenCode 的独立规则  |
| 存储       | `src-tauri/src/usage/store/`              | 表结构、版本化迁移、幂等导入、增量同步水位 |
| 时间工具   | `src-tauri/src/usage/time.rs`             | ISO 8601 UTC 解析与 Unix 毫秒系统时间      |
| 统计查询   | `src-tauri/src/usage/analytics/`          | 查询类型、区间/趋势/客户端汇总、排行与摘要 |

边界：本模块负责"数据模型 / 统计口径 / 存储 / 查询"，不包含客户端文件读取与 UI；
客户端采集器在 `src-tauri/src/collectors/`（见第 8 节）。采集数据来源与字段证据见 [`data-source-research.md`](./data-source-research.md)。

目录入口保持 `usage::tokens` / `normalize` / `store` / `analytics` 的公开调用路径，
内部职责与可见性约定见 `architecture.md`。`store/initial-schema.sql` 是 v1 原始迁移，
后续变更只能追加迁移，不编辑已发布 SQL。`model` 的时间函数重导出仅用于兼容，新增代码使用 `usage::time`。

## 1. Token 字段语义（禁止重复求和）

字段关系全部来自本机实测（证据见 `data-source-research.md`），代码中每条口径都带 `basis` 说明。

| 客户端      | 口径 id                          | `cache_read` 相对 `input` | `reasoning` 相对 `output` | `cache_write` 相对 `input` | 客户端是否提供总量                                      |
| ----------- | -------------------------------- | ------------------------- | ------------------------- | -------------------------- | ------------------------------------------------------- |
| Codex       | `codex.usage.v1`                 | 包含（子集）              | 包含（子集）              | 未知                       | 是（`total = input + output`）                          |
| Claude Code | `claude_code.anthropic_usage.v1` | 独立                      | 客户端不提供              | 独立                       | 否                                                      |
| OpenCode    | `opencode.message.v1`            | 独立                      | 独立                      | 未知                       | 是（`total = input + output + reasoning + cache.read`） |

由此得到的**互不重叠分桶**（任意两项相加都不会重复计入同一批 token）：

| 分桶                        | Codex                       | Claude Code                  | OpenCode                    |
| --------------------------- | --------------------------- | ---------------------------- | --------------------------- |
| `input`（未命中缓存的输入） | `input − cache_read`        | `input`                      | `input`                     |
| `output`（不含推理）        | `output − reasoning`        | `output`                     | `output`                    |
| `reasoning`                 | `reasoning`                 | 不提供（NULL）               | `reasoning`                 |
| `cache_read`                | `cache_read`                | `cache_read`                 | `cache.read`                |
| `cache_write`               | `cache_write`（不计入总量） | `cache_creation`（计入总量） | `cache.write`（不计入总量） |

规范总量 `total_tokens`：

1. 客户端提供总量时（Codex、OpenCode）直接采用客户端值（`total_basis = client_reported`），
   并与分桶求和做一致性校验，不一致时置 `client_total_mismatch = 1`，不静默修正。
2. 客户端不提供总量时（Claude Code）由分桶求和得出（`total_basis = derived_from_buckets`）。
3. 必需分桶缺失（NULL）时总量为 `NULL`（`total_basis = unavailable`）并记录未知项，**不把缺失当 0**。
4. `cache_write` 是否计入总量由口径决定：Codex/OpenCode 的客户端总量本身不含它，故不计入且标注未知；
   Claude Code 未提供总量、且已证伪 cache 字段与 input 的包含关系，故计入。

## 2. 未确认的语义（明确标注，不推断）

| 未知项标识                                                    | 含义                                                                                   | 影响                                                   |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `cache_write.relation_to_input`                               | Codex / OpenCode 的 `cache_write` 与 `input` 的关系无法判定（本机取值全为 0 或缺样本） | 不计入总量；记录在 `unknown_relations` 列，UI 需可标注 |
| `cache_read.missing`                                          | 客户端未提供 `cache_read`，无法把 input 拆成"未命中缓存"部分                           | `input` 桶保留原始值，总量按客户端值                   |
| `reasoning.missing`                                           | 客户端未提供 `reasoning`，无法把 output 拆成"非推理"部分                               | 同上                                                   |
| `total.unavailable_due_to_missing_fields`                     | 字段缺失导致总量未知                                                                   | `total_tokens` 为 NULL                                 |
| `anomaly.cache_read_gt_input` / `anomaly.reasoning_gt_output` | 与声明的包含关系矛盾的数据                                                             | 分桶取 0 并标注，不 panic、不回绕                      |

## 3. 去重标识

| 客户端      | 会话键                                              | 消息键（`dedup_key`）                                   | 说明                                                                                                  |
| ----------- | --------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Codex       | `thread_id`（唯一键 `(client, client_session_id)`） | `response_id`；旧格式无该字段时 `token_count:<ordinal>` | 同一 thread 的多个 rollout 文件（续接/分页）只取累计值最大值，**绝不相加**；新旧格式混排规则见第 8 节 |
| Claude Code | `sessionId`                                         | `message.id`                                            | 同一 `(sessionId, message.id)` 会被重复写入（本机最多 15 次，值一致），只保留一行                     |
| OpenCode    | `session.id` / `session_v2.id`                      | `message.id`                                            | `session` 与 `session_v2` 按 `time_updated` 取新、平局取 `session_v2`，**绝不把两表相加**             |

会话总量的取值依据记录在 `sessions.total_basis`：

- `client_cumulative`：Codex，取该线程的累计值最大值（新旧两个累计计数器取最大，见第 8 节）。
- `client_reported`：OpenCode，直接使用会话聚合列。
- `sum_of_messages`：Claude Code，由去重后的消息求和得出。

`sessions.message_coverage` 记录请求明细完整度：Codex 的
分页/续接文件（`ordinal` 不从 0 开始）标记为 `partial`，明细不完整但会话总量仍取累计值；其他客户端默认 `complete`，由采集器按实际导入结果覆盖。

## 4. SQLite 表结构（v1）

库位置：`<应用数据目录>/toklens-usage.sqlite3`（Windows：`%APPDATA%\com.toklens.app\`）。与客户端自身的数据库分离：
只读客户端库、只写 TokLens 库，且不写项目目录。
迁移由 `schema_migrations` 记录，`PRAGMA user_version` 同步冗余。连接统一设置
`foreign_keys=ON`、`journal_mode=WAL`、`synchronous=NORMAL`、`busy_timeout=5000`。

### `source_files`（增量同步水位）

| 列                        | 说明                                                   |
| ------------------------- | ------------------------------------------------------ |
| `client` / `path`         | 唯一约束 `(client, path)`                              |
| `kind`                    | `jsonl` / `sqlite`                                     |
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

实现：`src-tauri/src/usage/analytics/`；命令层只做参数校验与 DTO 转换（`src-tauri/src/commands/`）。

| 规则       | 说明                                                                                                     |
| ---------- | -------------------------------------------------------------------------------------------------------- |
| 事实来源   | 所有 token 汇总取自 `sessions` 表（唯一事实来源）；`messages` 只用于计数与时间序列，二者绝不相加         |
| 区间过滤   | 按会话的 `started_at_ms` 过滤；边界由前端按本地日历传入（`fromMs`/`toMs`，`toMs` 为排他上界）            |
| 每日趋势   | 按会话开始时间归属到本地日期，缺失日期补 0；桶按 24 小时整除，含夏令时的时区会有偏差（待引入时间库修正） |
| 模型排行   | 按会话主模型（`sessions.model`）聚合；Codex 的逐请求模型未落库，故不按请求维度排行                       |
| 项目排行   | 按 `sessions.project_path` 聚合，跨客户端合并                                                            |
| 未知值     | `total_tokens` 为 NULL 的会话不计入任何合计，单独计数为 `sessions_without_usage`（界面显示 “—”）         |
| 不完整数据 | `message_coverage = partial`（源文件被分页/续接）单独计数，界面加标注                                    |
| 口径冲突   | 客户端自报总量与其分项之和不一致时按客户端值计入，并计数为 `sessions_total_mismatch`                     |
| 费用       | 当前恒不展示（`costAvailable: false`），避免虚构金额                                                     |

分桶之和应等于规范总量；唯一例外是上一行的 `client_total_mismatch` 会话（总量取客户端值，分项按其原始字段计算）。
