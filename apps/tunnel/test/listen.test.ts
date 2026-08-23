import { describe, expect, test } from "bun:test";
import { isLoopbackHost, resolveListenConfig, withCompanionToken } from "../src/listen.ts";

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
    expect(() => resolveListenConfig({ advertise: true, advertiseHost: "0.0.0.0" })).toThrow();
    expect(isLoopbackHost("0.0.0.0")).toBe(true);
  });

  test("advertise binds the reachable host, not every interface", () => {
    const cfg = resolveListenConfig({ advertise: true, advertiseHost: "192.168.1.20" });
    expect(cfg.bindHost).toBe("192.168.1.20");
    expect(cfg.publicHost).toBe("192.168.1.20");
    expect(cfg.bindHost).not.toBe("0.0.0.0");
    expect(cfg.publicHost).not.toBe("127.0.0.1");
  });

  test("companion token is appended once", () => {
    expect(withCompanionToken("ws://127.0.0.1:17421", "abc")).toBe(
      "ws://127.0.0.1:17421/?token=abc",
    );
    expect(withCompanionToken("ws://127.0.0.1:17421/?token=abc", "other")).toBe(
      "ws://127.0.0.1:17421/?token=abc",
    );
  });
});
