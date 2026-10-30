# telekinesis (tk)

[![license](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/telekinesis.svg)](https://crates.io/crates/telekinesis)

**AI coding agent in your terminal**

telekinesis is the CLI + TUI host for the [rotary](https://github.com/tschk/rotary)
(rx4) agent harness. rx4 runs in-process; telekinesis adds the terminal UX,
OAuth login, pi-compatible JSONL v3 sessions, and an ACP JSON-RPC server over
stdio.

## Quick Start

### 1. Install

macOS and Linux (installs `tk` to `~/.local/bin`):

```bash
curl -fsSL https://raw.githubusercontent.com/tschk/telekinesis/main/install.sh | bash
```

Any platform with a Rust toolchain:

```bash
cargo install telekinesis
```

### 2. Log in

```bash
tk login grok
```

Also: `openai`, `claude`, `gemini`, `copilot`, `kimi`, `antigravity`.

### 3. Run

```bash
tk                                # interactive TUI
tk exec "summarize this repo"     # one headless turn; final text on stdout
```

## Usage

```bash
printf '%s\n' "review the diff" | tk exec -
printf '%s\n' "review the diff" | tk --no-yolo
tk -c                             # continue newest session for this project
tk exec --json --cwd /workspace "list the rust crates"
tk exec --model grok-4.5 "summarize this repo"
tk acp                            # ACP JSON-RPC server on stdin/stdout
```

Default non-TTY / `tk exec` is yolo (`AlwaysAllow`); `--no-yolo` denies
Ask-class tools. `tk exec` flags: `--json`, `--cwd <dir>`, `--provider <id>`,
`--model <name>`, `--effort|--thinking <low|medium|high|xhigh>`, `--mcp`,
`--prewalk`, `--smol-model <name>`, `--investigate-model <name>`.

`tk acp` speaks ACP JSON-RPC 2.0 (one request per stdin line, one response per
stdout line) with `initialize`, `session/new`, `session/list`, `session/prompt`,
and `session/cancel`. It accepts the same `--cwd` / `--provider` / `--model` /
`--mcp` / `--no-yolo` flags.

## Features

Default `tk` is the lightest useful coding CLI: `pi-compat` (JSONL v3 sessions +
embed SDK) + `acp` (enabled by default) over rx4 `providers` / `builtin-tools`.
Everything else is opt-in:

| feature | what it adds |
|---|---|
| `mcp` | rx4 MCP + `~/.telekinesis/mcp.json` discover/register |
| `search` | darash `web_search` tool |
| `computer-use` | `cu_*` tools (Praefectus) |
| `skills` | rx4 skill engine |
| `graph-memory` | rx4 graph memory / dream |
| `full` | all of the above |

```bash
cargo install telekinesis --features full
```

TUI, CLI, and GUI forward rotary host events (`RetryReason`, `ProcessStdin`,
`ProcessStart`, `ProcessEnd`, `RequestPermissions`, `PatchHunk`, `Recovery`,
`ToolSpill`) the same way as tool execution and approvals.

## Examples

See [examples/README.md](examples/README.md) for TUI, headless exec, sessions,
MCP, and ACP flows.

## Documentation

- [Docs index](docs/README.md)
- [Usage](docs/USAGE.md) — install features, CLI, OAuth, TUI keys and slash commands.
- [Architecture](docs/ARCHITECTURE.md) — product layers and the in-process event path.
- [Rotary integration](docs/ROTARY.md) — host/engine boundary and the rx4 API.
- [AVO loop](docs/AVO.md) — NVIDIA agentic variation loop via `scripts/avo` + `tk exec`.
- [PGO / PGSO](docs/PGO.md) — `panic = "abort"` and the rustc PGO + LLVM PGSO release lane.

Build from source:

```bash
cd ui/tui && cargo build --release
# binary: ui/tui/target/release/tk
```

## UI surfaces

- **TUI** (`ui/tui`) — primary
- **GPUI companion** (`ui/gui`) — experimental native window
- **ADE desktop** (`apps/desktop`) — experimental Tauri ADE shell (Local | Cloud); scaffold only
- **Web** (`apps/web`) — experimental control plane (shared `--tk-*` tokens)

Cloud backend remains private `tschk/tk-cloud`. Archived sibling `tschk/tk-desktop` is not the desktop home.

Mobile companion (Expo M0 scaffold): [`apps/mobile`](apps/mobile).

## License

MPL-2.0

## Local chat and worker preview

The native app has one pinned Coordinator chat with a stable session and an
explicitly selected workspace. Project navigation does not move that execution
workspace. Other threads are grouped by their stored canonical project roots.
Normal Direct tools and existing histories remain available by default. The
Coordinator label does not introduce a separate host agent loop.

Experimental worker orchestration requires `coordinator-preview`, which is
excluded from both default and `full` features. Enable it explicitly for an
isolated preview store; normal histories are not migrated into worker ownership.

Build and stage a matching CLI or macOS GPUI app from the same dependency source:

```bash
python3 scripts/stage-local-host.py cli --out target/local-cli
python3 scripts/stage-local-host.py gpui --out target/local-gpui
```

Staging uses locked dependencies and allows Cargo to fetch missing packages. Add
`--offline` only when all dependencies are already cached.

Use `--native-layout --target-dir target/local-host` to reuse a native development
build. The helper is a separate executable beside the host. `cargo install --path
ui/tui` installs both binaries; `cargo run` still selects `tk`. Missing or mismatched
helpers fail closed. Tauri stages `binaries/rx4-script-worker-<target-triple>` as an
external binary before bundling. Local staging records source identity and helper
checksums, but does not perform distribution signing or notarization. Sign the
helper as nested executable code before signing/notarizing a macOS app, and verify
the real hardened-runtime package and LuaJIT entitlement requirements separately.

Local GPUI staging opens an isolated state directory beside the artifact. Only
the explicit worker preview refuses legacy Direct histories, because those stores
cannot prove background-process ownership. The default host accepts normal
existing histories and refuses worker-preview stores rather than discarding their
unresolved ownership.

The explicit worker preview uses read tools in Direct and Coordinator roots. A single delegate owns workspace writes until the engine proves settlement;
other admissions return busy. Durable admission markers survive host loss and are
cleared only after proven settlement; dropping a lock or task is insufficient.
Generic root process/file-write tools are hidden
until an engine per-run ownership API can preserve successful background work and
prove cleanup without sealing the registry. This restriction must not silently
replace the existing Direct product. Worker history, parent links, results and
accepted steering persist. Accepted steering does not mean provider consumption.
Interrupted or unknown cleanup does not replay work or release workspace ownership.
The desktop Local host and Direct/Coordinator workflow are independent choices;
this preview adds no cloud provisioning or unauthenticated mobile transport.
