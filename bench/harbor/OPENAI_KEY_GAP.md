# OPENAI_KEY_GAP — resolved by policy (2026-09-13)

Max: **do not buy/use OpenAI API credits.**

Codex baseline uses **ChatGPT/Codex subscription OAuth** already on cp.local
(`~/.codex/auth.json` with `tokens`, `OPENAI_API_KEY: null`).

## Phase A path

1. Try Harbor `--agent codex` with logged-in Codex CLI (OAuth).
2. If Harbor still requires `OPENAI_API_KEY`, use custom installed agent
   `CodexOauthHarborAgent` wrapping `codex exec` (same OAuth session).
3. Do **not** treat missing API key as a blocker that needs a paid key.

## Interim

`grok-build` / `grok-4.6` Harbor smoke may run for tooling validation only —
**not** the Codex baseline.
