# TokLens

TokLens is a local-first desktop app that summarizes how you use AI coding clients such as
**Codex**, **Claude Code**, and **OpenCode** on this machine (sessions, requests, tokens, models, cost).

This repository currently contains only the project scaffolding. Business logic, data collection,
UI screens, and additional UI component libraries are intentionally not included yet.

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

> `pnpm dev` only serves the web frontend. Tauri APIs such as `invoke` are unavailable in that mode, so the
> scaffold shows a "backend not connected" status when opened in a plain browser.

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
│  ├─ App.tsx               # Root component (backend connectivity check)
│  ├─ App.css               # Base styles
│  ├─ index.css             # Tailwind v4 entry and design tokens
│  ├─ components/ui/        # shadcn/ui generated components (vendor code)
│  ├─ hooks/                # Reusable hooks (use-mobile.ts from shadcn)
│  └─ lib/                  # Framework-free helpers
└─ src-tauri/               # Rust / Tauri backend
   ├─ Cargo.toml            # Rust crate manifest
   ├─ build.rs              # Tauri build script
   ├─ tauri.conf.json       # Tauri application configuration
   ├─ capabilities/         # Tauri 2 permission capabilities
   ├─ icons/                # Application icons
   └─ src/
      ├─ main.rs            # Binary entry point
      └─ lib.rs             # Application setup and Tauri commands
```

## Notes

- The application identifier is `com.toklens.app`.
- The only registered Tauri command is `app_info`, a placeholder used to verify that the frontend can reach the
  Rust backend.
- Statistics must distinguish measured data from estimated data, and local session data is private by default:
  see [`docs/data-metrics.md`](./docs/data-metrics.md).
