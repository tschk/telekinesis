# PGO / PGSO

Release `tk` binaries abort on panic and, when built with
[`scripts/pgo/pgo.sh`](../scripts/pgo/pgo.sh), are profile-guided.

## Panic abort

`ui/tui` release profile:

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

That drops Rust landing pads (`.eh_frame` / `.gcc_except_table`). On Linux
x86_64 that cut the v0.6.19 binary from 8.8 MiB to 7.8 MiB before PGO.
Debug/test builds still unwind. There is no `catch_unwind` in the host.

## Lane

There is no Rust equivalent of fx's Zig PGSO driver. This lane is the rustc
PGO workflow plus LLVM's `--pgso` pass (the same profile-guided size
optimization fx uses on Apple Silicon):

1. `cargo build --release --target <triple>` with `-C profile-generate`
2. Run [`scripts/pgo/train.sh`](../scripts/pgo/train.sh) on the instrumented
   `tk` (help/version/error paths; no network, no TUI)
3. `llvm-profdata merge`
4. Rebuild with `-C profile-use` and `-C llvm-args=--pgso --force-pgso`

References:

- rustc PGO: <https://doc.rust-lang.org/rustc/profile-guided-optimization.html>
- cargo-pgo (same flags; CI shape): <https://github.com/Kobzol/cargo-pgo>
- fx PGSO (Zig, macOS arm64 only): <https://github.com/vercel-labs/fx/blob/main/scripts/pgso/README.md>

`cargo install telekinesis` does not run this lane. GitHub Release artifacts
do (`scripts/pgo/pgo.sh` in `.github/workflows/release.yml`).

## Local

```bash
rustup component add llvm-tools-preview
bash scripts/pgo/pgo.sh                          # host triple
```

`--target` must be the host triple (the GitHub Release matrix is native
per runner: macOS Intel/ARM and Ubuntu x86_64/ARM). Cross-compiled
instrumented binaries cannot be trained on the builder.

Optimized binary: `ui/tui/target/<triple>/release/tk`.
Profiles: `ui/tui/target/pgo/<triple>/` (gitignored via `ui/tui/target/`).

`--skip-pgso` still applies PGO (speed-oriented) without LLVM size opts.

Training is CLI-only. Agent/TUI hot paths stay cold in the profile, which is
what we want for size: LLVM `--pgso` / `--force-pgso` /
`--pgso-cold-code-only-for-instr-pgo` compiles unprofiled functions for size
and keeps the trained CLI paths fast.

On this Linux x86_64 host, PGO+PGSO then took the abort-on-panic binary from
7.8 MiB to 6.8 MiB. Apple Silicon should still be smaller; that is the number
fx advertises after its own PGSO lane.
