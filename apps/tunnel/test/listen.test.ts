import { describe, expect, test } from "bun:test";
import { isLoopbackHost, resolveListenConfig } from "../src/listen.ts";

describe("listen config", () => {
  test("default bind is loopback", () => {
    expect(resolveListenConfig({ advertise: false })).toEqual({
      advertise: false,
      bindHost: "127.0.0.1",
      publicHost: "127.0.0.1",
    });
  });

  test("advertise refuses loopback pairing hosts", () => {
    expect(() => resolveListenConfig({ advertise: true, advertiseHost: "127.0.0.1" })).toThrow(
      /reachable|loopback/,
    );
    expect(() => resolveListenConfig({ advertise: true, advertiseHost: "localhost" })).toThrow();
    expect(isLoopbackHost("0.0.0.0")).toBe(true);
  });

  test("advertise with an explicit LAN host binds all interfaces and emits that host", () => {
    const cfg = resolveListenConfig({ advertise: true, advertiseHost: "192.168.1.20" });
    expect(cfg.bindHost).toBe("0.0.0.0");
    expect(cfg.publicHost).toBe("192.168.1.20");
    expect(cfg.publicHost).not.toBe("127.0.0.1");
  });
});
