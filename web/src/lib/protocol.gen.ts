// Generated from src/protocol.rs by `UPDATE_PROTOCOL=1 cargo test`. Do not edit.

export type Band = { start_mhz: number, stop_mhz: number, bins: number, fps: number, };

export type TrackKind = "fhss_control" | "wideband";

export type Track = { id: number, kind: TrackKind, center_mhz: number, bandwidth_mhz: number, peak_dbm: number, /** 0..1 */ confidence: number, first_seen_ms: number, last_seen_ms: number, active: boolean, };

export type Hello = { device: string, version: string, band: Band, };

export type Frame = { seq: number, t_ms: number, noise_floor_dbm: number, threshold_dbm: number, /** Power per bin, rounded to 0.1 dB to keep frames small. */ power_dbm: Array<number>, tracks: Array<Track>, };

export type ServerMsg = { "type": "hello" } & Hello | { "type": "frame" } & Frame;

export type Status = { device: string, version: string, /** Sweeps processed since start. */ frames: number, /** Connected WebSocket clients. */ clients: number, drone_detected: boolean, tracks: Array<Track>, };
