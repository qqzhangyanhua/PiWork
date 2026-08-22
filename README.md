# CoDo

CoDo is a local-first desktop assistant that takes on useful work across professions. A Work keeps its goal, local workspace, Runs, and event history in CoDo-owned SQLite storage so collaboration can continue across sessions. Execution engines remain hidden and replaceable behind CoDo's own engine interface.

This repository is currently the foundation vertical slice. It uses a deterministic fake engine to exercise streaming, tool events, completion, multi-Run history, persistence, and interruption recovery. A real pi binary, model provider, credentials flow, and permission bridge are **not** integrated or bundled at this stage.

Windows 10/11 x64 is the first supported desktop target.

## Prerequisites

- Windows 10 or 11 x64
- Node.js 20.19+ or 22.12+
- pnpm 9.12.0 (Corepack is recommended)
- Rust stable with the MSVC toolchain
- Microsoft C++ Build Tools and WebView2 development prerequisites required by Tauri 2

## Development

Install dependencies:

```powershell
pnpm install
```

Run the desktop app with hot reload:

```powershell
pnpm tauri dev
```

Run the test and static-analysis gates:

```powershell
pnpm test
pnpm typecheck
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Build the frontend and a debug Windows NSIS installer:

```powershell
pnpm build
pnpm tauri build --debug --bundles nsis
```

## Architecture

- [Product and architecture design (English)](docs/superpowers/specs/2026-07-28-piwork-design.md)
- [产品与架构设计（中文）](docs/superpowers/specs/2026-07-28-piwork-design.zh-CN.md)

Third-party attribution and current bundling status are recorded in [NOTICE](NOTICE).
