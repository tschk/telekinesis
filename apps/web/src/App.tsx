import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createWorkspace,
  exec,
  getHealth,
  getWorkspace,
  listWorkspaces,
} from "./api/client";
import type { WorkspaceMeta } from "./api/types";
import { AppShell } from "./components/AppShell";
import { Button } from "./components/Button";
import { DiffPane } from "./components/DiffPane";
import { Input } from "./components/Input";
import { LogPane, type LogLine } from "./components/LogPane";
import { Panel } from "./components/Panel";
import { PromptBox } from "./components/PromptBox";
import { WorkspaceCard } from "./components/WorkspaceCard";
import {
  addWorkspaceId,
  loadWorkspaceIds,
  syncWorkspaceIds,
} from "./lib/workspaceStore";

function billingUrl(): string {
  const raw = import.meta.env.VITE_BILLING_PORTAL_URL as string | undefined;
  return (raw && raw.trim()) || "#";
}

function nowTs(): string {
  return new Date().toLocaleTimeString();
}

function makeId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export default function App() {
  const [healthOk, setHealthOk] = useState<boolean | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceMeta[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [createName, setCreateName] = useState("");
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState(false);
  const [listSource, setListSource] = useState<"api" | "local" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lines, setLines] = useState<LogLine[]>([]);

  const appendLog = useCallback((text: string, tone?: LogLine["tone"]) => {
    setLines((prev) => [
      ...prev,
      { id: makeId(), ts: nowTs(), text, tone },
    ]);
  }, []);

  const refreshWorkspaces = useCallback(async () => {
    setError(null);
    try {
      const listed = await listWorkspaces();
      setWorkspaces(listed);
      syncWorkspaceIds(listed.map((w) => w.id));
      setListSource("api");
      setSelectedId((cur) => cur ?? listed[0]?.id ?? null);
      return;
    } catch {
      // list not ready / offline — fall back to localStorage ids
    }

    const ids = loadWorkspaceIds();
    const metas: WorkspaceMeta[] = [];
    for (const id of ids) {
      try {
        metas.push(await getWorkspace(id));
      } catch (err) {
        metas.push({
          id,
          name: id,
          tier: "unknown",
          createdAt: "",
          computerBackend: null,
          status: err instanceof Error ? `error: ${err.message}` : "unreachable",
        });
      }
    }
    setWorkspaces(metas);
    setListSource("local");
    setSelectedId((cur) => cur ?? metas[0]?.id ?? null);
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await getHealth();
        if (!cancelled) setHealthOk(true);
      } catch {
        if (!cancelled) setHealthOk(false);
      }
      if (!cancelled) await refreshWorkspaces();
    })();
    return () => {
      cancelled = true;
    };
  }, [refreshWorkspaces]);

  const selected = useMemo(
    () => workspaces.find((w) => w.id === selectedId) ?? null,
    [workspaces, selectedId],
  );

  async function handleCreate() {
    setCreating(true);
    setError(null);
    try {
      const meta = await createWorkspace(
        createName.trim() ? { name: createName.trim() } : undefined,
      );
      addWorkspaceId(meta.id);
      setCreateName("");
      setWorkspaces((prev) => {
        const without = prev.filter((w) => w.id !== meta.id);
        return [meta, ...without];
      });
      setSelectedId(meta.id);
      appendLog(`Created workspace ${meta.id} (${meta.name})`, "ok");
      // Prefer re-sync via list when available
      void refreshWorkspaces().then(() => setSelectedId(meta.id));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      appendLog(`Create failed: ${msg}`, "danger");
    } finally {
      setCreating(false);
    }
  }

  async function handleRun(source: string) {
    if (!selectedId) return;
    setBusy(true);
    setError(null);
    appendLog(`$ exec ${selectedId}\n${source}`, "muted");
    try {
      const result = await exec(selectedId, { source });
      const bits: string[] = [];
      if (result.message) bits.push(result.message);
      if (result.stdout) bits.push(result.stdout);
      if (result.stderr) bits.push(result.stderr);
      if (result.exitCode !== undefined) bits.push(`exitCode=${result.exitCode}`);
      if (result.backend) bits.push(`backend=${result.backend}`);
      if (result.stub) bits.push("(stub)");
      const text = bits.join("\n") || JSON.stringify(result);
      appendLog(text, result.ok ? "ok" : "danger");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      appendLog(`Exec failed: ${msg}`, "danger");
    } finally {
      setBusy(false);
    }
  }

  return (
    <AppShell healthOk={healthOk} billingUrl={billingUrl()}>
      <div className="tk-layout">
        <aside className="tk-layout__side">
          <Panel
            title="Workspaces"
            actions={
              <Button variant="ghost" onClick={() => void refreshWorkspaces()}>
                Refresh
              </Button>
            }
          >
            <p className="tk-hint">
              {listSource === "api"
                ? "Loaded from GET /v1/workspaces"
                : listSource === "local"
                  ? "List API unavailable — using localStorage ids"
                  : "Loading…"}
            </p>
            <div className="tk-create">
              <Input
                label="Name (optional)"
                name="workspaceName"
                placeholder="my-workspace"
                value={createName}
                onChange={(e) => setCreateName(e.target.value)}
                disabled={creating}
              />
              <Button onClick={() => void handleCreate()} disabled={creating}>
                {creating ? "Creating…" : "Create workspace"}
              </Button>
            </div>
            <div className="tk-wlist">
              {workspaces.length === 0 ? (
                <p className="tk-hint">No workspaces yet. Create one to start.</p>
              ) : (
                workspaces.map((w) => (
                  <WorkspaceCard
                    key={w.id}
                    workspace={w}
                    selected={w.id === selectedId}
                    onSelect={setSelectedId}
                  />
                ))
              )}
            </div>
          </Panel>
        </aside>

        <section className="tk-layout__main">
          <Panel
            title={selected ? `Exec · ${selected.name || selected.id}` : "Exec"}
          >
            {!selected ? (
              <p className="tk-hint">Select or create a workspace to run prompts.</p>
            ) : (
              <PromptBox
                disabled={!selected}
                busy={busy}
                onRun={handleRun}
              />
            )}
            {error ? <p className="tk-error">{error}</p> : null}
          </Panel>

          <Panel title="Log">
            <LogPane lines={lines} />
          </Panel>

          <Panel title="Diff">
            <DiffPane />
          </Panel>
        </section>
      </div>
    </AppShell>
  );
}
