export type SignalMessage = {
  type: "offer" | "answer" | "candidate";
  sdp?: string;
  candidate?: string;
};

export function isSignalMessage(value: unknown): value is SignalMessage {
  if (!value || typeof value !== "object") {
    return false;
  }
  const type = (value as { type?: unknown }).type;
  return type === "offer" || type === "answer" || type === "candidate";
}

export class SignalRoom {
  private readonly peers = new Set<{ send(data: string): void }>();

  add(peer: { send(data: string): void }): void {
    this.peers.add(peer);
  }

  remove(peer: { send(data: string): void }): void {
    this.peers.delete(peer);
  }

  size(): number {
    return this.peers.size;
  }

  fanout(from: { send(data: string): void }, message: SignalMessage): number {
    const payload = JSON.stringify(message);
    let sent = 0;
    for (const peer of this.peers) {
      if (peer !== from) {
        peer.send(payload);
        sent += 1;
      }
    }
    return sent;
  }
}
