# TokLens

TokLens is a local-first desktop app that summarizes how you use AI coding clients such as
**Codex**, **Claude Code**, **OpenCode**, **DSH (deepseek-hermes)**, **Kimi Code**, **MiMoCode**, **Pi Desktop**, **WorkBuddy**, **GitHub Copilot**, **Gemini CLI**, **Grok CLI**, and **MCode** on this machine (sessions, requests, tokens, models, cost).

What is implemented today:

- a unified usage data model with local SQLite storage (`src-tauri/src/usage/`),
- a read-only Codex collector for `%USERPROFILE%\.codex\sessions` (`src-tauri/src/collectors/codex/`),
- read-only Claude Code JSONL, OpenCode SQLite, DSH JSONL/Zstandard, Kimi Code state/wire, MiMoCode SQLite, Pi Desktop JSONL, WorkBuddy JSONL, GitHub Copilot SQLite, Gemini CLI journals, Grok CLI usage snapshots, and MCode SQLite collectors (`src-tauri/src/collectors/`),
- the Tauri command layer (`src-tauri/src/commands/`) powering the desktop UI,
- the overview UI: overview / analytics / sessions / data sources pages (`src/pages/`).

Kimi Code reads `~/.kimi-code/sessions` (or `$KIMI_CODE_HOME/sessions`), counts incremental
`usage.record` events from agent wire logs, and keeps sessions without usage visible as unknown.

MiMoCode reads `~/.local/share/mimocode/mimocode.db` (`MIMOCODE_DB` / `XDG_DATA_HOME` supported),
including live WAL updates. Imported Claude/external messages are excluded to avoid duplicate accounting; native continuation requests remain counted.
Pi Desktop reads `~/.pi-desktop/sessions` (`PI_DESKTOP_SESSIONS_DIR` supported) and deduplicates physical
request operations; client-estimated cost is not imported as measured cost.

All twelve clients participate in startup/manual sync, filters, trends, and rankings. OpenCode's old JSON
storage and unverified DSH persistence formats are not supported. No unified cost figures are shown;
client-reported costs are preserved as metadata without treating them as a shared pricing source.

Development rules live in [`AGENTS.md`](./AGENTS.md) with details under [`docs/`](./docs/README.md):

| Document                                           | Scope                                      |
| -------------------------------------------------- | ------------------------------------------ |
| [`AGENTS.md`](./AGENTS.md)                         | Entry-point rules: stack, layout, workflow |
| [`docs/architecture.md`](./docs/architecture.md)   | Layers, boundaries, adapters, decisions    |
| [`docs/ui-guidelines.md`](./docs/ui-guidelines.md) | Visual language, components, charts        |
| [`docs/data-metrics.md`](./docs/data-metrics.md)   | Metric definitions and privacy rules       |

## Tech Stack

| Layer           | Technology                               |
| --------------- | ---------------------------------------- |
| Desktop shell   | Tauri 2                                  |
| Frontend        | React 19 + TypeScript + Vite             |
| Styling         | Tailwind CSS v4 + shadcn/ui (new-york)   |
| Backend         | Rust (Tauri commands)                    |
| Package manager | pnpm                                     |
| Quality         | ESLint + Prettier + tsc + rustfmt/clippy |

## Requirements

- [Node.js](https://nodejs.org/) 20.19+ or 22.12+ (22 LTS recommended) and [pnpm](https://pnpm.io/) 10+
- [Rust](https://www.rust-lang.org/tools/install) with the edition 2024 toolchain (stable, 1.85+)
- Platform build prerequisites for Tauri 2
  - **Windows**: Microsoft C++ Build Tools (MSVC + Windows SDK) and WebView2 Runtime
  - **macOS**: Xcode Command Line Tools
  - **Linux**: `webkit2gtk`, `libgtk-3-dev`, and related packages

## Getting Started

Install frontend dependencies:

```bash
pnpm install
```

Start the desktop app in development mode (Vite dev server + Rust backend with hot reload):

```bash
pnpm tauri dev
```

The first run compiles the Rust dependencies and may take several minutes.

## Common Commands

| Command            | Description                                                    |
| ------------------ | -------------------------------------------------------------- |
| `pnpm install`     | Install frontend dependencies                                  |
| `pnpm tauri dev`   | Run the desktop app in development mode                        |
| `pnpm tauri build` | Build the production desktop bundle                            |
| `pnpm dev`         | Run the frontend alone in a browser (no Rust backend)          |
| `pnpm build`       | Type-check and build the frontend assets                       |
| `pnpm preview`     | Preview the built frontend assets                              |
| `pnpm lint`        | Run ESLint (`pnpm lint:fix` to auto-fix)                       |
| `pnpm typecheck`   | Type-check `src` and `vite.config.ts`                          |
| `pnpm format`      | Format the repository with Prettier (`format:check` to verify) |
| `pnpm check`       | lint + typecheck + format:check (run before committing)        |
| `pnpm rust:fmt`    | Format Rust code (`rust:fmt:check` to verify)                  |
| `pnpm rust:clippy` | Lint Rust code with `clippy -D warnings`                       |
| `pnpm rust:test`   | Run the Rust unit tests (`rust:check` also runs fmt + clippy)  |

> `pnpm dev` only serves the web frontend. Tauri APIs such as `invoke` are unavailable in that mode, so the
> app shows a local-backend-unavailable message when opened in a plain browser.

## Project Structure

```
TokLens/
├─ AGENTS.md                # Development rules (entry point)
├─ docs/                    # Architecture, UI, and data-metrics standards
├─ index.html               # Vite HTML entry
├─ package.json             # Frontend scripts and dependencies
├─ eslint.config.js         # ESLint flat config
├─ .prettierrc.json         # Prettier config (with Tailwind class sorting)
├─ .gitattributes           # LF normalization for cross-platform consistency
├─ vite.config.ts           # Vite configuration (Tauri-aware)
├─ tsconfig.json            # TypeScript configuration
├─ public/                  # Static assets served as-is
├─ src/                     # React frontend source
│  ├─ main.tsx              # React entry point
│  ├─ App.tsx               # App shell: sidebar navigation + pages
│  ├─ pages/                # Overview / analytics / sessions / data sources
│  ├─ components/           # Business components (charts, tables, cards)
│  ├─ index.css             # Tailwind v4 entry and design tokens
│  ├─ components/ui/        # shadcn/ui generated components (vendor code)
│  ├─ hooks/                # Reusable hooks (use-mobile.ts from shadcn)
│  └─ lib/                  # Pure helpers & DTOs; api.ts is the Tauri bridge
└─ src-tauri/               # Rust / Tauri backend
   ├─ Cargo.toml            # Rust crate manifest
   ├─ build.rs              # Tauri build script
   ├─ tauri.conf.json       # Tauri application configuration
   ├─ capabilities/         # Tauri 2 permission capabilities
   ├─ icons/                # Application icons
   └─ src/
      ├─ main.rs            # Binary entry point
      ├─ lib.rs             # Tauri setup: state, initial sync, command registry
      ├─ commands/          # State, errors, DTOs, queries & background sync
      ├─ collectors/        # Read-only Codex / Claude Code / OpenCode / DSH / Kimi Code / MiMoCode / Pi Desktop / WorkBuddy / GitHub Copilot / Gemini CLI / Grok CLI / MCode collectors
      └─ usage/
         ├─ client.rs        # Stable client identity
         ├─ model.rs         # Unified records and deduplication identifiers
         ├─ time.rs          # UTC timestamp parsing and system clock
         ├─ tokens/          # Counts, semantics registry & normalization
         ├─ normalize/       # Per-client record normalization
         ├─ store/           # Connections, migrations, writes & imports
         └─ analytics/       # Aggregates, rankings & metadata queries
```

## Notes

- The application identifier is `com.toklens.app`.
- The local usage database lives in the app data directory: `%APPDATA%\com.toklens.app\toklens-usage.sqlite3`.
  Client data directories are only ever read, never written.
- Commands: `get_overview_stats`, `get_daily_token_trend`, `get_client_usage`, `get_rankings`, `get_sessions`,
  `get_data_source_status`, `sync_usage`, `sync_codex` (plus `app_info`). Queries read the local database;
  `sync_usage` updates only TokLens's own database. `sync_codex` retains Codex-only compatibility.
- Usage accuracy can be reconciled against Codex's own counters with
  `cargo run --manifest-path src-tauri/Cargo.toml --example codex_usage_report`.
  For the other three clients, use `--example client_usage_report`; it imports into memory and prints aggregate metadata only.
- Statistics must distinguish measured data from estimated data, and local session data is private by default:
  see [`docs/data-metrics.md`](./docs/data-metrics.md).

WorkBuddy reads `~/.workbuddy/projects` (or `WORKBUDDY_CONFIG_DIR/projects`) and counts only native output items with usage. Provider usage and transcript usage are mirrors, not additive. Existing installations can enable WorkBuddy in Data Sources without changing other client choices.

GitHub Copilot reads `~/.copilot/session-store.db` (or `COPILOT_HOME/session-store.db`) and SDK metadata in `session-state/*/events.jsonl`. On Windows it also links VS Code `%APPDATA%/Code/agentSessionData/*/session.db` to the SDK sessions. Whole-turn Token totals are kept separate from partial API-call usage; mirrors are counted once. SQLite/WAL snapshots stay in memory without opening client databases for writing. Missing usage, reasoning, cache writes and unverified nonzero-cache totals stay unknown. Context counters, premium requests and costs are not converted to tokens. Enable GitHub Copilot in Data Sources for existing installations.

Gemini CLI reads metadata from `~/.gemini/tmp/*/chats/**/*.jsonl`, replays replacement/rewind operations before counting final response IDs, and excludes Antigravity. `GEMINI_CLI_HOME` replaces the user home before `.gemini` is appended. Cached input is a subset, thoughts are separate, and tool usage has no verified canonical bucket. Legacy `.json` and sandbox-exec paths are not collected.

Grok CLI reads authoritative `usage.json` and metadata-only `summary.json` under `GROK_HOME/sessions` (default `~/.grok/sessions`). Each detail is a whole turn, not an individual model call; request counts and averages are explicitly labeled, with time assigned to the native turn end. Session and turn totals are never added together. Cache creation remains unverified; costs are ignored.

MCode reads `v2/sqlite/runtime-state.sqlite` under `MINIMAX_DATA_DIR`, then `MAVIS_DATA_DIR`, then `~/.minimax`. Request IDs come from the usage ledger. Independent caches count toward Token usage; absent models stay unknown. Totals are derived only when reasoning is explicitly zero and all four required buckets exist; nonzero or missing reasoning remains unverified. SQLite/WAL snapshots stay in memory and never modify client SHM. The client's fresh/goal summary excludes caches and therefore differs from full Token usage.

New installations enable all twelve collectors. Existing preferences are preserved; enable the three new sources in Data Sources. No new runtime dependencies, Tauri permissions, telemetry, or network access are introduced.
