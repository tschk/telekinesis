import { describe, expect, it } from "vitest";
import { diffFromEvent, parseDiff } from "./diffView";

const DIFF = "diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1 +1 @@\n-old\n+new\n";

describe("diffFromEvent", () => {
  it("reads a custom payload.diff and ignores a normal message", () => {
    expect(
      diffFromEvent({ id: 1, ts: "", kind: "custom", payload: { diff: DIFF } }),
    ).toBe(DIFF);
    expect(
      diffFromEvent({
        id: 2,
        ts: "",
        kind: "message",
        payload: { content: "no diff here" },
      }),
    ).toBeNull();
  });

  it("classifies a deletion that is also a diff header as meta, not a deletion", () => {
    const lines = parseDiff("--- a/a.ts\n-old\n+new\n");
    expect(lines.map((line) => line.kind)).toEqual(["meta", "del", "add"]);
  });
});
