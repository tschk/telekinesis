import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AppShell,
  Button,
  HostSwitcher,
  LogPane,
  Panel,
  PromptBox,
  WorkspaceCard,
} from "./components";
import {
  cloudApiBase,
  createWorkspace,
  execInWorkspace,
  getHealth,
  listWorkspaces,
} from "./lib/cloudApi";
import type { HealthResponse, HostMode, SpawnResult, Workspace } from "./lib/types";
import "./App.css";

function App() {
  const [mode, setMode] = useState<HostMode>("local");
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [listSource, setListSource] = useState<"api" | "mock" | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [localStatus, setLocalStatus] = useState<string>(
    "Checking for `tk` / `telekinesis` on PATH…",
  );
  const [spawnBusy, setSpawnBusy] = useState(false);
  const [newName, setNewName] = useState("");
  const [createBusy, setCreateBusy] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [prompt, setPrompt] = useState("");
  const [execBusy, setExecBusy] = useState(false);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);

  const refreshHealth = useCallback(async () => {
    const result = await getHealth();
    setHealth(result.health);
    setHealthError(result.error ?? null);
  }, []);

  const refreshCloud = useCallback(async () => {
    setLoading(true);
    const result = await listWorkspaces();
    setWorkspaces(result.workspaces);
    setListSource(result.source);
    setListError(result.error ?? null);
    setSelectedId((prev) => {
      if (prev && result.workspaces.some((w) => w.id === prev)) return prev;
      return result.workspaces[0]?.id ?? null;
    });
    setLoading(false);
  }, []);

  const refreshLocalStatus = useCallback(async () => {
    try {
      const result = await invoke<SpawnResult>("telekinesis_status");
      setLocalStatus(result.message);
    } catch {
      // Browser/vite-only: Tauri IPC unavailable
      setLocalStatus(
        "Tauri IPC unavailable in browser preview. Use `npm run tauri dev` for local spawn.",
      );
    }
  }, []);

  useEffect(() => {
    if (mode === "cloud") {
      void refreshCloud();
      void refreshHealth();
      const id = window.setInterval(() => {
        void refreshHealth();
      }, 15_000);
      return () => window.clearInterval(id);
    }
    void refreshLocalStatus();
    return undefined;
  }, [mode, refreshCloud, refreshHealth, refreshLocalStatus]);

  async function onSpawn() {
    setSpawnBusy(true);
    try {
      const result = await invoke<SpawnResult>("spawn_telekinesis");
      setLocalStatus(result.message);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setLocalStatus(
        `Spawn stub failed (expected outside Tauri): ${message}`,
      );
    } finally {
      setSpawnBusy(false);
    }
  }

  async function onCreateWorkspace() {
    const name = newName.trim() || undefined;
    setCreateBusy(true);
    setCreateError(null);
    const { workspace, error } = await createWorkspace({ name });
    setCreateBusy(false);
    if (error || !workspace.id) {
      setCreateError(error ?? "create failed");
      return;
    }
    setNewName("");
    setWorkspaces((prev) => {
      if (prev.some((w) => w.id === workspace.id)) return prev;
      return [workspace, ...prev];
    });
    setSelectedId(workspace.id);
    setListSource("api");
    void refreshCloud();
  }

  async function onExec() {
    if (!selectedId || !prompt.trim()) return;
    const source = prompt.trim();
    setExecBusy(true);
    setLogLines((prev) => [
      ...prev,
      `$ ${source}`,
    ]);
    const { result, error } = await execInWorkspace(selectedId, { source });
    const chunks: string[] = [];
    if (result.stdout) chunks.push(result.stdout.replace(/\n$/, ""));
    if (result.stderr) {
      chunks.push(
        result.stderr
          .split("\n")
          .map((line) => (line ? `[stderr] ${line}` : "[stderr]"))
          .join("\n")
          .replace(/\n$/, ""),
      );
    }
    if (result.message && !result.stdout && !result.stderr) {
      chunks.push(result.message);
    }
    const footer = [
      result.ok ? "ok" : "fail",
      result.exitCode != null ? `exit=${result.exitCode}` : null,
      result.backend ? `backend=${result.backend}` : null,
      result.stub ? "stub" : null,
      error && result.ok !== false ? `err=${error}` : null,
    ]
      .filter(Boolean)
      .join(" ");
    setLogLines((prev) => [
      ...prev,
      ...(chunks.length ? chunks : ["(no output)"]),
      `# ${footer}`,
      "",
    ]);
    setExecBusy(false);
  }

  const selected = workspaces.find((w) => w.id === selectedId) ?? null;

  const healthChipClass =
    health?.ok === true
      ? "tk-health tk-health--ok"
      : healthError
        ? "tk-health tk-health--down"
        : "tk-health tk-health--unknown";

  const healthLabel =
    health?.ok === true
      ? `cloud ok · ${health.computer ?? health.service ?? "up"}`
      : healthError
        ? `cloud down · ${healthError}`
        : "cloud · …";

  const topBar = (
    <>
      <div className="tk-shell__brand">
        Telekinesis
        <span>desktop ADE</span>
      </div>
      <HostSwitcher mode={mode} onChange={setMode} />
      <div className="tk-shell__top-spacer" />
      <button
        type="button"
        className={healthChipClass}
        title={healthError ?? JSON.stringify(health ?? {})}
        onClick={() => void refreshHealth()}
      >
        {healthLabel}
      </button>
      <span className="tk-hint" style={{ borderStyle: "solid" }}>
        API: <code>{cloudApiBase()}</code>
      </span>
    </>
  );

  const sidebar =
    mode === "cloud" ? (
      <>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <h2 className="tk-nav-section-title">Workspaces</h2>
          <Button variant="ghost" onClick={() => void refreshCloud()} disabled={loading}>
            {loading ? "…" : "Refresh"}
          </Button>
        </div>
        <p className="tk-hint">
          Source: <code>{listSource ?? "—"}</code>
          {listError ? ` — ${listError}` : null}
        </p>
        <form
          className="tk-create-ws"
          onSubmit={(e) => {
            e.preventDefault();
            void onCreateWorkspace();
          }}
        >
          <input
            className="tk-create-ws__input"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="New workspace name"
            disabled={createBusy}
          />
          <Button type="submit" disabled={createBusy}>
            {createBusy ? "…" : "Create"}
          </Button>
        </form>
        {createError ? (
          <p className="tk-hint" style={{ color: "var(--tk-danger)" }}>
            Create failed: {createError}
          </p>
        ) : null}
        <div className="tk-nav-list">
          {workspaces.map((ws) => (
            <WorkspaceCard
              key={ws.id}
              workspace={ws}
              selected={ws.id === selectedId}
              onSelect={(w) => setSelectedId(w.id)}
            />
          ))}
          {workspaces.length === 0 && listSource === "api" ? (
            <p className="tk-hint">No workspaces yet — create one above.</p>
          ) : null}
        </div>
        <h2 className="tk-nav-section-title">Sessions</h2>
        <p className="tk-hint">Session list stub — wire to tk-cloud WS later.</p>
      </>
    ) : (
      <>
        <h2 className="tk-nav-section-title">Local daemon</h2>
        <p className="tk-hint">
          Spawns <code>tk</code> or <code>telekinesis</code> from PATH via Tauri
          command. Missing binary is handled gracefully.
        </p>
        <Button onClick={() => void onSpawn()} disabled={spawnBusy}>
          {spawnBusy ? "Spawning…" : "Spawn telekinesis"}
        </Button>
        <Button variant="ghost" onClick={() => void refreshLocalStatus()}>
          Recheck PATH
        </Button>
        <h2 className="tk-nav-section-title">Sessions</h2>
        <p className="tk-hint">Local session stubs — parallel worktrees later.</p>
      </>
    );

  return (
    <AppShell topBar={topBar} sidebar={sidebar}>
      <Panel
        title={
          mode === "cloud"
            ? selected
              ? `Cloud · ${selected.name}`
              : "Cloud · no workspace"
            : "Local · telekinesis"
        }
        actions={
          mode === "cloud" ? (
            <Button variant="ghost" onClick={() => void refreshCloud()}>
              Reload
            </Button>
          ) : (
            <Button onClick={() => void onSpawn()} disabled={spawnBusy}>
              Spawn
            </Button>
          )
        }
      >
        {mode === "local" ? (
          <>
            <p>
              Host mode <strong>Local</strong> talks to the telekinesis daemon /
              CLI on this machine. Cloud Workspace Durable Objects stay on the
              other side of the switcher (see{" "}
              <code>tschk/tk-cloud</code> PLAN · M6).
            </p>
            <div className="tk-status-line">{localStatus}</div>
          </>
        ) : (
          <>
            <p>
              Host mode <strong>Cloud</strong> talks to the tk-cloud M1 API (
              <code>GET/POST /v1/workspaces</code>,{" "}
              <code>POST …/exec</code>). Point{" "}
              <code>VITE_TK_CLOUD_API</code> at wrangler dev. Empty list from API
              is valid; mock fallback only on network/HTTP failure.
            </p>
            {selected ? (
              <div className="tk-status-line">
                {`id: ${selected.id}\nname: ${selected.name}\ntier: ${selected.tier}\nbackend: ${selected.computerBackend ?? "—"}\nstatus: ${selected.status}\ncreatedAt: ${selected.createdAt}\nsource: ${listSource}`}
              </div>
            ) : (
              <p>No workspace selected.</p>
            )}
            <h3 className="tk-section-label">Exec</h3>
            <PromptBox
              value={prompt}
              onChange={setPrompt}
              onSubmit={() => void onExec()}
              disabled={execBusy || !selected}
            />
            <h3 className="tk-section-label">Log</h3>
            <LogPane lines={logLines} />
          </>
        )}

        <div className="tk-placeholder-grid">
          <div className="tk-placeholder">Terminal pane stub</div>
          <div className="tk-placeholder">Diff review stub</div>
          <div className="tk-placeholder">In-app browser stub</div>
        </div>
      </Panel>
    </AppShell>
  );
}

export default App;
