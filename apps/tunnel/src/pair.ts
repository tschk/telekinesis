export type PairRecord = {
  token: string;
  expiresAt: number;
  createdAt: number;
};

export const PAIR_TTL_MS = 30 * 60 * 1000;

export function mintPairToken(now = Date.now(), ttlMs = PAIR_TTL_MS): PairRecord {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  const token = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  return {
    token,
    createdAt: now,
    expiresAt: now + ttlMs,
  };
}

export function pairUrl(host: string, port: number, token: string): string {
  return `tkpair://${host}:${port}/ws?token=${token}`;
}

export function isUsablePair(record: PairRecord | undefined, now = Date.now()): boolean {
  return Boolean(record && record.token.length >= 32 && record.expiresAt > now);
}

export class PairStore {
  private readonly tokens = new Map<string, PairRecord>();

  mint(now = Date.now()): PairRecord {
    this.prune(now);
    const record = mintPairToken(now);
    this.tokens.set(record.token, record);
    return record;
  }

  get(token: string, now = Date.now()): PairRecord | undefined {
    const record = this.tokens.get(token);
    if (!isUsablePair(record, now)) {
      if (record) {
        this.tokens.delete(token);
      }
      return undefined;
    }
    return record;
  }

  prune(now = Date.now()): number {
    let removed = 0;
    for (const [token, record] of this.tokens) {
      if (!isUsablePair(record, now)) {
        this.tokens.delete(token);
        removed += 1;
      }
    }
    return removed;
  }

  size(): number {
    return this.tokens.size;
  }
}
