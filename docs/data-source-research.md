# 数据来源调研：Codex / Claude Code / OpenCode

本文记录 TokLens 第一项调研工作的**实际结果**：三个客户端在本机（Windows）落地的本地用量数据来源、文件格式、
可获取字段、口径与限制。所有结论均来自本机真实数据的只读检查，未联网核实官方文档。

## 0. 调研方法与合规说明

- 环境：Windows，`%USERPROFILE%`；客户端版本：Codex CLI `0.162.0-alpha.17.2`、Claude Code `2.1.294`、OpenCode `1.18.35`。
- 读取方式：全部**只读**。JSONL 用流式逐行解析；SQLite 使用 `file:<path>?mode=ro` 只读打开（不写、不建表、不 checkpoint WAL）。
- 数据处理：只提取**元数据与数值字段**（时间戳、标识、模型名、token 计数、成本）。正文、提示词、工具输出、
  会话标题、账号凭据一律不读取、不保存、不输出。路径在本文中脱敏为 `%USERPROFILE%`。
- 本文出现的会话标识一律截断为前 8 位。
- 原始数据未被修改（未写入、未移动、未删除任何客户端文件）。

证据统计（本次实测）：

| 数据源                                            | 文件数                       | 体量     | 覆盖时间                 |
| ------------------------------------------------- | ---------------------------- | -------- | ------------------------ |
| `%USERPROFILE%\.codex\sessions`                   | 353 个 rollout JSONL         | 988.5 MB | 2026-05-24 .. 2026-10-11 |
| `%USERPROFILE%\.codex\archived_sessions`          | 24 个 rollout JSONL          | 8.5 MB   | 同上区间                 |
| `%USERPROFILE%\.codex\state_5.sqlite`             | 1 个 SQLite                  | 2.8 MB   | 356 个线程               |
| `%USERPROFILE%\.claude\projects`                  | 103 个文件（72 个 `.jsonl`） | 30.9 MB  | 2026-06-05 .. 2026-10-08 |
| `%USERPROFILE%\.local\share\opencode\opencode.db` | 1 个 SQLite（WAL）           | 1 186 MB | 2026-08-10 .. 2026-10-09 |

## 1. 结论摘要

| 客户端      | 首选数据源                                             | 格式                             | 请求级 token 明细                                                    | 会话级聚合                                                | 项目维度                                                          | 成本                                  |
| ----------- | ------------------------------------------------------ | -------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------- |
| Codex       | `state_5.sqlite` → `threads`；明细回落到 rollout JSONL | SQLite + JSONL（append-only）    | 有（`token_usage_record`，0.153.0+；更早为 `event_msg.token_count`） | 有（`tokens_used`，且 JSONL 有累计 `thread_token_usage`） | 有（`cwd`、`git_branch`、`git_origin_url`）                       | 无，需自建价格表                      |
| Claude Code | `projects/**/*.jsonl`                                  | JSONL（append-only，含重复写入） | 有（`message.usage.*`）                                              | 无聚合表（需自行聚合；`.claude.json` 仅"最后一次会话"）   | 有（每行 `cwd`、`gitBranch`；目录名是 cwd 变形）                  | 无（仅 `.claude.json` 的 `lastCost`） |
| OpenCode    | `opencode.db`                                          | SQLite（WAL）                    | 有（`message.data.tokens.*`）                                        | 有（`session` / `session_v2` 的 token 列与 `cost`）       | 有（`project.worktree`、`session.directory`、`message.path.cwd`） | 有（`cost`，客户端自算）              |

三个客户端都记录了**输入、输出、缓存读取**；**缓存写入**只有 Codex 与 Claude Code 有字段（OpenCode 本机数据全为 0）。
`请求级明细`都能拿到，但**去重口径差异很大**（见第 5 节），这是后续实现中最关键的一处风险。

## 2. Codex

### 2.1 数据路径

| 路径                                                                                    | 作用                | 说明                                                                                                                             |
| --------------------------------------------------------------------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `%USERPROFILE%\.codex\sessions\<年>\<月>\<日>\rollout-<ISO时间>-<thread_id>.jsonl`      | 主会话流水          | 353 个文件，988.5 MB，单文件最大 67.7 MB，最小 23.6 KB                                                                           |
| `%USERPROFILE%\.codex\sessions\...\rollout-<时间>-<thread_id>_<parent_thread_id>.jsonl` | 续接会话流水        | 文件名含两个 id；`session_meta.history_base.thread_id` 指向父线程                                                                |
| `%USERPROFILE%\.codex\archived_sessions\**\rollout-*.jsonl`                             | 归档会话            | 24 个文件，格式与 sessions 相同                                                                                                  |
| `%USERPROFILE%\.codex\state_5.sqlite`                                                   | 线程索引/聚合       | 表 `threads`（356 行），含 `tokens_used`、`model`、`cwd`、`git_branch`、`archived`、`cli_version`                                |
| `%USERPROFILE%\.codex\thread_history_1.sqlite`                                          | 投影（turn 时间线） | 表 `thread_turns`（1409 行，含 `started_at`/`completed_at`/`duration_ms`）、`thread_items`（38 399 行，含正文 JSON，**不使用**） |
| `%USERPROFILE%\.codex\session_index.jsonl`                                              | 轻量索引            | 仅 `{id, thread_name, updated_at}`，无 token 字段（`thread_name` 属正文，不使用）                                                |
| `%USERPROFILE%\.codex\logs_2.sqlite`                                                    | 运行日志            | `logs.feedback_log_body` 含自由文本，**不使用**                                                                                  |
| `%USERPROFILE%\.codex\history.jsonl`                                                    | 提示词历史          | `{session_id, ts, text}`，`text` 是提示词正文，**不使用**                                                                        |

### 2.2 证据（实测数字）

- 377 个 rollout 文件、19 379 条 `token_usage_record`、32 787 条 `event_msg.token_count`，解析错误 0。
- 行类型分布：`response_item` 107 513、`event_msg` 89 870、`token_usage_record` 19 360、`turn_context` 2 518、
  `world_state` 1 138、`session_meta` 413、`compacted` 119、`retained_context` 16。
- `threads` 表：356 行（327 未归档 / 29 已归档），`tokens_used` 合计 5 330 876 848，29 行 `tokens_used = 0`。
- 字段覆盖：
  easoning_output_tokens 与 cache_write_input_tokens 在 19 360/19 360 条记录中存在。
- token 合计（261 个含新格式的文件）：input 3 453 701 489、output 15 125 645、cached_input 3 338 992 199、cache_write 676 664、
  easoning_output 5 847 260、 otal 3 468 827 134；仅含旧格式的文件需另走 legacy 路径，故以上为下限。
- 累计单调性：`thread_token_usage.total_tokens` 在同一文件内**无下降**（0 处违例），可用于取"线程最终值"。
- 口径校验：19 379 条记录中 `input_tokens + output_tokens == total_tokens` **全部成立**；
  `cached_input_tokens <= input_tokens`、`reasoning_output_tokens <= output_tokens` 全部成立。

### 2.3 记录结构（`token_usage_record`，0.153.0+）

```text
{ "timestamp": ISO8601, "ordinal": n, "type": "token_usage_record",
  "payload": { "thread_id", "turn_id", "session_id", "root_turn_id", "response_id",
    "usage":               { input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens, reasoning_output_tokens, total_tokens },
    "turn_token_usage":    { 同上 6 个字段 },
    "thread_token_usage":  { 同上 6 个字段 } } }
```

旧格式（0.151.x 及更早，本机 80 个文件仅此路径）：

```text
{ "type": "event_msg",
  "payload": { "type": "token_count",
    "info": { "last_token_usage": {6 个字段}, "total_token_usage": {6 个字段}, "model_context_window": n } } }
```

`session_meta` 提供 `session_id`、`id`(=thread_id)、`timestamp`、`cwd`、`cli_version`、`originator`、`source`、
`base_instructions.provenance.model`、`git.{branch, commit_hash, repository_url}`；
`turn_context` 提供逐轮 `model`、`effort`、`cwd`、`workspace_roots`、`current_date`、`timezone`。

### 2.4 字段映射

| 目标字段   | 来源                                                                   | 等级     | 备注                                                                                   |
| ---------- | ---------------------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------- |
| 会话 ID    | `payload.thread_id`（= `session_meta.id`）/ `payload.session_id`       | measured | 二者不同：356 线程 vs 192 个 `session_id`，**thread 粒度更细**；续接会产生新 thread_id |
| 时间       | 行级 `timestamp`、`state_5.threads.created_at_ms/updated_at_ms`        | measured | JSONL 为 ISO8601（UTC，`Z`）；SQLite 为毫秒时间戳                                      |
| 模型       | `turn_context.model`、`threads.model`                                  | measured | 记录的是本次运行实际使用的模型                                                         |
| 项目       | `turn_context.cwd`、`threads.cwd`、`payload.git.branch/repository_url` | measured | cwd 可直接映射到项目；`projects` 表另有 12 条项目记录                                  |
| 输入 token | `usage.input_tokens`（累计：`thread_token_usage.input_tokens`）        | measured | **已包含** `cached_input_tokens`（子集，不可相加）                                     |
| 输出 token | `usage.output_tokens`                                                  | measured | **已包含** `reasoning_output_tokens`（子集，不可相加）                                 |
| 缓存读取   | `usage.cached_input_tokens`                                            | measured | 命中缓存的输入部分                                                                     |
| 缓存写入   | `usage.cache_write_input_tokens`                                       | measured | 本机 19 385 条中 119 条非 0（合计 676 664），字段可用但命中很少                        |
| 推理 token | `usage.reasoning_output_tokens`                                        | measured | 输出子集，可单独展示                                                                   |
| 成本       | ——                                                                     | 不可得   | 客户端不记录价格；需 TokLens 内置价格表推导                                            |

### 2.5 限制与风险

1. **格式分界**：`token_usage_record` 自 `cli_version 0.153.0` 起出现；0.151.x 及更早只有 `event_msg.token_count`。
   本机 353 个文件（`sessions`）中：27 个只有新格式、74 个只有旧格式、252 个两者都有。解析必须双路径。
2. **两套记录同时存在时的关系（2026-10-11 补充实测，重要）**：同一文件里两套记录并非总是镜像关系。
   - 新格式窗口内的每条请求，旧格式会写一条数值相同的镜像事件；这些镜像必须丢弃，否则同一请求会被计两次。
   - 新格式开始之前的历史在混合文件里**只以旧格式存在**（本机 81 个混合文件出现了这种前缀），必须保留为明细。
   - 两个累计计数器起点不同：`thread_token_usage.total_tokens` 可能只覆盖新格式窗口（实测某线程仅 2,545,710），
     而 `total_token_usage` 覆盖整条旧格式链（同线程 34,754,480）。**线程总量必须取两者的最大值**，
     既不能相加，也不能固定偏向新格式。按最大值的口径与 Codex 自身 `state_5.sqlite` 的 `threads.tokens_used`
     对账：309 个有数据的线程中 307 个完全一致，剩余差值合计 331,155（0.006%）。
3. **续接会话**：8 个文件的 `history_base.thread_id != payload.id`，其 `thread_token_usage` 含继承的父线程累计值；
   本机这 8 个父线程都不在 `threads` 表中（未发现重复计数），但**一旦父线程仍存在同表，按行相加会重复计数**。
   必须按 thread_id 取"最终累计值"，禁止把多文件相加。
4. **`tokens_used` 存在滞后**：当前活跃线程 SQLite 值 4 799 144 vs JSONL 最终累计 4 941 884（差 142 740）；
   其余 7 个已完成线程两者完全一致。结论：SQLite 适合快速列表/筛选，精确值以 JSONL 累计计数器（见上一条）为准。
5. **体量大**：1 GB JSONL 全量解析约 8~10 秒（本机 Node 流式）；UI 首次扫描需异步 + 缓存。
6. **同目录存在敏感内容**：`thread_history_1.sqlite`、`logs_2.sqlite`、`history.jsonl`、`session_index.jsonl(thread_name)`
   含正文/提示词，采集实现必须严格限定到上述字段白名单。
7. **SQLite 为活跃 WAL 库**：必须以只读方式打开；`threads` 表另有 `title`/`preview`/`first_user_message` 等正文字段，禁止读取。

## 3. Claude Code

### 3.1 数据路径

| 路径                                                                            | 作用         | 说明                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `%USERPROFILE%\.claude\projects\<cwd 变形>\<sessionId>.jsonl`                   | 主会话转录   | 目录名是 cwd 的字符替换形式（如 `D--Repos-RuoYi-Vue`），不保证可逆；每行带 `cwd`，应以行内 `cwd` 为准                                                                                                                                                                                               |
| `%USERPROFILE%\.claude\projects\<cwd 变形>\<sessionId>\subagents\agent-*.jsonl` | 子代理转录   | 每个 Task 子代理一个文件，`isSidechain=true`；与主文件**无 message.id 重叠**                                                                                                                                                                                                                        |
| `%USERPROFILE%\.claude\projects\...\subagents\agent-*.meta.json`                | 子代理元数据 | 小文件（约 140 B）                                                                                                                                                                                                                                                                                  |
| `%USERPROFILE%\.claude.json`                                                    | 全局状态     | 含每个项目的 `lastTotalInputTokens`、`lastTotalOutputTokens`、`lastTotalCacheReadInputTokens`、`lastTotalCacheCreationInputTokens`、`lastCost`、`lastSessionId`、`lastModelUsage`（**仅最后一次会话**）；同文件含 `userID`、`machineID`、`mcpServers`，部分项目条目含提示词历史，**必须白名单读取** |
| `%USERPROFILE%\.claude\history.jsonl`                                           | 提示词历史   | `{display, pastedContents, timestamp, project, sessionId}`，`display`/`pastedContents` 属正文，**不使用**                                                                                                                                                                                           |
| `%USERPROFILE%\.claude\settings.json`                                           | 配置         | 仅 `env` 键，可能含凭据，**不读取值**                                                                                                                                                                                                                                                               |
| `%USERPROFILE%\.claude\{debug,backups,file-history,jobs,teams,plugins}`         | 辅助数据     | 与用量统计无关                                                                                                                                                                                                                                                                                      |
| `%USERPROFILE%\.claude\cache\`                                                  | 无用量缓存   | 仅有 `changelog.md` 等，无 `stats-cache.json`                                                                                                                                                                                                                                                       |

### 3.2 证据（实测数字）

- 72 个 `.jsonl`（14 个主目录 + 子代理目录），39 个唯一 `sessionId`；解析错误 0。
- `assistant` 行 4 956 条且**全部**含 `message.usage`；`user` 行 2 645 条；`isSidechain` 真 1 172 / 假 3 784（与文件位置一致：主文件全为 false，子代理文件全为 true）。
- **重复写入**：同一 `(sessionId, message.id)` 最多出现 15 次；本机 1 735 个唯一消息对应 4 956 行，即 1 531 个键被重复写入。
  抽样 218 个重复键，`(input, output, cache_read)` 值在重复之间**完全一致**（说明是重复落盘，而非流式增量）。
- 去重后合计：`input 32 817 400`、`output 1 172 759`、`cache_read 189 724 162`、`cache_creation 626 365`；
  其中主转录 `input 30 475 975 / output 899 067 / cache_read 176 526 850 / cache_creation 626 365`，
  子代理 `input 2 341 425 / output 273 692 / cache_read 13 197 312 / cache_creation 0`。
- 版本跨度 `2.1.161 .. 2.1.294`；模型值含 `deepseek-v4-flash`、`claude-sonnet-5`、`claude-opus-5`、`<synthetic>` 等（代理/自定义模型名）。
- `usage.cache_creation.ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens` 在 136 行非 0；`usage.iterations` 仅 3 行非空。

### 3.3 记录结构（`type = "assistant"`）

```text
{ "type": "assistant", "uuid", "parentUuid", "sessionId", "timestamp", "cwd", "gitBranch",
  "version", "userType", "isSidechain", "message": {
      "id", "model", "stop_reason", "type",
      "usage": { "input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens",
                 "service_tier", "speed", "inference_geo",
                 "cache_creation": { "ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens" },
                 "server_tool_use": { "web_fetch_requests", "web_search_requests" },
                 "iterations": [ ... ] } } }
```

其他行类型：`user`（2 645 行）、`system`、`queue-operation`、`attachment`、`ai-title`、`custom-title`、`last-prompt`、`mode`。
这些行不含 token 用量，但 `mode`/`system`/`attachment` 可用于辅助时间线。

### 3.4 字段映射

| 目标字段   | 来源                                                                               | 等级                                               | 备注                                                   |
| ---------- | ---------------------------------------------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------ |
| 会话 ID    | `sessionId`（行级）                                                                | measured                                           | 主文件与子代理文件共享同一 `sessionId`                 |
| 请求标识   | `message.id`                                                                       | measured                                           | 去重主键（配合 `sessionId`）                           |
| 时间       | `timestamp`（ISO8601，UTC `Z`）                                                    | measured                                           | 每条 assistant 记录一个时间点                          |
| 模型       | `message.model`                                                                    | measured                                           | 含代理模型名与 `<synthetic>`（合成消息，需过滤或标记） |
| 项目       | `cwd`（推荐）、`gitBranch`；目录名为辅助                                           | measured                                           | 目录名不可靠可逆，行内 `cwd` 才是准值                  |
| 输入 token | `message.usage.input_tokens`                                                       | measured                                           | 与缓存读取/写入**不重叠**（Anthropic 语义）            |
| 输出 token | `message.usage.output_tokens`                                                      | measured                                           | 与 input 不重叠                                        |
| 缓存读取   | `message.usage.cache_read_input_tokens`                                            | measured                                           | 独立字段                                               |
| 缓存写入   | `message.usage.cache_creation_input_tokens`（细分见 `cache_creation.ephemeral_*`） | measured                                           | 本机 5m/1h 细分仅 136 行非 0                           |
| 子代理用量 | 同字段，来自 `subagents/agent-*.jsonl`                                             | measured                                           | 需与主转录合并统计，但注意按会话区分                   |
| 成本       | ——                                                                                 | 不可得（除 `.claude.json` 的 `lastCost` 单会话值） | 需价格表推导                                           |

### 3.5 限制与风险

1. **重复写入必须去重**：不去重会把本机总量高估约 2.9 倍（4 956 行 → 1 735 条唯一消息）。
2. **无会话级聚合表**：`session`/`sessionId` 级指标需自行聚合；`.claude.json` 只有"最后一次会话"的快照，不能当历史。
3. **子代理用量在独立文件**：只读主转录会漏掉约 7% 的输入与 23% 的输出（本机比例）。
4. **正文无处不在**：`user`/`assistant` 行的 `message.content`、`history.jsonl`、部分项目配置都含正文，必须字段白名单化。
5. **可能存在自动清理**：`%USERPROFILE%\.claude\.last-cleanup` 记录了 `2026-10-08T15:01:59Z` 的清理动作，
   本机最早会话为 2026-06-05。**历史深度是否受保留策略限制尚未确认**（需继续观察或查文档）。
6. **无成本字段**，且存在代理/自定义模型名（`deepseek-v4-flash` 等），价格表若只覆盖官方模型会大面积"未定价"。

## 4. OpenCode

### 4.1 数据路径

| 路径                                                                  | 作用                        | 说明                                                                        |
| --------------------------------------------------------------------- | --------------------------- | --------------------------------------------------------------------------- |
| `%USERPROFILE%\.local\share\opencode\opencode.db`                     | **主数据库**（SQLite，WAL） | 1 186 MB；`journal_mode=wal`；最后写入 2026-10-11                           |
| `%USERPROFILE%\.local\share\opencode\storage\session_diff\ses_*.json` | 会话 diff 占位              | 均为 2 B 空对象；旧 JSON 存储已迁移到 SQLite（`kv` 中有 `migration.v1-v2`） |
| `%USERPROFILE%\.local\share\opencode\log\*.log`                       | 运行日志（10.5 MB）         | 非统计必需                                                                  |
| `%USERPROFILE%\.local\share\opencode\snapshot\`                       | 文件快照（431 个文件）      | 与 token 统计无关                                                           |
| `%USERPROFILE%\.config\opencode\`                                     | 配置与插件                  | `auth.json` 等含凭据，**不读取**                                            |

数据库表（只读列出）：

| 表                                           | 行数         | 用途                                                                                                                                                                                                    |
| -------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `session` / `session_v2`                     | 49 / 48      | 会话聚合：`tokens_input/output/reasoning/cache_read/cache_write`、`cost`、`model`、`version`、`parent_id`、`directory`、`time_created/updated/archived`                                                 |
| `message`                                    | 5 776        | 逐条消息 JSON（`data`）：`tokens.{input,output,reasoning,cache.read,cache.write,total}`、`cost`、`modelID`、`providerID`、`variant`、`mode`、`agent`、`time.created/completed`、`path.cwd/root`、`role` |
| `part`                                       | 23 141       | 内容片段（text/tool/step-start/step-finish/...），**不使用**                                                                                                                                            |
| `project` / `project_directory` / `worktree` | 21 / 18 / 17 | 项目维度：`worktree`、`vcs`、`time_active`                                                                                                                                                              |
| `session_message`                            | 5 807        | 会话事件流：`type`(assistant/user/system/compaction/synthetic/idle) + 自增 `seq`                                                                                                                        |
| `event` / `event_sequence`                   | 348 / 50     | 事件溯源（仅 2 个聚合），不可作为水位                                                                                                                                                                   |
| `credential` / `account` / `account_state`   | 3 / 0 / 0    | 凭据，**禁止读取**                                                                                                                                                                                      |

### 4.2 证据（实测数字）

- `session` 合计：`input 16 997 781`、`output 1 468 457`、`reasoning 1 006 064`、`cache_read 1 284 944 667`、`cache_write 0`、`cost 4.93`；
  `session_v2` 为 `16 697 113 / 1 455 035 / 1 006 083 / 1 284 178 075 / 0 / 4.93`。两表 47 行重叠且 token 值**完全一致**（0 行不一致）。
- 一致性：抽 8 个会话，`session.tokens_input/output/cache_read` 与 `message` 表按会话求和**逐字段完全相等**；
  说明会话列是消息的物化聚合，二者不可相加。
- 口径校验：5 232 条消息含 `tokens.total`，`total == input + output + reasoning + cache.read` **全部成立**（0 处不一致）。
- 抽样 4 000 条消息：`message.data.tokens.cache.write` 全为 0；`cost` 仅 3 798 条 assistant 消息有值（0 ~ 0.0916）。
- `parent_id` 非空的会话 20/49（子代理/嵌套会话）；已归档 1 个。
- 版本列出现 `1.18.16 .. 1.18.34` 与 `2.0.22`；模型列是 JSON 字符串（`{"id","providerID","variant"}`），
  消息里旧格式为扁平 `modelID`/`providerID`，新格式为嵌套 `model.modelID`（本机 202 条）。

### 4.3 字段映射

| 目标字段   | 来源                                                                       | 等级                  | 备注                                        |
| ---------- | -------------------------------------------------------------------------- | --------------------- | ------------------------------------------- |
| 会话 ID    | `session.id`（`ses_*`），父会话 `parent_id`                                | measured              | 子会话需标记，避免与父会话混淆              |
| 消息 ID    | `message.id`                                                               | measured              | 请求级主键                                  |
| 时间       | `session.time_created/updated`、`message.data.time.created/completed`      | measured              | 毫秒时间戳；有真实耗时（可推导时长）        |
| 模型       | `session.model`（JSON）、`message.data.modelID` + `providerID` + `variant` | measured              | 需解析 JSON，兼容扁平/嵌套两种写法          |
| 项目       | `session.directory`、`project.worktree`、`message.data.path.cwd/root`      | measured              | `project` 表另有 `vcs`、`time_active`       |
| 输入 token | `session.tokens_input` / `message.data.tokens.input`                       | measured              | 与缓存读取不重叠（见 total 公式）           |
| 输出 token | `session.tokens_output` / `message.data.tokens.output`                     | measured              |                                             |
| 推理 token | `session.tokens_reasoning` / `message.data.tokens.reasoning`               | measured              | 独立字段，非 `output` 子集                  |
| 缓存读取   | `session.tokens_cache_read` / `message.data.tokens.cache.read`             | measured              | 本机占比最大（1.28 B）                      |
| 缓存写入   | `session.tokens_cache_write` / `message.data.tokens.cache.write`           | measured              | 本机全 0，需按客户端/供应商确认             |
| 成本       | `session.cost` / `message.data.cost`                                       | derived（客户端自算） | 客户端按 provider 价位计算，币种/精度需确认 |

### 4.4 限制与风险

1. **双表并存**：`session` 与 `session_v2` 在迁移期同时存在且内容基本一致，**绝对不能相加**；
   应"按 id 合并，优先 `session_v2`，缺失则回落到 `session`"（本机各有 1~2 行独有）。
2. **SQLite 活跃写入**：WAL 模式、应用运行时会持续写入；必须以只读打开，必要时先复制快照（需用户显式同意）再解析。
3. **`data` 是大 JSON**：`message.data`、`part.data`、`session_message.data` 内含正文，必须只取白名单字段。
4. **旧版 JSON 存储已迁移**：本机没有历史 JSON 存储可读；若目标机器是旧版本 OpenCode，需要额外适配 `storage/session|message|part` 目录结构（本机无样本，**未验证**）。
5. **`event` 表覆盖不全**（仅 2 个聚合），不能作为增量水位；应使用 `session.time_updated` 与 `message.time_updated`。

## 5. 字段可得性总结（三客户端对照）

| 能力             | Codex                                                     | Claude Code                                          | OpenCode                             |
| ---------------- | --------------------------------------------------------- | ---------------------------------------------------- | ------------------------------------ |
| 请求级输入 token | 实测（`usage.input_tokens`，含缓存）                      | 实测（`usage.input_tokens`，不含缓存）               | 实测（`tokens.input`）               |
| 请求级输出 token | 实测（`usage.output_tokens`，含推理）                     | 实测（`usage.output_tokens`）                        | 实测（`tokens.output`，推理单列）    |
| 请求级缓存读取   | 实测（`cached_input_tokens`，**是 input 子集**）          | 实测（`cache_read_input_tokens`，独立）              | 实测（`cache.read`，独立）           |
| 请求级缓存写入   | 实测（`cache_write_input_tokens`，本机仅 119 条非 0）     | 实测（`cache_creation_input_tokens`，含 5m/1h 细分） | 实测（`cache.write`，本机 0）        |
| 会话级累计       | 实测（`thread_token_usage` 累计 + `threads.tokens_used`） | 需自行聚合                                           | 实测（`session.tokens_*`）           |
| 模型             | 实测（含 effort）                                         | 实测                                                 | 实测（含 provider/variant）          |
| 项目             | 实测（cwd + git 信息）                                    | 实测（cwd + gitBranch）                              | 实测（directory + project.worktree） |
| 成本             | **不可得**                                                | **不可得**（仅 `.claude.json` 单会话 `lastCost`）    | 实测（客户端自算，币种待确认）       |
| 请求耗时         | 可推导（`turn_context` + 事件时序）                       | 可推导（相邻 timestamp 差）                          | 实测（`time.created/completed`）     |
| 消息正文         | 有（**不采集**）                                          | 有（**不采集**）                                     | 有（**不采集**）                     |

可靠推导 vs 不可得：

- **可靠推导**：会话时长（`thread_turns.duration_ms` / `time.created~completed` / 相邻事件间隔）；成本（价格表 × token，须标注为 `derived`，且必须能提示"未定价模型"）；缓存命中率（`cache_read ÷ (input + cache_read)`，仅在口径澄清后）。
- **不可得**：客户端计费的真实现金金额（除 OpenCode 自算值）；超出本地记录范围的请求（含被客户端清理的历史）；被代理/中转改写后的真实供应商用量。

## 6. 历史导入、增量同步、去重、升级兼容评估

### 6.1 历史导入

- 一次性全量扫描：Codex 约 1 GB JSONL（本机 8~10 s 流式解析）——建议**优先走 `state_5.sqlite.threads` 快速填充列表**，
  再按需对单线程解析 JSONL 取精确值；Claude Code 31 MB（亚秒级，直接全量）；OpenCode 直接查询 SQLite 聚合列（最快）。
- 导入必须幂等：以唯一键 upsert，重复执行不产生新记录。
- 导入上限应可见：UI 需要展示"扫描到的客户端/会话/未解析记录数"，而不是静默丢弃。

### 6.2 增量同步

| 客户端      | 增量依据                                                        | 说明                                                                                                                            |
| ----------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Codex       | 文件 `mtime + size`，文件内按 `ordinal` / 字节偏移续读          | rollout 为 append-only；`thread_history_1.thread_history_projection_state` 里 `next_rollout_byte_offset` 证明客户端也采用该策略 |
| Claude Code | 文件 `mtime + size`，按行续读；`(sessionId, message.id)` upsert | 同一 message 会被重复写入，不能只按行号追加                                                                                     |
| OpenCode    | `session.time_updated` / `message.time_updated`（毫秒）         | SQLite 可直接增量查询；表 `event` 不完整，不要依赖                                                                              |

### 6.3 去重（最关键）

| 客户端      | 唯一键                              | 取值规则                                                             | 反例（会造成重复计数）                                              |
| ----------- | ----------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Codex       | `thread_id`（+ `response_id` 明细） | 每线程取"最终累计 `thread_token_usage`"；请求级按 `response_id` 去重 | 多文件相加；把 `total_token_usage` 与 `token_usage_record` 同时累加 |
| Claude Code | `(sessionId, message.id)`           | 重复出现时保留任意一条（值一致）                                     | 按行累加（本机高估 ~2.9 倍）                                        |
| OpenCode    | `message.id`                        | 以 `message` 明细为事实源，`session.tokens_*` 作为聚合缓存           | `session` + `session_v2` 相加；session 列与 message 求和相加        |

### 6.4 升级兼容

- **Codex**：`token_usage_record`（≥0.153.0）与 `event_msg.token_count`（≤0.151.x）双路径解析；两者字段名相同，可复用同一映射；
  需容忍新行类型（`world_state`、`retained_context`、`compacted`）与新增字段。
- **Claude Code**：23 个版本共存的字段稳定（`message.usage.*` 一直在）；新增 `cache_creation.ephemeral_*`、`iterations[]`、
  `<synthetic>` 模型值，应按"未知字段忽略、未知模型标记"处理。
- **OpenCode**：`session` → `session_v2` 迁移 + 模型字段扁平→嵌套，需要"并表 + 双写法解析"；
  表结构由 `migration` 表（48 条迁移记录）驱动，读取方应做列存在性检测（`PRAGMA table_info`）而不是硬编码。
- 通用建议：解析层只依赖白名单字段，字段缺失按"缺失"处理并在 UI 显示"未解析/未识别"计数。

## 7. 无法确认 / 待验证事项

1. Codex `threads.tokens_used` 对**活跃线程**存在滞后（本机差 142 740），需确认是写入节流还是口径差异；
   若要用 SQLite 出精确值，必须做一次"活跃线程回落到 JSONL"的处理。
2. Codex 续接会话（`history_base`）在"父线程仍存在"的场景是否会重复计数——本机 8 例父线程均已不在 `threads` 表中，无法证伪。
3. Claude Code 的历史保留策略：`.last-cleanup` 存在，但**清理周期与是否删除转录文件未确认**（无法从本机数据判定）。
4. Claude Code `usage.iterations` 语义（仅 3 条非空）未确认，暂不纳入口径。
5. OpenCode `cost` 的币种与定价来源未确认（`providerID` 含自建中转，如 `kuncode`、`chain888`、`opencode-go`）。
6. OpenCode 旧版本（JSON `storage/` 结构）的样本在本机不存在，兼容方案未经验证。
7. 三个客户端在本机都通过**第三方中转/代理**使用自定义模型名（`deepseek-flash`、`mimo-v2.5-pro` 等），
   官方价格表无法覆盖；成本口径与价格表来源必须由项目显式决策（见 `docs/data-metrics.md` E3）。
8. 以上结论均为本机单机实测（3 个客户端的当前版本），**未与官方文档交叉验证**，也未覆盖 macOS/Linux 的路径差异。

## 8. 建议的下一步（仍不写采集代码）

1. 先定义归一化映射与去重键（对应 `docs/architecture.md` 第 4 节），并把本文第 6.3 节的唯一键写入 `docs/data-metrics.md`。
2. 决策读取策略：Codex "SQLite 快路径 + JSONL 精确值"、Claude Code "JSONL 全量去重"、OpenCode "SQLite 直查"。
3. 定义 fixture 规范：从上述结构派生**手工构造**的去敏样本（禁止复制真实会话），覆盖旧/新格式、续接会话、重复写入、缺失字段。
4. 决策成本口径：价格表来源、自定义/中转模型的"未定价"表现、币种与精度（`docs/data-metrics.md` 第 5、7 节）。
5. 明确隐私边界：实现时只允许读取本文列出的白名单字段，禁止读取 `thread_items`、`part`、`message.content`、`history.jsonl`、`*.json` 凭据。

## 9. DSH 补充调研（2026-10-11）

用户确认 DSH 指 deepseek-hermes。本机 `~/.dsh/sessions` 存在 8 份 `session.jsonl.zstd` 活动产物与 4 份 v4 迁移备份。
核对本机 deepseek-harness 源码：`session-persistence-jsonl/src/format.ts` 定义 header / 文件布局，
`zstd.ts` 定义串联 checksummed Zstandard 帧；`core/session/src/types.ts` 定义 v0 / seq / time / seedLength；
`llm-deepseek/src/translate.ts` 表明 inputTokens 已扣除缓存命中、outputTokens 为 completion_tokens（含 reasoning）。
按客户端 token-meter 规则对 assistant/chunk 的早期 usage 与 assistant/message 的最终 usage 按 turn / step 替换；不统计打包文本 / 推理 chunk、标题 / 搜索插件的额外 LLM 调用。
该范围与已持久化核心请求日志一致，不能宣称覆盖 DSH 所有插件调用。token-meter 的客户端无关投影契约与 pi-ai 适配器也已核对；SQLite 后端未验证，未知持久化格式拒绝提交。
检查只输出字段形态与聚合数值，不输出会话正文、标题、系统提示词或凭据；未修改任何源文件。
最新三客户端对账数字及缺失情况见 `data-model.md` 第 10 节。

## Kimi Code — 本机数据与客户端契约（2026-10-11）

只读检查用户指定 `.kimi-code`：2 份会话 state 与 2 份 agent wire（协议 1.4 / 1.5）。
另有插件 state，仅含 enabled，不属于会话，按目录形状排除。
现代 state version 2 含 id / cwd / 毫秒时间；旧版无 id，workDir / ISO UTC 时间，
客户端迁移逻辑回退到完整 session_* 目录名。根目录由客户端 `KIMI_CODE_HOME` 配置覆盖。

证据来自已安装 kimi.exe 内嵌 JavaScript 的 provider adapter、addUsage、usageTotal、record 与 migration 代码，
只检索字段/函数契约，未运行客户端命令、未输出正文或凭据。
OpenAI adapter 的 inputOther = promptTokens - cached；Anthropic adapter 的 inputOther 与缓存字段独立。
usageTotal = inputOther + output + inputCacheRead + inputCacheCreation；record 默认 session scope，
turn scope 也计入会话累计，故 `usage.record` 是增量而非累计；相同 usage 值不可内容去重。
`token_counting.measured` / `turn_recorded` 是上下文长度，服务端镜像不重复扫描。

独立只读脚本与内存库导入对账：2 会话、2 usage、普通输入 31,422、输出 925、缓存读取 28,800、
缓存写入 0、规范总量 61,147；另一个会话无 usage，保留未知。没有未识别行。
这些是本次采样结果，不是固定 fixture；自动测试仅使用合成数据。

## MiMoCode — SQLite 与去重（2026-10-11）

本机用户指定 ~/.local/share/mimocode/mimocode.db 包含 session / message / part；
只读 schema 和 SQL JSON 投影确认 message.data.tokens 独立字段：input、output、reasoning、cache.read、cache.write、可选 total。
635 个 assistant 中 613 条有 total，全部满足 total = 五个桶之和；cache.read > input 出现 583 次，reasoning > output 出现 60 次。
已安装 Xiaomi MiMo app.asar/out/main/node.mjs 的 getUsage 函数确认 input 预先扣除缓存，output 预先扣除 reasoning；cache.write 独立计入。
MIMOCODE_DB 的绝对/相对覆盖与 XDG data 根同样由该模块确认。这里只检索安装源码中的字段契约，不执行客户端、不输出正文或凭据。

导入标记 claude_import 中 3 个会话的 message_ids 对应 17 个 assistant 副本；external_import 为空。
仅排除登记的导入消息，不因会话曾导入而丢弃其后续本地请求；纯导入会话不计入 MiMo。
独立只读 SQL 与采集器对账：14 个本地会话、618 个请求、input 3,581,156、output 198,576、
reasoning 25,596、cache.read 65,200,832、cache.write 0，总量 69,006,160。全库（含导入）会得到 70,582,900，差值来自外部副本。
MiMo cost 是 getUsage 按价格表计算，当前不作为客户端实测费用导入。

## Pi Desktop — schema 1 与物理操作账本（2026-10-11）

用户指定 ~/.pi-desktop/sessions 有一份 schema 1 JSONL：session header、message 行，统计来自 assistant.meta.usage。
本机 16 个 assistant operation，input 26,490、output（含推理）7,062、reasoning 1,843、cacheRead 320,896、cacheWrite 未提供。
全部满足 total = input + output + cacheRead + cacheWrite，总量 354,448；归一化普通 output 为 5,219，不重复加 reasoning。
一条未含 cacheRead，schema 中可选缓存字段按 native ledger 默认 0；不读取 blocks、system meta 中的 messageJson。

源码依据：[vastsa/PI-Desktop](https://github.com/vastsa/PI-Desktop/tree/f24984d69667243c4e763b7009e07015be6677d1)，
核对 packages/agent-runtime/src/agent-messages.ts（usageFromPi / usageToPi）、request-usage.ts、
packages/shared/src/message-usage.ts（addUsage）和 crates/host-core/src/sessions/usage.rs。
operationId 是物理请求，operations 是原子账本，父 aggregate 不得与子请求再相加；完整总量优先，legacy 未归属残差仍可加。
usageOrigin 当前约定 pi / legacy；costStatus 明确 reported / estimated / unknown，本机全部为 estimated，当前不导入 cost。
LobeHub XiaomiMiMo 对应小米品牌；其 Pi 标识为 Inflection AI，Pi Desktop 使用本地通用终端图标避免错标。
这些数字是本机只读时点快照；自动回归使用合成 fixture，无真实会话正文进入仓库。
