import { invoke } from "@tauri-apps/api/core";
export interface Project {
  id: string;
  path: string;
  name: string;
}
export interface ThreadConfig {
  provider?: string;
  model?: string;
  effort: string;
  permission: "yolo" | "restrictive";
  role: "general" | "research" | "review" | "plan";
}
export interface Thread {
  id: string;
  projectId: string;
  title: string;
  sessionId: string;
  config: ThreadConfig;
  parentThreadId?: string;
}
export interface Message {
  id: string;
  role: string;
  text: string;
}
export interface Receipt {
  clientMessageId: string;
  threadId: string;
  runId: string;
  state:
    | "queued"
    | "accepted"
    | "inserted"
    | "completed"
    | "cancelled"
    | "failed"
    | "interrupted";
  mode: "steer" | "queue";
  text: string;
  error?: string;
}
export interface ThreadSnapshot {
  thread: Thread;
  messages: Message[];
  receipts: Receipt[];
  activeRunId: string | null;
}
export interface RuntimeEvent {
  projectId: string;
  threadId: string;
  runId: string;
  sequence: number;
  kind: "engine" | "receipt" | "settled";
  payload: unknown;
}
export interface RuntimeOption {
  provider: string;
  model: string;
}
export const localApi = {
  projects: () => invoke<Project[]>("local_projects"),
  addProject: (path: string) => invoke<Project>("local_add_project", { path }),
  options: () => invoke<RuntimeOption[]>("local_runtime_options"),
  threads: (projectId: string) =>
    invoke<Thread[]>("local_threads", { projectId }),
  create: (request: {
    projectId: string;
    title: string;
    config?: ThreadConfig;
    parentThreadId?: string;
    requestId?: string;
  }) => invoke<Thread>("local_create_thread", { request }),
  load: (threadId: string) =>
    invoke<ThreadSnapshot>("local_load_thread", { threadId }),
  update: (threadId: string, config: ThreadConfig) =>
    invoke<Thread>("local_update_thread", { threadId, config }),
  submit: (request: {
    threadId: string;
    clientMessageId: string;
    text: string;
    mode: "steer" | "queue";
    referenceContext?: string;
  }) => invoke<Receipt>("local_submit", { request }),
  cancel: (threadId: string, runId: string) =>
    invoke<boolean>("local_cancel", { threadId, runId }),
};
export const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
