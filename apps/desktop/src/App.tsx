import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AppShell,
  Button,
  HostSwitcher,
  Panel,
  WorkspaceCard,
} from "./components";
import { cloudApiBase, listWorkspaces } from "./lib/cloudApi";
import type { HostMode, SpawnResult, Workspace } from "./lib/types";
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
    } else {
      void refreshLocalStatus();
    }
  }, [mode, refreshCloud, refreshLocalStatus]);

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

  const selected = workspaces.find((w) => w.id === selectedId) ?? null;

  const topBar = (
    <>
      <div className="tk-shell__brand">
        Telekinesis
        <span>desktop ADE</span>
      </div>
      <HostSwitcher mode={mode} onChange={setMode} />
      <div className="tk-shell__top-spacer" />
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
        <div className="tk-nav-list">
          {workspaces.map((ws) => (
            <WorkspaceCard
              key={ws.id}
              workspace={ws}
              selected={ws.id === selectedId}
              onSelect={(w) => setSelectedId(w.id)}
            />
          ))}
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
              Host mode <strong>Cloud</strong> lists workspaces from{" "}
              <code>VITE_TK_CLOUD_API</code> (default{" "}
              <code>http://127.0.0.1:8787/v1/workspaces</code>) and falls back to
              in-app mock JSON when the stub is down.
            </p>
            {selected ? (
              <div className="tk-status-line">
                {`id: ${selected.id}\nname: ${selected.name}\nstatus: ${selected.status}\nregion: ${selected.region ?? "—"}\nsource: ${listSource}`}
              </div>
            ) : (
              <p>No workspace selected.</p>
            )}
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
