import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AppShell,
  Button,
  HostSwitcher,
  LogPane,
  Panel,
  PromptBox,
  StatusBadge,
  WorkspaceCard,
} from "./components";
import {
  cloudApiBase,
  connectWorkspaceStream,
  createWorkspace,
  eventText,
  execInWorkspace,
  getHealth,
  listWorkspaces,
} from "./lib/cloudApi";
import type { HealthResponse, HostMode, SpawnResult, Workspace } from "./lib/types";
import { diffFromEvent } from "@tk/diff-view";
import "./App.css";

function healthBadgeStatus(
  health: HealthResponse | null,
  healthError: string | null,
): string {
  if (health?.ok === true) return "ok";
  if (healthError) return "down";
  return "starting";
}

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
  const [streamStatus, setStreamStatus] = useState<string | null>(null);
  const [steer, setSteer] = useState("");
  const [pendingApproval, setPendingApproval] = useState<string | null>(null);
  const [diff, setDiff] = useState<string | null>(null);
  const streamRef = useRef<ReturnType<typeof connectWorkspaceStream> | null>(null);

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

  useEffect(() => {
    if (mode !== "cloud" || !selectedId) {
      setStreamStatus(null);
      return;
    }
    const seen = new Set<number>();
    const stream = connectWorkspaceStream({
      baseUrl: cloudApiBase(),
      workspaceId: selectedId,
      onStatus: setStreamStatus,
      onUnavailable: () => setStreamStatus("unavailable"),
      onEvents: (events) => {
        const rows = events.map((event) => {
          if (seen.has(event.id)) return null;
          seen.add(event.id);
          const text = eventText(event);
          const patch = diffFromEvent(event);
          if (patch) setDiff(patch);
          if (event.kind === "approval_requested") {
            const payload = event.payload;
            const id =
              payload && typeof payload === "object" && "id" in payload
              && typeof payload.id === "string"
                ? payload.id
                : String(event.id);
            setPendingApproval(id);
          }
          return text
            ? `${event.kind} #${event.id} ${text}`
            : `${event.kind} #${event.id}`;
        }).filter((row): row is string => row != null);
        if (rows.length === 0) return;
        setLogLines((prev) => [...prev, ...rows]);
      },
    });
    streamRef.current = stream;
    return () => {
      stream.close();
      streamRef.current = null;
    };
  }, [mode, selectedId]);

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
    setLogLines((prev) => [...prev, `$ ${source}`]);
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

  const healthLabel =
    health?.ok === true
      ? `cloud ok · ${health.computer ?? health.service ?? "up"}`
      : healthError
        ? `cloud down · ${healthError}`
        : "cloud · …";

  const topBar = (
    <>
      <div className="tk-shell__brand">
        <span className="tk-shell__mark" aria-hidden>
          tk
        </span>
        <div className="tk-shell__brand-text">
          <span className="tk-shell__title">Telekinesis</span>
          <span className="tk-shell__subtitle">desktop ADE</span>
        </div>
      </div>
      <HostSwitcher mode={mode} onChange={setMode} />
      <div className="tk-shell__top-spacer" />
      <div className="tk-shell__top-meta">
        <button
          type="button"
          className="tk-health"
          title={healthError ?? JSON.stringify(health ?? {})}
          onClick={() => void refreshHealth()}
          aria-label={healthLabel}
        >
          <StatusBadge status={healthBadgeStatus(health, healthError)} />
          <span className="tk-health__label">{healthLabel}</span>
        </button>
        <span className="tk-shell__billing" title="Billing stub">
          billing · —
        </span>
        <span className="tk-hint tk-shell__api">
          API <code>{cloudApiBase()}</code>
        </span>
      </div>
    </>
  );

  const sidebar =
    mode === "cloud" ? (
      <>
        <div className="tk-nav-header">
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
          <p className="tk-hint tk-hint--danger">Create failed: {createError}</p>
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
            <p className="tk-hint tk-hint--empty">
              No workspaces yet — create one above.
            </p>
          ) : null}
          {workspaces.length === 0 && !listSource && !loading ? (
            <p className="tk-hint tk-hint--empty">Waiting for cloud…</p>
          ) : null}
        </div>
        <h2 className="tk-nav-section-title">Sessions</h2>
        <p className="tk-hint tk-hint--empty">
          {streamStatus
            ? `Event stream ${streamStatus}. Replay continues from the last id.`
            : "Select a workspace to attach the event stream."}
        </p>
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
        <p className="tk-hint tk-hint--empty">
          Local session stubs — parallel worktrees later.
        </p>
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
              <p className="tk-hint tk-hint--empty">No workspace selected.</p>
            )}
            <h3 className="tk-section-label">Exec</h3>
            <PromptBox
              value={prompt}
              onChange={setPrompt}
              onSubmit={() => void onExec()}
              disabled={execBusy || !selected}
            />
            <form
              className="tk-create-ws"
              onSubmit={(e) => {
                e.preventDefault();
                const text = steer.trim();
                if (!text) return;
                streamRef.current?.send("steer", { text });
                setLogLines((prev) => [...prev, `steer ${text}`]);
                setSteer("");
              }}
            >
              <input
                className="tk-create-ws__input"
                value={steer}
                onChange={(e) => setSteer(e.target.value)}
                placeholder="Steer the running session"
                disabled={!selected}
              />
              <Button type="submit" disabled={!selected || !steer.trim()}>
                Steer
              </Button>
            </form>
            {pendingApproval ? (
              <div className="tk-prompt__actions">
                <Button
                  onClick={() => {
                    streamRef.current?.send("approval_response", {
                      id: pendingApproval,
                      approved: true,
                    });
                    setLogLines((prev) => [...prev, `approved ${pendingApproval}`]);
                    setPendingApproval(null);
                  }}
                >
                  Approve
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    streamRef.current?.send("approval_response", {
                      id: pendingApproval,
                      approved: false,
                    });
                    setLogLines((prev) => [...prev, `denied ${pendingApproval}`]);
                    setPendingApproval(null);
                  }}
                >
                  Deny
                </Button>
              </div>
            ) : null}
            <h3 className="tk-section-label">
              Log{streamStatus ? ` · ${streamStatus}` : ""}
            </h3>
            <LogPane lines={logLines} />
          </>
        )}

        <div className="tk-placeholder-grid">
          <div className="tk-placeholder">Terminal pane stub</div>
          <div className="tk-log" role="region" aria-label="Workspace diff">
            {diff ? (
              <pre className="tk-log__body">{diff}</pre>
            ) : (
              <div className="tk-log__empty">No diff in the session yet.</div>
            )}
          </div>
          <div className="tk-placeholder">In-app browser stub</div>
        </div>
      </Panel>
    </AppShell>
  );
}

export default App;
