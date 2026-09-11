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

Build from source:

```bash
cd ui/tui && cargo build --release
# binary: ui/tui/target/release/tk
```

## License

MPL-2.0
