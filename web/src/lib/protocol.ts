// The wire types are generated from src/protocol.rs (protocol.gen.ts); this module adds
// parsing and labels on top.
import type { ServerMsg, TrackKind } from './protocol.gen';

export type * from './protocol.gen';

const MESSAGE_TYPES: ReadonlySet<unknown> = new Set<ServerMsg['type']>(['hello', 'frame']);

/** Parses one WebSocket text message; `null` for malformed or unknown messages. */
export function parseMsg(data: string): ServerMsg | null {
  try {
    const msg = JSON.parse(data);
    return msg && MESSAGE_TYPES.has(msg.type) ? (msg as ServerMsg) : null;
  } catch {
    return null;
  }
}

export const KIND_LABEL: Record<TrackKind, string> = {
  fhss_control: 'Drone control link (FHSS)',
  wideband: 'Wideband (Wi-Fi / video)',
};
