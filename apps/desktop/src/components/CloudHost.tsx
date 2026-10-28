import { useCallback, useEffect, useState } from "react";
import { Button, LogPane, PromptBox, WorkspaceCard } from "./index";
import {
  cloudApiBase,
  createWorkspace,
  execInWorkspace,
  getHealth,
  listWorkspaces,
} from "../lib/cloudApi";
import type { Workspace } from "../lib/types";

/** Kept mounted across host switches so cloud drafts never become local drafts. */
export function CloudHost({ active }: { active: boolean }) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const [logs, setLogs] = useState<Record<string, string[]>>({});
  const [error, setError] = useState<string | null>(null);
  const [health, setHealth] = useState("Checking connection…");
  const [loading, setLoading] = useState(false);
  const [creating, setCreating] = useState(false);
  const [running, setRunning] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    setLoading(true);
    const [list, status] = await Promise.all([listWorkspaces(), getHealth()]);
    setWorkspaces(list.workspaces);
    setError(list.error ?? null);
    setHealth(status.health?.ok ? "Connected" : "Unavailable");
    setSelectedId((previous) =>
      list.workspaces.some((w) => w.id === previous)
        ? previous
        : (list.workspaces[0]?.id ?? null),
    );
    setLoading(false);
  }, []);
  useEffect(() => {
    if (active) void refresh();
  }, [active, refresh]);
  async function create() {
    setCreating(true);
    const result = await createWorkspace({ name: name.trim() || undefined });
    setCreating(false);
    if (result.error || !result.workspace.id) {
      setError(result.error ?? "Workspace could not be created.");
      return;
    }
    setWorkspaces((previous) => [
      result.workspace,
      ...previous.filter((w) => w.id !== result.workspace.id),
    ]);
    setSelectedId(result.workspace.id);
    setName("");
    setError(null);
  }
  async function execute() {
    if (!selectedId || !prompt.trim() || running) return;
    const id = selectedId;
    const source = prompt.trim();
    setRunning(id);
    setPrompt("");
    setLogs((previous) => ({
      ...previous,
      [id]: [...(previous[id] ?? []), `$ ${source}`],
    }));
    const { result, error: failure } = await execInWorkspace(id, { source });
    const output = [
      result.stdout,
      result.stderr && `[stderr] ${result.stderr}`,
      result.message,
      failure,
      `# ${result.ok ? "Completed" : "Failed"}${result.exitCode != null ? ` · exit ${result.exitCode}` : ""}${result.stub ? " · server returned a stub" : ""}`,
    ].filter((v): v is string => Boolean(v));
    setLogs((previous) => ({
      ...previous,
      [id]: [...(previous[id] ?? []), ...output],
    }));
    setRunning(null);
  }
  const selected = workspaces.find((w) => w.id === selectedId);
  return (
    <div className="host-layout" hidden={!active}>
      <aside className="project-nav" aria-label="Cloud workspaces">
        <div className="nav-heading">
          <span>Cloud workspaces</span>
          <Button
            variant="ghost"
            onClick={() => void refresh()}
            disabled={loading}
          >
            {loading ? "Loading…" : "Refresh"}
          </Button>
        </div>
        <form
          className="compact-form"
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
        >
          <label htmlFor="cloud-name">New workspace</label>
          <input
            id="cloud-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Workspace name"
          />
          <Button type="submit" disabled={creating}>
            {creating ? "Creating…" : "Create workspace"}
          </Button>
        </form>
        {workspaces.map((workspace) => (
          <WorkspaceCard
            key={workspace.id}
            workspace={workspace}
            selected={workspace.id === selectedId}
            onSelect={(w) => setSelectedId(w.id)}
          />
        ))}
        {!workspaces.length && (
          <p className="muted">
            {loading ? "Loading workspaces…" : "No connected workspaces."}
          </p>
        )}
        <div className="nav-footer">
          <span className="connection-dot" />
          {health}
          <code>{cloudApiBase()}</code>
        </div>
      </aside>
      <main className="thread-main">
        <header className="thread-header">
          <div>
            <span className="eyebrow">Cloud host</span>
            <h1>{selected?.name ?? "Your remote workspace"}</h1>
          </div>
        </header>
        {error && (
          <p className="notice error" role="alert">
            {error}
          </p>
        )}
        <div className="cloud-content">
          <p className="muted">
            Run a command in your connected cloud workspace. Local projects and
            threads stay on this computer.
          </p>
          <LogPane lines={selectedId ? (logs[selectedId] ?? []) : []} />
        </div>
        <div className="composer">
          <PromptBox
            value={prompt}
            onChange={setPrompt}
            onSubmit={() => void execute()}
            disabled={Boolean(running) || !selected}
          />
        </div>
      </main>
    </div>
  );
}
