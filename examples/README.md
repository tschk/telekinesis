# telekinesis examples

Runnable `tk` flows for the main CLI surfaces. Everything here targets the
default binary (`tk`); optional features are called out inline.

## Install

```bash
# macOS / Linux installer (installs to ~/.local/bin)
curl -fsSL https://raw.githubusercontent.com/tschk/telekinesis/main/install.sh | bash

# or, any platform with a Rust toolchain
cargo install telekinesis
cargo install telekinesis --features full   # MCP + search + computer-use + skills + graph-memory
```

## 1. Interactive TUI

```bash
tk login grok     # openai, claude, gemini, copilot, kimi, antigravity
tk
tk -c             # continue the newest session for this project
```

Slash commands inside the TUI: `/model`, `/scope`, `/config`, `/sessions`,
`/resume <n>`, `/cost`, `/usage`, `/clear`, `/help`, `/quit`.

## 2. Headless exec

```bash
# final assistant text on stdout
tk exec "summarize this repo"

# machine-readable turn
tk exec --json --cwd /workspace "list the rust crates"

# override provider / model / effort
tk exec --provider xai --model grok-4.5 --effort high "explain src/main.rs"

# prompt from stdin
printf '%s\n' "review the diff" | tk exec -

# non-TTY stdin run; --no-yolo denies Ask-class tools (default is yolo)
printf '%s\n' "review the diff" | tk --no-yolo
```

## 3. Sessions

`tk` persists pi-compatible JSONL v3 sessions per project.

```bash
tk -c                 # continue newest
# inside the TUI:
#   /sessions         list newest first
#   /resume <n>       switch to session n
#   /clear            clear messages + reset cost
```

## 4. API keys (non-OAuth)

```bash
XAI_API_KEY=... tk
OPENAI_API_KEY=... tk exec "say hi"
# also: ANTHROPIC_API_KEY, GOOGLE_API_KEY, OPENCODE_API_KEY,
#       OPENROUTER_API_KEY, CLINE_API_KEY
```

## 5. MCP servers (`--features mcp` or `full`)

Copy [mcp.json](mcp.json) to `~/.telekinesis/mcp.json`, then:

```bash
tk                                    # TUI discovers servers on startup
tk exec --mcp "list the MCP tools"
# inside the TUI: /mcp
```

## 6. ACP host

`tk acp` is a JSON-RPC 2.0 server on stdin/stdout: one request per line, one
response per line. See [acp-requests.jsonl](acp-requests.jsonl) for a handshake.

```bash
cargo build --release          # cd ui/tui first for a source build
tk acp < examples/acp-requests.jsonl
```

Methods: `initialize`, `session/new`, `session/list`, `session/prompt`,
`session/cancel`. `session/prompt` takes
`{"sessionId":"<id from session/new>","prompt":"..."}`. Flags: `--cwd <dir>`,
`--provider <id>`, `--model <name>`, `--mcp`, `--no-yolo`.

## 7. AVO loop (scripts only)

Optional NVIDIA Agentic Variation Operator loop; no `tk avo` subcommand.

```bash
scripts/avo/tick.sh --init speedup \
  --goal  "maximize a higher-is-better objective" \
  --score "$PWD/scripts/adapters/score-example.sh" \
  --agent "$PWD/scripts/adapters/agent-tk.sh"
scripts/avo/run.sh --max-ticks 8
scripts/avo/status.sh
```

See [docs/AVO.md](../docs/AVO.md) for the scorer contract and environment
variables.

## Build and test

```bash
cd ui/tui
cargo build
cargo run
cargo test
cargo clippy
```

## Files

```
examples/
├── README.md               # this file
├── acp-requests.jsonl      # ACP initialize + session/new handshake
└── mcp.json                # sample ~/.telekinesis/mcp.json
```
