# Product audit and opportunities

This audit records source observations from November 2026. Items marked as
proposals are not promises of implemented behavior. Verify the cloud protocol
and exercise each flow before choosing an implementation.

The TUI temporarily pins an unmerged rotary revision for approval-authorizer
retention. `ui/tui/Cargo.toml` records the upstream merge and authority fixes
required before repinning. Mobile rendering and live Claude and Copilot calls
remain unverified; type checks and mock-server tests do not replace them.

## Reliability first

- Interactive resume now restores the displayed conversation, agent messages,
  and todo state together, and rejects resume while the agent is busy. The
  shutdown path in `ui/tui/src/tui.rs` still discards final save errors. Report
  those errors instead of silently losing buffered entries.
- The apps disable remote steering and approval responses. The inspected cloud
  server records these events but has no agent consumer or correlated result.
  Add an idempotent command protocol and an agent consumer before enabling
  controls. An event-log echo alone does not prove that the agent acted.
- Desktop reports workspace-list failures as unavailable instead of selecting
  mock workspace IDs for real cloud actions.
- Web, desktop, and mobile scope approval and diff state to the workspace.
  Mobile processes every event in a batch. The stream client reconnects after
  the server's 200-event replay page instead of skipping older events.

## More capable workflows

- `/sessions [query]` searches prompt text, model, and filename among the 20
  most recent sessions. Add date filtering, pagination, and explicit directory
  errors. Branching from a chosen point can use the existing pi session support.
- Rank memory matches by exact phrase and query coverage before applying the
  result limit. `ui/tui/src/memory.rs` currently stops after eight substring
  hits, before inspecting all notes.
- Web diff review has file and hunk navigation and change counts. Add approval
  context and bring the review view to desktop and mobile, which render raw text.
- Mobile users can select an existing workspace. Build real status, steering,
  diff, and approval views; the current tab labels are disabled.
- Connect desktop local sessions to the shared host runtime. Its local session,
  terminal, and browser panes are placeholders; starting a CLI process alone
  does not provide session discovery or reconnectable interaction.

## Boundaries

The TUI and apps are hosts, not separate agent engines. New harness features
belong in rotary and should be exposed through the shared host boundary. Cloud
delivery guarantees require a matching server protocol, not client-side status
labels alone. The observations above are source audit findings, not a claim
that every scenario has been reproduced against a running cloud deployment.
