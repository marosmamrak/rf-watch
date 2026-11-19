// Mirrors src/protocol.rs and src/detect.rs.

export interface Band {
  start_mhz: number;
  stop_mhz: number;
  bins: number;
  fps: number;
}

export type TrackKind = 'fhss_control' | 'wideband';

export interface Track {
  id: number;
  kind: TrackKind;
  center_mhz: number;
  bandwidth_mhz: number;
  peak_dbm: number;
  confidence: number;
  first_seen_ms: number;
  last_seen_ms: number;
  active: boolean;
}

export interface Hello {
  type: 'hello';
  device: string;
  version: string;
  band: Band;
}

export interface Frame {
  type: 'frame';
  seq: number;
  t_ms: number;
  noise_floor_dbm: number;
  threshold_dbm: number;
  power_dbm: number[];
  tracks: Track[];
}

export type ServerMsg = Hello | Frame;

export function parseMsg(data: string): ServerMsg | null {
  try {
    const msg = JSON.parse(data);
    return msg && (msg.type === 'hello' || msg.type === 'frame') ? (msg as ServerMsg) : null;
  } catch {
    return null;
  }
}

export const KIND_LABEL: Record<TrackKind, string> = {
  fhss_control: 'Drone control link (FHSS)',
  wideband: 'Wideband (Wi-Fi / video)',
};
