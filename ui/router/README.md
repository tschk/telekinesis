# telekinesis-router

Small crate shared by the TUI and GPUI companion.

- Provider specs and aliases
- API key resolution (`env` first, then OpenCode `auth.json` for Cline-pass)
- Model slug normalize
- Local usage totals (`~/.telekinesis/usage.json`) — request/token activity, not invoices

No rx4, no TUI, no host loop.
