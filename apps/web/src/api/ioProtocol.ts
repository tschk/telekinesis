import type { PromptRequest } from "./types";

/** Page → worker messages. Plain structured-clone objects only. */
export interface IoInitMessage {
  id: number;
  type: "init";
  baseUrl: string;
}

export interface IoHttpMessage {
  id: number;
  type: "http";
  method: string;
  path: string;
  body?: unknown;
}

export interface IoPromptMessage {
  id: number;
  type: "prompt";
  workspaceId: string;
  req: PromptRequest;
}

export interface IoParseWorkspaceIdsMessage {
  id: number;
  type: "parseWorkspaceIds";
  raw: string;
}

export interface IoStringifyWorkspaceIdsMessage {
  id: number;
  type: "stringifyWorkspaceIds";
  ids: string[];
}

export interface IoStringifyMessage {
  id: number;
  type: "stringify";
  value: unknown;
}

export type IoRequest =
  | IoInitMessage
  | IoHttpMessage
  | IoPromptMessage
  | IoParseWorkspaceIdsMessage
  | IoStringifyWorkspaceIdsMessage
  | IoStringifyMessage;

/** Worker → page error shapes; the page rebuilds real Error subclasses. */
export interface IoHttpErrorShape {
  name: "ApiHttpError";
  status: number;
  method: string;
  path: string;
  message: string;
}

export interface IoPlainErrorShape {
  name: "Error";
  message: string;
}

export type IoErrorShape = IoHttpErrorShape | IoPlainErrorShape;

export type IoReply =
  | { id: number; ok: true; result: unknown }
  | { id: number; ok: false; error: IoErrorShape };
