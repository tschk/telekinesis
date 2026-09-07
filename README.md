# telekinesis (tk)

[![license](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/telekinesis.svg)](https://crates.io/crates/telekinesis)

AI coding agent CLI + TUI, powered by [rotary](https://github.com/tschk/rotary)
(rx4) and [crepuscularity-tui](https://github.com/tschk/crepuscularity).

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/tschk/telekinesis/main/install.sh | bash
```

```bash
cargo install telekinesis
cargo install telekinesis --features full
```

## Usage

```bash
tk login grok
tk
tk exec "summarize this repo"
printf '%s\n' "review the diff" | tk --no-yolo
```

Default non-TTY / `tk exec` is yolo (`AlwaysAllow`); `--no-yolo` denies Ask-class tools.

TUI, CLI, and GUI forward rotary host events (`RetryReason`, `ProcessStdin`, `RequestPermissions`, `PatchHunk`) the same way as tool execution and approvals.

Streaming TUI, slash commands, OAuth login, slim default binary. Details:
[docs](docs/README.md). Evolutionary loop (NVIDIA AVO): [docs/AVO.md](docs/AVO.md).

## UI surfaces

- **TUI** (`ui/tui`) — primary
- **GPUI companion** (`ui/gui`) — experimental native window
- **ADE desktop** (`apps/desktop`) — experimental Tauri ADE shell (Local | Cloud); scaffold only
- **Web** (`apps/web`) — experimental control plane (shared `--tk-*` tokens)

Cloud backend remains private `tschk/tk-cloud`. Archived sibling `tschk/tk-desktop` is not the desktop home.

## License

MPL-2.0
