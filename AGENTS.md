# AGENTS.md - TokLens 开发规范

本文件是 TokLens 的唯一入口规范，适用于本仓库内的全部代码、脚本与文档，人和 AI Agent 同样遵守。
更细的规则拆分为四份配套文档（索引见 `docs/README.md`），出现冲突时以本文件的约束优先级为准：

| 文档                    | 负责内容                                        |
| ----------------------- | ----------------------------------------------- |
| `docs/architecture.md`  | 架构分层、模块边界、依赖方向、架构决策登记      |
| `docs/ui-guidelines.md` | UI/UX 设计规范、组件与可视化规范                |
| `docs/data-metrics.md`  | 数据来源、统计口径、估算规则、隐私默认值        |
| `docs/data-model.md`    | 统一数据模型、Token 语义、去重键、SQLite 表结构 |

## 1. 项目目标与边界

TokLens 是一个**本地优先**的桌面应用，用于统计本机 **Codex、Claude Code、OpenCode** 等 AI 编程客户端的使用情况
（会话、请求、token、模型、成本、时间分布等）。

必须坚持的三条底线：

1. **本地优先**：默认不联网、不上传、不做遥测。
2. **真实与估算分离**：任何指标都必须能回答"这是采集到的真实数据，还是推算出来的估算数据"（见 `docs/data-metrics.md`）。
3. **隐私默认保护**：只读取统计所需的最小元数据，默认不落盘、不打印、不外传会话正文。

当前仓库状态：已有本地 SQLite 用量库、Codex 只读采集器、Tauri 查询与同步命令，以及总览 / 用量分析 /
会话明细 / 数据源管理页面。Claude Code 与 OpenCode 采集器、费用换算尚未实现。
新增架构组件（持久化、状态管理、测试框架、错误处理库等）前，必须先在 `docs/architecture.md` 的决策记录中登记。

明确不做的事：云端账号体系、远程同步、修改或删除客户端的原始会话数据、绕过客户端自身配置读取数据、采集与统计无关的正文内容。

## 2. 技术栈与工具链

| 层       | 技术                                                     |
| -------- | -------------------------------------------------------- |
| 桌面外壳 | Tauri 2（identifier：`com.toklens.app`）                 |
| 前端     | React 19 + TypeScript + Vite                             |
| 样式     | Tailwind CSS v4（CSS-first，见 `src/index.css`）         |
| 组件     | shadcn/ui（new-york、baseColor neutral）+ lucide-react   |
| 后端     | Rust（edition 2024，Tauri command 为前后端唯一边界）     |
| 包管理   | pnpm（唯一包管理器）                                     |
| 校验     | ESLint（flat config）+ Prettier + tsc + rustfmt + clippy |

- 只使用 pnpm；不要生成 `package-lock.json` / `yarn.lock`，不要提交它们。
- 编辑器：安装 `.vscode/extensions.json` 推荐的扩展，并把 Prettier 设为默认格式化工具（保存时格式化）；提交前仍以 `pnpm check` 结果为准。
- 依赖版本以 `package.json` 与 `Cargo.toml` 为准，命令以 `package.json` 的 `scripts` 为准（本文件只做说明）。

常用命令：

```bash
pnpm install          # 安装前端依赖
pnpm tauri dev        # 启动桌面应用（前端 + Rust 热重载）
pnpm dev              # 仅启动前端（无 Tauri API，仅用于纯 UI 调试）
pnpm lint             # ESLint
pnpm lint:fix         # ESLint 自动修复
pnpm typecheck        # tsc 类型检查（src + vite.config.ts）
pnpm format           # Prettier 写入
pnpm format:check     # Prettier 校验
pnpm check            # lint + typecheck + format:check（提交前必跑）
pnpm rust:fmt         # cargo fmt 写入
pnpm rust:fmt:check   # cargo fmt --check
pnpm rust:clippy      # cargo clippy --all-targets -- -D warnings
pnpm rust:test        # Rust 单元测试（cargo test）
pnpm rust:check       # rust:fmt:check + rust:clippy + rust:test（Rust 改动必跑）
pnpm tauri build      # 生产打包
```

## 3. 目录职责

| 路径                        | 职责                                                                                |
| --------------------------- | ----------------------------------------------------------------------------------- |
| `src/`                      | 前端应用源码（React）                                                               |
| `src/components/ui/`        | shadcn CLI 生成的组件：视为 vendor 代码，不手改（已豁免 ESLint/Prettier）           |
| `src/components/`           | 业务组件（不依赖 Tauri 细节，数据统一来自 `src/lib/api.ts`）                        |
| `src/lib/`                  | 纯函数与工具（不得依赖 React）；`api.ts` 是唯一 Tauri 调用与 DTO 封装入口           |
| `src/hooks/`                | 复用 Hook；`use-mobile.ts` 为 shadcn 生成物，同样视为 vendor                        |
| `src/pages/`                | 页面级组件（总览 / 用量分析 / 会话明细 / 数据源管理）                               |
| `src-tauri/src/`            | Rust 侧：数据采集、解析、归一化、聚合、持久化与 Tauri command                       |
| `src-tauri/src/usage/`      | 统一数据模型 / Token 口径 / SQLite 存储（不含采集器与 UI，见 `docs/data-model.md`） |
| `src-tauri/src/collectors/` | 客户端采集器：发现/解析/增量导入（当前仅 Codex，见 `docs/data-model.md` 第 8 节）   |
| `src-tauri/src/commands/`   | Tauri command 层：应用状态、后台同步调度与前端 DTO（口径仍由 `usage` 提供）         |
| `src-tauri/capabilities/`   | Tauri 2 权限声明（最小权限，见第 5 节）                                             |
| `docs/`                     | 规范文档（架构 / UI / 数据统计）                                                    |
| `public/`                   | 原样拷贝的静态资源                                                                  |

约束：

- 依赖方向单向：前端 → Tauri command；查询 command → `usage/analytics` → `usage/store`；
  同步 command → `collectors` → `usage/normalize` / `usage/store`。`usage` 不反向依赖 command 或采集器。
  禁止跨层跳步（例如前端直接读文件、command 里写统计口径），模块图见 `docs/architecture.md`。
- 业务组件、领域模块等新目录在引入时同步登记到 `docs/architecture.md`，不要先建目录后补文档。
- `src/components/ui/` 内不得添加业务逻辑；业务组件不要直接 import 具体客户端解析细节。
- 按变更原因拆分职责：Token 类型 / 语义 / 换算、客户端归一化、文件发现 / 解析 / 同步、连接 / 迁移 / 写入 / 查询分开。
  不按固定行数拆文件，不为每个结构体创建文件；较大模块用目录组织，`mod.rs` 保持薄入口与显式导出。
- 模块内部默认私有，跨子模块优先 `pub(super)`；拆分不扩大公开接口。公开路径可用重导出兼容，避免连带改动调用方。
- 结构重构保留统计规则、DTO 与迁移 SQL；口径或行为修复另做提交并补相应测试，不与文件搬移混在一起。

## 4. 代码规范

### TypeScript / React

- 保持 `tsconfig.json` 现有严格设置（`strict`、`noUncheckedIndexedAccess`、`verbatimModuleSyntax` 等），不要为了省事关闭。
- 禁止 `any`：用 `unknown` + 类型收窄；禁止 `@ts-ignore`，确需豁免时用 `@ts-expect-error` 并写明原因。
- 只作为类型使用时必须 `import type`（与 `verbatimModuleSyntax` 一致）。
- 跨端结构体/枚举的 TS 类型与 Rust DTO 一一对应，命名保持一致（见 `docs/architecture.md`）。
- 文件命名：组件 `PascalCase.tsx`；非组件模块与 Hook `kebab-case.ts`；导出用 `camelCase` / `PascalCase`，常量用 `UPPER_SNAKE_CASE`，布尔用 `is/has/can` 前缀。
- 组件使用函数组件 + Hook；不使用 class 组件；副作用集中、可读，避免在渲染期间做 I/O 或数据推算。
- 前端只负责呈现：统计口径、聚合、单位换算放在 Rust；界面层做纯展示格式化。
- 格式化结果由 Prettier 决定，不要手工对齐；提交前跑 `pnpm format`。
- 控制台输出遵循 `no-console` 规则：仅允许 `warn` / `error`，调试日志不要留在提交里。

### Rust

- 遵循 rustfmt 默认风格，`clippy -D warnings` 必须零告警。
- 错误处理：可恢复路径返回 `Result`，不要 `unwrap()`；启动期不可恢复的失败才允许 `expect(...)` 并写明原因。
- 禁止 `todo!()` / `unimplemented!()` 进入主分支；未知输入按"忽略未知字段 + 记录计数"处理，不要 panic。
- Tauri command 使用 `snake_case` 命名，返回可序列化结构；`Result<T, E>` 的错误类型必须能序列化给前端。
- 长耗时的文件扫描/解析不得阻塞主线程；跨端 DTO 变更必须同步更新前端类型。

### 提交信息

采用 `type(scope): 描述` 形式（与仓库现有 `chore: ...` 风格一致），`type` 取 `feat` / `fix` / `chore` / `docs` /
`refactor` / `test` / `build` / `ci`。描述统一使用英文，与已有提交历史保持一致；一次提交只做一件事，说明做了什么。

## 5. 安全要求

- **最小权限**：`src-tauri/capabilities/default.json` 只声明当前功能必需的权限。新增 `fs`、`shell`、`http` 等权限必须在
  PR/提交说明中给出用途、范围与替代方案评估。
- **不引入高风险能力**：不得使用 `shell:allow-execute`、任意路径 `fs` 读写、远程 `http` 等，除非功能必需且经过人工确认。
- **CSP**：`tauri.conf.json` 当前为 `csp: null`（脚手架默认）。在接入任何外部资源、内联脚本或自定义协议前，必须先配置 CSP。
- 前端不执行远程代码、不 `eval`、不加载远程脚本；外部链接统一通过 `tauri-plugin-opener` 交给系统浏览器打开。
- 不引入带遥测/统计上报的依赖；新增依赖需说明理由、维护状态与体积影响，且必须来自 npm 官方 registry。
- 不在代码、日志、错误信息或仓库中存放任何密钥、令牌、真实会话内容；本地数据目录、`.env`、导出文件不得入库。

## 6. 数据处理原则

完整口径见 `docs/data-metrics.md`，此处为强制约束：

1. **只读**：对客户端数据目录只做读取，绝不写入、移动或删除。
2. **最小采集**：只提取统计必需元数据（时间、模型、token 计数、会话标识等），不持久化消息正文。
3. **可溯源分级**：每个指标必须标注来源等级——`measured`（客户端真实记录）、`derived`（由真实字段直接换算）、
   `estimated`（推算，例如按字符数估算 token）。估算不得与实测混在同一数值里。
4. **估算可见**：UI 上估算数据必须有明确标注（图例、tooltip、样式区分），见 `docs/ui-guidelines.md`。
5. **本地无出口**：默认无任何网络请求；导出、分享等能力只能由用户显式触发。
6. **幂等**：重复扫描不得重复计数，唯一键与去重规则写在 `docs/data-metrics.md`。
7. **口径变更留痕**：修改统计口径必须更新 `docs/data-metrics.md` 并记录版本、日期与影响。
8. **可清除**：应用产生的本地数据必须提供一键清除方式，且不影响客户端原始数据。

## 7. 测试要求

现状：Rust 侧已有领域、采集器与命令层单元测试（`pnpm rust:test`），前端尚未配置测试框架（无 Vitest）。引入前端框架时先在 `docs/architecture.md` 登记并补脚本。

口径类改动（Token 语义、去重键、累计值、迁移）必须带 `#[cfg(test)]` 单测，且只能使用合成数据。

必须覆盖的内容：

- **测试组织**：小模块测试跟随实现；较大的模块按行为分成 `#[cfg(test)] mod tests` 子模块，
  共享合成 fixture 构造器仅在测试时编译；重构必须保留原有用例与断言。
- **Rust**：解析器、归一化、聚合、去重等纯函数使用 `#[cfg(test)]` 单测；测试数据放去敏 fixture，不得使用真实会话内容。
- **前端**：`src/lib/` 纯函数与关键 Hook 的单测（引入 Vitest + Testing Library 后）。
- **估算公式**：每个估算规则必须有单测，覆盖边界（字段缺失、0 值、跨天、超大值）。
- **回归**：客户端格式变化导致解析失败时，新增对应 fixture 用例，保证"忽略未知字段"的行为不回退。

不做的测试：依赖真实客户端数据目录、依赖网络、依赖当前时间的自动化测试（改用 fixture + 注入时间）。

## 8. 开发完成后的验收流程

1. 依赖变化时执行 `pnpm install`，确认 `pnpm-lock.yaml` 同步更新。
2. 运行 `pnpm check`（lint + typecheck + format 校验），必须全绿。
3. 改动涉及 Rust 时，运行 `pnpm rust:check`（fmt + clippy + test），必须全绿。
4. 涉及界面或交互时，运行 `pnpm tauri dev` 手动验证：窗口可启动、控制台无报错、关键路径可完成、空/加载/错误三态正常。
5. 涉及统计数据时，抽样与客户端自身统计对账，并在提交说明中写明口径、差异与原因。
6. 涉及新依赖、新权限、联网能力、目录结构或统计口径时，同步更新 `AGENTS.md` / `docs/` 对应文档。
7. 自检清单：
   - [ ] 没有引入新的 UI 库、状态管理库或未经登记的架构组件
   - [ ] 没有提交 vendor 代码（`src/components/ui/`、`use-mobile.ts`）的手改
   - [ ] 没有把会话正文写入日志、缓存或仓库
   - [ ] 估算数据在 UI 上有明确标注
   - [ ] `pnpm check` 通过，Rust 侧改动额外通过 `pnpm rust:check`（fmt + clippy + test）

## 9. 规范维护

- 单一事实来源：命令以 `package.json` 为准，统计口径以 `docs/data-metrics.md` 为准，视觉规范以 `docs/ui-guidelines.md` 为准，
  结构与边界以 `docs/architecture.md` 为准。
- 修改任一规范文件后，检查其余规范文件是否出现表述冲突（术语、路径、命令、分级命名）。
- 规则取舍优先级：本地优先 > 最小权限 > 可验证（有自动化检查）> 实现便利。
- 能由工具检查的规则优先写进 `eslint.config.js` / `.prettierrc.json` / `tsconfig.json` / `Cargo.toml`，文档只解释"为什么"。
