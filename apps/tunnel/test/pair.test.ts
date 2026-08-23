import { describe, expect, test } from "bun:test";
import { iceServers } from "../src/ice.ts";
import { isUsablePair, mintPairToken, PairStore, pairUrl } from "../src/pair.ts";
import { isSignalMessage, SignalRoom } from "../src/signal.ts";

describe("pairing", () => {
  test("mints a 32-char token and QR-ready url", () => {
    const record = mintPairToken(1_000);
    expect(record.token).toHaveLength(32);
    expect(record.expiresAt).toBe(1_000 + 30 * 60 * 1000);
    expect(pairUrl("127.0.0.1", 8787, record.token)).toBe(
      `tkpair://127.0.0.1:8787/ws?token=${record.token}`,
    );
  });

  test("rejects expired or missing tokens", () => {
    const store = new PairStore();
    const record = store.mint(1_000);
    expect(store.get(record.token, 1_000)).toEqual(record);
    expect(store.get(record.token, record.expiresAt + 1)).toBeUndefined();
    expect(store.get("nope")).toBeUndefined();
    expect(isUsablePair(undefined)).toBe(false);
  });

  test("loopback ice list is empty; advertise adds stun", () => {
    expect(iceServers(false)).toEqual([]);
    expect(iceServers(true)).toEqual([{ urls: "stun:stun.cloudflare.com:3478" }]);
  });

  test("signal room fans out to the other peer", () => {
    const room = new SignalRoom();
    const seen: string[] = [];
    const a = { send() {} };
    const b = {
      send(data: string) {
        seen.push(data);
      },
    };
    room.add(a);
    room.add(b);
    expect(isSignalMessage({ type: "offer", sdp: "v=0" })).toBe(true);
    expect(room.fanout(a, { type: "offer", sdp: "v=0" })).toBe(1);
    expect(seen).toEqual([JSON.stringify({ type: "offer", sdp: "v=0" })]);
  });
});
