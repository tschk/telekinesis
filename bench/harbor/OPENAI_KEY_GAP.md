# OPENAI_KEY_GAP — not a gap (2026-09-13)

Max policy: **never use OpenAI API billing** for this Harbor bench.

Codex baseline = **ChatGPT/Codex subscription OAuth** (`~/.codex/auth.json`).

- Harbor `--agent codex` if OAuth works.
- Else custom installed agent wrapping `codex exec` (OAuth) — `agents/codex_oauth_harbor_agent.py`.
- Do **not** report “Codex needs OPENAI_API_KEY” as the plan or blocker.
