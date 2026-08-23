import os from "node:os";

export function isLoopbackHost(host: string): boolean {
  const value = host.trim().toLowerCase();
  return (
    value === "127.0.0.1" ||
    value === "localhost" ||
    value === "0.0.0.0" ||
    value === "::1" ||
    value === "[::1]" ||
    value === "::" ||
    value.startsWith("127.")
  );
}

export function discoverLanHost(): string | undefined {
  const interfaces = os.networkInterfaces();
  for (const addrs of Object.values(interfaces)) {
    for (const addr of addrs ?? []) {
      if (addr.internal) {
        continue;
      }
      const family = String(addr.family);
      if (family === "IPv4" || family === "4") {
        if (!isLoopbackHost(addr.address)) {
          return addr.address;
        }
      }
    }
  }
  return undefined;
}

export type ListenConfig = {
  advertise: boolean;
  bindHost: string;
  publicHost: string;
};

export function resolveListenConfig(options: {
  advertise: boolean;
  advertiseHost?: string;
}): ListenConfig {
  if (!options.advertise) {
    return { advertise: false, bindHost: "127.0.0.1", publicHost: "127.0.0.1" };
  }
  const explicit = options.advertiseHost?.trim() ?? "";
  if (explicit) {
    if (isLoopbackHost(explicit)) {
      throw new Error("advertise host must be reachable; refusing loopback pairing URLs");
    }
    return { advertise: true, bindHost: explicit, publicHost: explicit };
  }
  const found = discoverLanHost();
  if (!found) {
    throw new Error("advertise requires TK_ADVERTISE_HOST or a non-loopback interface");
  }
  return { advertise: true, bindHost: found, publicHost: found };
}

export function withCompanionToken(url: string, token?: string): string {
  if (!token) {
    return url;
  }
  const parsed = new URL(url);
  if (!parsed.searchParams.get("token")) {
    parsed.searchParams.set("token", token);
  }
  return parsed.toString();
}
