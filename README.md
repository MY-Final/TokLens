# TokLens

TokLens is a local-first desktop app that summarizes how you use AI coding clients such as
**Codex**, **Claude Code**, and **OpenCode** on this machine (sessions, requests, tokens, models, cost).

What is implemented today:

- a unified usage data model with local SQLite storage (`src-tauri/src/usage/`),
- a read-only Codex collector for `%USERPROFILE%\.codex\sessions` (`src-tauri/src/collectors/codex/`),
- the Tauri command layer (`src-tauri/src/commands/`) powering the desktop UI,
- the overview UI: overview / analytics / sessions / data sources pages (`src/pages/`).

Claude Code and OpenCode collectors are not implemented yet (the UI marks them as unsupported) and no cost
figures are shown, because there is no reliable cost source on this machine.

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
      ├─ collectors/        # Client collectors (Codex implemented)
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
  `get_data_source_status`, `sync_codex` (plus `app_info`). Queries read the local database;
  `sync_codex` reads client files and updates only TokLens's own database.
- Usage accuracy can be reconciled against Codex's own counters with
  `cargo run --manifest-path src-tauri/Cargo.toml --example codex_usage_report`.
- Statistics must distinguish measured data from estimated data, and local session data is private by default:
  see [`docs/data-metrics.md`](./docs/data-metrics.md).
