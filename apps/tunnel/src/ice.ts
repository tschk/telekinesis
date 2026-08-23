export type IceServer = {
  urls: string;
  username?: string;
  credential?: string;
};

export function iceServers(advertise: boolean): IceServer[] {
  if (!advertise) {
    return [];
  }
  return [{ urls: "stun:stun.cloudflare.com:3478" }];
}
