import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  errorMessage,
  localApi,
  type Project,
  type RuntimeEvent,
  type RuntimeOption,
  type Thread,
  type ThreadSnapshot,
} from "../lib/localApi";

export const hasNativeHost = () => "__TAURI_INTERNALS__" in window;
function remember(key: string, value?: string) {
  try {
    if (value !== undefined) localStorage.setItem(key, value);
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
export function useWorkspaceHost() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [threads, setThreads] = useState<Thread[]>([]);
  const [threadId, setThreadId] = useState<string | null>(null);
  const [snapshots, setSnapshots] = useState<Record<string, ThreadSnapshot>>(
    {},
  );
  const [options, setOptions] = useState<RuntimeOption[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [activity, setActivity] = useState<Record<string, string>>({});
  const currentProject = useRef(projectId);
  currentProject.current = projectId;
  const loadVersions = useRef<Record<string, number>>({});
  const refreshThread = useCallback(async (id: string) => {
    const version = (loadVersions.current[id] ?? 0) + 1;
    loadVersions.current[id] = version;
    const loaded = await localApi.load(id);
    if (version !== loadVersions.current[id]) return;
    setSnapshots((previous) => ({ ...previous, [id]: loaded }));
    setThreads((previous) =>
      previous.map((thread) => (thread.id === id ? loaded.thread : thread)),
    );
    if (!loaded.activeRunId)
      setActivity((previous) => {
        const next = { ...previous };
        delete next[id];
        return next;
      });
  }, []);
  const connect = useCallback(async () => {
    if (!hasNativeHost()) return;
    setConnecting(true);
    try {
      const [found, available] = await Promise.all([
        localApi.projects(),
        localApi.options(),
      ]);
      setProjects(found);
      setOptions(available);
      setError(null);
      setProjectId((previous) =>
        found.some((p) => p.id === previous)
          ? previous
          : (found.find(
              (p) => p.id === remember("tk.react.project") && p.path !== "/",
            )?.id ?? null),
      );
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setConnecting(false);
    }
  }, []);
  useEffect(() => {
    void connect();
  }, [connect]);
  useEffect(() => {
    let disposed = false;
    setThreads([]);
    setThreadId(null);
    if (!projectId) return;
    remember("tk.react.project", projectId);
    void localApi
      .threads(projectId)
      .then((found) => {
        if (disposed) return;
        setThreads(found);
        const saved = remember(`tk.react.thread.${projectId}`);
        setThreadId(
          found.find((t) => t.id === saved)?.id ?? found[0]?.id ?? null,
        );
      })
      .catch((failure) => {
        if (!disposed) setError(errorMessage(failure));
      });
    return () => {
      disposed = true;
    };
  }, [projectId]);
  useEffect(() => {
    if (!threadId) return;
    if (projectId) remember(`tk.react.thread.${projectId}`, threadId);
    void refreshThread(threadId).catch((failure) =>
      setError(errorMessage(failure)),
    );
    const timer = window.setInterval(() => {
      void refreshThread(threadId).catch((failure) =>
        setError(errorMessage(failure)),
      );
    }, 2000);
    return () => window.clearInterval(timer);
  }, [threadId, projectId, refreshThread]);
  useEffect(() => {
    if (!hasNativeHost()) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void listen<RuntimeEvent>("local-runtime", ({ payload: event }) => {
      if (event.kind === "engine") {
        const payload = event.payload as Record<string, unknown>;
        setActivity((previous) => ({
          ...previous,
          [event.threadId]:
            payload.type === "ToolExecutionStart"
              ? `Using ${String(payload.name ?? "tool")}…`
              : "Working…",
        }));
        if (payload.type !== "MessageEnd") return;
      }
      void refreshThread(event.threadId).catch((failure) =>
        setError(errorMessage(failure)),
      );
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else cleanup = unlisten;
      })
      .catch((failure) => setError(errorMessage(failure)));
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [refreshThread]);
  function selectThread(id: string) {
    setThreadId(id);
    setError(null);
  }
  function addThread(thread: Thread) {
    if (thread.projectId !== currentProject.current) return;
    setThreads((previous) => [
      thread,
      ...previous.filter((t) => t.id !== thread.id),
    ]);
    selectThread(thread.id);
  }
  return {
    projects,
    setProjects,
    projectId,
    setProjectId,
    threads,
    threadId,
    selectThread,
    addThread,
    snapshots,
    options,
    error,
    setError,
    connecting,
    activity,
    connect,
    refreshThread,
  };
}
