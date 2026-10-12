# TokLens

TokLens is a local-first desktop app for analyzing usage metadata from AI coding clients. It stores data in a local SQLite database, reads client data without modifying it, and does not use telemetry or network access by default.

## Features

- Overview, trends, model rankings, session metadata, and data source status.
- Read-only collectors for Codex, Claude Code, OpenCode, DSH, Kimi Code, MiMoCode, Pi Desktop, WorkBuddy, GitHub Copilot, Gemini CLI, Grok CLI, and MCode.
- Explicit separation of measured, derived, estimated, unknown, and partial data.
- Local data clearing and configurable source collection.

Model icons are bundled locally and matched by model family. Unknown models keep their original names and use a neutral fallback icon.

## Requirements

- Node.js 20.19+ or 22.12+, pnpm 10+
- Rust stable with edition 2024 support
- Tauri 2 platform prerequisites

## Development

```bash
pnpm install
pnpm tauri dev
```

`pnpm dev` runs only the web frontend and does not provide Tauri commands.

## Commands

| Command            | Purpose                                   |
| ------------------ | ----------------------------------------- |
| `pnpm check`       | ESLint, TypeScript, and formatting checks |
| `pnpm build`       | Build frontend assets                     |
| `pnpm tauri build` | Build the desktop application             |
| `pnpm rust:check`  | Rust formatting, Clippy, and tests        |

## Repository layout

```text
src/                 React frontend and business components
src-tauri/src/       Rust commands, collectors, storage, and analytics
docs/                Architecture, UI, data, and release documentation
scripts/             Release validation and packaging helpers
```

Development rules are in [`AGENTS.md`](./AGENTS.md). Supporting documentation is indexed in [`docs/README.md`](./docs/README.md).

## Releases

Stable tags use the form `vX.Y.Z`. Pushing a matching tag runs the signed release workflow for Windows x64, macOS Intel, and Apple Silicon. See [`docs/releases.md`](./docs/releases.md) for the release process.
