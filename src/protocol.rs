//! The wire contract: JSON messages on `/ws` (tagged by `type`) and `/api/status`.
//!
//! These types are the single source of truth for every client:
//! - the TypeScript types of the web UI (`web/src/lib/protocol.gen.ts`) are generated from
//!   them,
//! - sample messages (`protocol/*.json`) are generated from them and parsed by the web
//!   and Android tests.
//!
//! `cargo test` fails when either is out of date; `UPDATE_PROTOCOL=1 cargo test`
//! regenerates them. Domain types ([`crate::detect`]) are converted at this boundary, so
//! they can change without changing what clients see.

use serde::Serialize;

use crate::detect::{self, Report};
use crate::spectrum::{self, Sweep};

/// The band as announced to clients, with the sweep rate.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Band {
    pub start_mhz: f32,
    pub stop_mhz: f32,
    pub bins: usize,
    pub fps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    /// Frequency-hopping remote-control link (typical for drones).
    FhssControl,
    /// Wideband emitter: Wi-Fi, analog or digital video downlink.
    Wideband,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Track {
    pub id: u32,
    pub kind: TrackKind,
    pub center_mhz: f32,
    pub bandwidth_mhz: f32,
    pub peak_dbm: f32,
    /// 0..1
    pub confidence: f32,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    pub active: bool,
}

/// First message on every connection.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Hello {
    pub device: String,
    pub version: String,
    pub band: Band,
}

/// One processed sweep.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Frame {
    pub seq: u64,
    pub t_ms: u64,
    pub noise_floor_dbm: f32,
    pub threshold_dbm: f32,
    /// Power per bin, rounded to 0.1 dB to keep frames small.
    pub power_dbm: Vec<f32>,
    pub tracks: Vec<Track>,
}

/// Everything the node sends on `/ws`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello(Hello),
    Frame(Frame),
}

/// `GET /api/status`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Status {
    pub device: String,
    pub version: String,
    /// Sweeps processed since start.
    pub frames: u64,
    /// Connected WebSocket clients.
    pub clients: usize,
    pub drone_detected: bool,
    pub tracks: Vec<Track>,
}

impl Band {
    pub fn new(band: &spectrum::Band, fps: u32) -> Self {
        Self {
            start_mhz: band.start_mhz,
            stop_mhz: band.stop_mhz,
            bins: band.bins,
            fps,
        }
    }
}

impl Frame {
    pub fn new(seq: u64, sweep: &Sweep, report: &Report) -> Self {
        Self {
            seq,
            t_ms: sweep.t_ms,
            noise_floor_dbm: round_tenth(report.noise_floor_dbm),
            threshold_dbm: round_tenth(report.threshold_dbm),
            power_dbm: sweep.power_dbm.iter().copied().map(round_tenth).collect(),
            tracks: report.tracks.iter().map(Track::from).collect(),
        }
    }
}

impl From<&detect::Track> for Track {
    fn from(t: &detect::Track) -> Self {
        Self {
            id: t.id,
            kind: match t.kind {
                detect::TrackKind::FhssControl => TrackKind::FhssControl,
                detect::TrackKind::Wideband => TrackKind::Wideband,
            },
            center_mhz: t.center_mhz,
            bandwidth_mhz: t.bandwidth_mhz,
            peak_dbm: t.peak_dbm,
            confidence: t.confidence,
            first_seen_ms: t.first_seen_ms,
            last_seen_ms: t.last_seen_ms,
            active: t.active,
        }
    }
}

pub fn round_tenth(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ts_rs::{Config, TS};

    use super::*;

    #[test]
    fn messages_are_tagged() {
        let json = serde_json::to_value(sample_hello()).unwrap();
        assert_eq!(json["type"], "hello");
        assert_eq!(json["band"]["bins"], 256);
        let json = serde_json::to_value(sample_frame()).unwrap();
        assert_eq!(json["type"], "frame");
        assert_eq!(json["tracks"][0]["kind"], "fhss_control");
    }

    #[test]
    fn rounding_keeps_one_decimal() {
        assert_eq!(round_tenth(-95.3449), -95.3);
        assert_eq!(serde_json::to_string(&round_tenth(-61.26)).unwrap(), "-61.3");
    }

    #[test]
    fn frames_carry_the_report() {
        let sweep = Sweep {
            t_ms: 300,
            power_dbm: vec![-96.04, -61.26],
        };
        let report = Report {
            noise_floor_dbm: -96.04,
            threshold_dbm: -86.04,
            tracks: vec![detect::Track {
                id: 7,
                kind: detect::TrackKind::Wideband,
                center_mhz: 2437.0,
                bandwidth_mhz: 18.0,
                peak_dbm: -61.26,
                confidence: 0.5,
                first_seen_ms: 100,
                last_seen_ms: 300,
                active: true,
            }],
        };
        let f = Frame::new(4, &sweep, &report);
        assert_eq!((f.seq, f.t_ms), (4, 300));
        assert_eq!(f.power_dbm, [-96.0, -61.3]);
        assert_eq!((f.noise_floor_dbm, f.threshold_dbm), (-96.0, -86.0));
        assert_eq!(f.tracks[0].kind, TrackKind::Wideband);
        assert_eq!(f.tracks[0].id, 7);
    }

    fn sample_tracks() -> Vec<Track> {
        vec![
            Track {
                id: 3,
                kind: TrackKind::FhssControl,
                center_mhz: 2441.8,
                bandwidth_mhz: 72.0,
                peak_dbm: -61.2,
                confidence: 0.9,
                first_seen_ms: 10_100,
                last_seen_ms: 81_200,
                active: true,
            },
            Track {
                id: 1,
                kind: TrackKind::Wideband,
                center_mhz: 2437.1,
                bandwidth_mhz: 19.4,
                peak_dbm: -62.4,
                confidence: 0.45,
                first_seen_ms: 200,
                last_seen_ms: 79_900,
                active: false,
            },
        ]
    }

    fn sample_hello() -> ServerMsg {
        ServerMsg::Hello(Hello {
            device: "rf-watch".into(),
            version: "0.1.0".into(),
            band: Band::new(&spectrum::Band::ISM_2G4, 10),
        })
    }

    fn sample_frame() -> ServerMsg {
        ServerMsg::Frame(Frame {
            seq: 812,
            t_ms: 81_200,
            noise_floor_dbm: -96.8,
            threshold_dbm: -86.8,
            power_dbm: vec![-95.9, -97.1, -61.2],
            tracks: sample_tracks(),
        })
    }

    fn sample_status() -> Status {
        Status {
            device: "rf-watch".into(),
            version: "0.1.0".into(),
            frames: 813,
            clients: 2,
            drone_detected: true,
            tracks: sample_tracks(),
        }
    }

    fn typescript() -> String {
        let cfg = Config::new().with_large_int("number");
        let decls = [
            Band::decl(&cfg),
            TrackKind::decl(&cfg),
            Track::decl(&cfg),
            Hello::decl(&cfg),
            Frame::decl(&cfg),
            ServerMsg::decl(&cfg),
            Status::decl(&cfg),
        ];
        let mut out = String::from(
            "// Generated from src/protocol.rs by `UPDATE_PROTOCOL=1 cargo test`. Do not edit.\n",
        );
        for d in decls {
            out.push_str("\nexport ");
            out.push_str(&inline_docs(&d));
            out.push('\n');
        }
        out
    }

    /// ts-rs puts field docs on lines of their own inside one-line types; keep them inline.
    fn inline_docs(decl: &str) -> String {
        decl.replace(" \n/**\n * ", " /** ").replace("\n */\n", " */ ")
    }

    fn json(value: &impl Serialize) -> String {
        serde_json::to_string_pretty(value).unwrap() + "\n"
    }

    /// Compares `content` with the committed file, or rewrites it with `UPDATE_PROTOCOL=1`.
    fn check_generated(path: &str, content: &str) {
        let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        if std::env::var_os("UPDATE_PROTOCOL").is_some() {
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, content).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(&full).unwrap_or_default();
        assert!(
            committed == content,
            "{path} is out of date with src/protocol.rs; run `UPDATE_PROTOCOL=1 cargo test` \
             and update the clients.\n--- expected ---\n{content}"
        );
    }

    #[test]
    fn typescript_types_are_up_to_date() {
        check_generated("web/src/lib/protocol.gen.ts", &typescript());
    }

    #[test]
    fn sample_messages_are_up_to_date() {
        check_generated("protocol/hello.json", &json(&sample_hello()));
        check_generated("protocol/frame.json", &json(&sample_frame()));
        check_generated("protocol/status.json", &json(&sample_status()));
    }
}
