/**
 * Unified diff text from a workspace event.
 *
 * A custom entry whose payload is `{ diff }` or `{ patch }`, or a message
 * whose content starts with a diff header, renders as a diff. Other events
 * return null so the log keeps them.
 */

import type { WorkspaceEvent } from "./workspaceStream";

export interface DiffLine {
  kind: "add" | "del" | "hunk" | "meta" | "context";
  text: string;
}

export function diffFromEvent(event: WorkspaceEvent): string | null {
  const payload = event.payload;
  if (isRecord(payload)) {
    if (typeof payload.diff === "string" && payload.diff.includes("\n")) return payload.diff;
    if (typeof payload.patch === "string" && payload.patch.includes("\n")) return payload.patch;
    if (typeof payload.content === "string" && looksLikeDiff(payload.content)) {
      return payload.content;
    }
  }
  return null;
}

export function parseDiff(text: string): DiffLine[] {
  return text.split(/\r?\n/).filter((line, index, all) => line !== "" || index < all.length - 1).map((line) => ({
    kind: lineKind(line),
    text: line,
  }));
}

function lineKind(line: string): DiffLine["kind"] {
  if (line.startsWith("diff ") || line.startsWith("index ") || line.startsWith("--- ") || line.startsWith("+++ ")) {
    return "meta";
  }
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "del";
  return "context";
}

function looksLikeDiff(text: string): boolean {
  return text.includes("\ndiff ") || text.startsWith("diff ") || text.includes("\n@@");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
