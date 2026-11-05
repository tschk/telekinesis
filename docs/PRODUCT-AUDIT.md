# Product audit and opportunities

This audit records source observations from November 2026. Items marked as
proposals are not promises of implemented behavior. Verify the cloud protocol
and exercise each flow before choosing an implementation.

## Reliability first

- Interactive resume now restores the displayed conversation, agent messages,
  and todo state together, and rejects resume while the agent is busy. The
  shutdown path in `ui/tui/src/tui.rs` still discards final save errors. Report
  those errors instead of silently losing buffered entries.
- `apps/stream/workspaceStream.ts` queues controls in memory, and closing the
  stream drops the queue. The apps clear controls before the server confirms
  them. Define server acknowledgement and retry semantics before presenting
  an approval or steering request as delivered.
- Desktop's `apps/desktop/src/lib/cloudApi.ts` falls back to mock workspaces
  on list failures. Real controls can use the selected workspace ID. Separate
  disconnected and demo states so mock data is never mistaken for cloud data.
- Web and mobile keep approval and diff state across stream changes. Scope
  these to the workspace and show its identity with every approval. Mobile's
  `FutureTabs.tsx` examines only the last event in each received batch.

## More capable workflows

- Make sessions searchable by prompt, model, and date, with pagination and
  explicit directory errors. `session_files` currently returns at most 20.
  A useful next step is branching from a chosen point in a session, using the
  existing pi session support rather than another agent loop.
- Rank memory matches by exact phrase and query coverage before applying the
  result limit. `ui/tui/src/memory.rs` currently stops after eight substring
  hits, before inspecting all notes.
- Turn the shared diff parser in `apps/stream/diffView.ts` into a review view
  with file and hunk navigation, change counts, and approval context. Desktop
  and mobile currently render raw diff text.
- Let mobile users select an existing workspace and navigate real status,
  steering, diff, and approval views. Its workspace list is currently not a
  selection control, and the tab labels are disabled.
- Connect desktop local sessions to the shared host runtime. Its local session,
  terminal, and browser panes are placeholders; starting a CLI process alone
  does not provide session discovery or reconnectable interaction.

## Boundaries

The TUI and apps are hosts, not separate agent engines. New harness features
belong in rotary and should be exposed through the shared host boundary. Cloud
delivery guarantees require a matching server protocol, not client-side status
labels alone. The observations above are source audit findings, not a claim
that every scenario has been reproduced against a running cloud deployment.
