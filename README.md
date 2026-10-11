# TokLens

TokLens is a desktop application scaffolded with **Tauri 2**, **React + TypeScript + Vite**, and **Rust**.

This repository currently contains only the project scaffolding. Business logic, UI design, database access,
and additional UI component libraries are intentionally not included yet.

## Tech Stack

| Layer          | Technology                          |
| -------------- | ----------------------------------- |
| Desktop shell  | Tauri 2                             |
| Frontend       | React 19 + TypeScript + Vite        |
| Backend        | Rust (Tauri commands)               |
| Package manager| pnpm                                |

## Requirements

- [Node.js](https://nodejs.org/) 18+ and [pnpm](https://pnpm.io/) 9+
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain, `rustup` recommended)
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

| Command            | Description                                                |
| ------------------ | ---------------------------------------------------------- |
| `pnpm install`     | Install frontend dependencies                              |
| `pnpm tauri dev`   | Run the desktop app in development mode                    |
| `pnpm tauri build` | Build the production desktop bundle                        |
| `pnpm dev`         | Run the frontend alone in a browser (no Rust backend)       |
| `pnpm build`       | Type-check and build the frontend assets                    |
| `pnpm preview`     | Preview the built frontend assets                           |

> `pnpm dev` only serves the web frontend. Tauri APIs such as `invoke` are unavailable in that mode, so the
> scaffold shows a "backend not connected" status when opened in a plain browser.

## Project Structure

```
TokLens/
├─ index.html               # Vite HTML entry
├─ package.json             # Frontend scripts and dependencies
├─ vite.config.ts           # Vite configuration (Tauri-aware)
├─ tsconfig.json            # TypeScript configuration
├─ public/                  # Static assets served as-is
├─ src/                     # React frontend source
│  ├─ main.tsx              # React entry point
│  ├─ App.tsx               # Root component (backend connectivity check)
│  └─ App.css               # Base styles
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