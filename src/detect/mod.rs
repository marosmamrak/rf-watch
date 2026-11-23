//! Detection on power-spectrum sweeps.
//!
//! 1. [`segment`]: estimate the noise floor (25th percentile of the sweep), set the
//!    threshold above it and split the sweep into runs of bins above the threshold.
//! 2. [`wideband`]: wide segments are tracked one by one by centre frequency (Wi-Fi,
//!    video downlinks).
//! 3. [`fhss`]: narrow segments are pooled over a sliding window to recognise a
//!    frequency-hopping remote-control link.
//!
//! [`Detector`] runs the three steps and merges the tracks into a [`Report`].

mod fhss;
mod segment;
mod wideband;

pub use self::fhss::FhssConfig;
use self::fhss::FhssDetector;
pub use self::segment::{Segment, noise_floor, segments};
pub use self::wideband::WidebandConfig;
use self::wideband::WidebandTracker;
use crate::spectrum::{Band, Sweep};

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Detection threshold above the noise floor, dB.
    pub threshold_db: f32,
    /// Lost tracks stay listed (inactive) this long, ms.
    pub hold_ms: u64,
    pub wideband: WidebandConfig,
    pub fhss: FhssConfig,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            threshold_db: 10.0,
            hold_ms: 10_000,
            wideband: WidebandConfig::default(),
            fhss: FhssConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// Frequency-hopping remote-control link (typical for drones).
    FhssControl,
    /// Wideband emitter: Wi-Fi, analog or digital video downlink.
    Wideband,
}

#[derive(Debug, Clone, PartialEq)]
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

impl Track {
    /// An active hopping control link: what the node reports as a drone.
    pub fn is_drone_link(&self) -> bool {
        self.kind == TrackKind::FhssControl && self.active
    }
}

/// Result of processing one sweep.
#[derive(Debug, Clone)]
pub struct Report {
    pub noise_floor_dbm: f32,
    pub threshold_dbm: f32,
    /// Active tracks first, then by id.
    pub tracks: Vec<Track>,
}

impl Report {
    pub fn drone_detected(&self) -> bool {
        self.tracks.iter().any(Track::is_drone_link)
    }
}

/// Hands out track ids, unique across all trackers.
#[derive(Debug, Default)]
struct TrackIds(u32);

impl TrackIds {
    fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

pub struct Detector {
    band: Band,
    threshold_db: f32,
    ids: TrackIds,
    wideband: WidebandTracker,
    fhss: FhssDetector,
}

impl Detector {
    pub fn new(cfg: DetectorConfig, band: Band) -> Self {
        Self {
            band,
            threshold_db: cfg.threshold_db,
            ids: TrackIds::default(),
            wideband: WidebandTracker::new(cfg.wideband, cfg.hold_ms),
            fhss: FhssDetector::new(cfg.fhss, cfg.hold_ms),
        }
    }

    /// Processes one sweep of the detector's band.
    pub fn process(&mut self, sweep: &Sweep) -> Report {
        debug_assert_eq!(sweep.power_dbm.len(), self.band.bins, "sweep of another band");
        let floor = noise_floor(&sweep.power_dbm);
        let threshold = floor + self.threshold_db;
        let segs = segments(&self.band, &sweep.power_dbm, threshold);

        self.wideband.update(&segs, sweep.t_ms, &mut self.ids);
        self.fhss.update(&segs, sweep.t_ms, &mut self.ids);

        let mut tracks: Vec<Track> = self
            .fhss
            .track()
            .into_iter()
            .chain(self.wideband.tracks())
            .cloned()
            .collect();
        tracks.sort_by(|a, b| b.active.cmp(&a.active).then(a.id.cmp(&b.id)));

        Report {
            noise_floor_dbm: floor,
            threshold_dbm: threshold,
            tracks,
        }
    }
}

/// Hand-built sweeps for the detector's unit tests, independent of the simulator.
#[cfg(test)]
mod testing {
    use super::{Detector, DetectorConfig};
    use crate::spectrum::{Band, Sweep};

    pub const NOISE_DBM: f32 = -96.0;

    pub fn detector() -> Detector {
        Detector::new(DetectorConfig::default(), Band::ISM_2G4)
    }

    /// Flat noise with rectangular signals `(centre MHz, width MHz, level dBm)`.
    pub fn sweep(t_ms: u64, signals: &[(f32, f32, f32)]) -> Sweep {
        let band = Band::ISM_2G4;
        let power_dbm = (0..band.bins)
            .map(|i| {
                let f = band.freq_of_bin(i);
                signals
                    .iter()
                    .filter(|(center, width, _)| (f - center).abs() <= width / 2.0)
                    .map(|&(_, _, level)| level)
                    .fold(NOISE_DBM, f32::max)
            })
            .collect();
        Sweep { t_ms, power_dbm }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{NOISE_DBM, detector, sweep};
    use super::*;

    #[test]
    fn noise_alone_gives_no_tracks() {
        let mut det = detector();
        for i in 0..50 {
            let r = det.process(&sweep(i * 100, &[]));
            assert!(r.tracks.is_empty());
            assert_eq!(r.noise_floor_dbm, NOISE_DBM);
            assert_eq!(r.threshold_dbm, NOISE_DBM + 10.0);
        }
    }

    #[test]
    fn a_signal_below_the_threshold_is_ignored() {
        let mut det = detector();
        for i in 0..50 {
            let r = det.process(&sweep(i * 100, &[(2437.0, 20.0, NOISE_DBM + 9.0)]));
            assert!(r.tracks.is_empty());
        }
    }

    #[test]
    fn tracks_are_listed_active_first_then_by_id() {
        let mut det = detector();
        let a = (2420.0, 10.0, -60.0);
        let b = (2460.0, 10.0, -60.0);
        for i in 0..5 {
            det.process(&sweep(i * 100, &[a, b]));
        }
        // `a` stops; once it is inactive it is listed after `b` despite its lower id.
        let r = (5..40)
            .map(|i| det.process(&sweep(i * 100, &[b])))
            .last()
            .unwrap();
        let order: Vec<(u32, bool)> = r.tracks.iter().map(|t| (t.id, t.active)).collect();
        assert_eq!(order, [(2, true), (1, false)]);
    }

    #[test]
    fn only_an_active_hopping_link_is_a_drone() {
        let track = |kind, active| Track {
            id: 1,
            kind,
            center_mhz: 2440.0,
            bandwidth_mhz: 70.0,
            peak_dbm: -60.0,
            confidence: 0.9,
            first_seen_ms: 0,
            last_seen_ms: 0,
            active,
        };
        let report = |tracks| Report {
            noise_floor_dbm: NOISE_DBM,
            threshold_dbm: NOISE_DBM + 10.0,
            tracks,
        };
        assert!(report(vec![track(TrackKind::FhssControl, true)]).drone_detected());
        assert!(!report(vec![track(TrackKind::FhssControl, false)]).drone_detected());
        assert!(!report(vec![track(TrackKind::Wideband, true)]).drone_detected());
        assert!(!report(vec![]).drone_detected());
    }
}
