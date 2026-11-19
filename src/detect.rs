//! Detection on power-spectrum sweeps.
//!
//! 1. Estimate the noise floor (25th percentile of the sweep) and threshold above it.
//! 2. Split the sweep into contiguous segments above the threshold.
//! 3. Wide segments are tracked by centre frequency (Wi-Fi, analog video...).
//! 4. Narrow segments are pooled over a sliding window: many short bursts per sweep
//!    spread over many distinct channels is the signature of a frequency-hopping (FHSS)
//!    remote-control link. Isolated blips are not.

use std::collections::{BTreeSet, VecDeque};

use serde::Serialize;

use crate::sim::{Sweep, bin_mhz, freq_of_bin};

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Detection threshold above the noise floor, dB.
    pub threshold_db: f32,
    /// Sweeps in the FHSS sliding window.
    pub window: usize,
    /// Mean narrow bursts per sweep needed for an FHSS detection.
    pub fhss_min_rate: f32,
    /// Distinct 1 MHz channels hit within the window needed for an FHSS detection.
    pub fhss_min_channels: usize,
    /// Segments at least this wide (MHz) are wideband signals.
    pub wide_min_mhz: f32,
    /// Segments at most this wide (MHz) are narrowband bursts.
    pub narrow_max_mhz: f32,
    /// A wideband track is reported once its hit rate reaches this.
    pub wide_min_confidence: f32,
    /// While an FHSS link is active, the window only has to meet this fraction of the
    /// rate and channel thresholds to keep it (hysteresis against flapping).
    pub fhss_release_factor: f32,
    /// ...and it is reported lost only after this long without such a window.
    pub fhss_release_ms: u64,
    /// Lost tracks stay listed (inactive) this long.
    pub hold_ms: u64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            threshold_db: 10.0,
            window: 20,
            fhss_min_rate: 2.5,
            fhss_min_channels: 15,
            wide_min_mhz: 8.0,
            narrow_max_mhz: 2.5,
            wide_min_confidence: 0.2,
            fhss_release_factor: 0.6,
            fhss_release_ms: 1_000,
            hold_ms: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    /// Frequency-hopping remote-control link (typical for drones).
    FhssControl,
    /// Wideband emitter: Wi-Fi, analog or digital video downlink.
    Wideband,
}

#[derive(Debug, Clone, Serialize)]
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

/// Contiguous run of bins above the threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub center_mhz: f32,
    pub width_mhz: f32,
    pub peak_dbm: f32,
}

/// Result of processing one sweep.
#[derive(Debug, Clone)]
pub struct Report {
    pub noise_floor_dbm: f32,
    pub threshold_dbm: f32,
    pub tracks: Vec<Track>,
}

/// 25th-percentile power: robust against signals occupying part of the band.
pub fn noise_floor(power_dbm: &[f32]) -> f32 {
    let mut sorted = power_dbm.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[sorted.len() / 4]
}

pub fn segments(power_dbm: &[f32], threshold_dbm: f32) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..=power_dbm.len() {
        let above = power_dbm.get(i).is_some_and(|&p| p > threshold_dbm);
        match (start, above) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                let run = &power_dbm[s..i];
                out.push(Segment {
                    center_mhz: (freq_of_bin(s) + freq_of_bin(i - 1)) / 2.0,
                    width_mhz: run.len() as f32 * bin_mhz(),
                    peak_dbm: run.iter().copied().fold(f32::MIN, f32::max),
                });
                start = None;
            }
            _ => {}
        }
    }
    out
}

struct WideState {
    track: Track,
    /// Exponential moving average of "seen in this sweep".
    rate: f32,
    reported: bool,
}

pub struct Detector {
    cfg: DetectorConfig,
    next_id: u32,
    wide: Vec<WideState>,
    fhss: Option<Track>,
    /// Narrow bursts per sweep: (centre MHz, peak dBm).
    narrow: VecDeque<Vec<(f32, f32)>>,
}

impl Detector {
    pub fn new(cfg: DetectorConfig) -> Self {
        Self {
            cfg,
            next_id: 1,
            wide: Vec::new(),
            fhss: None,
            narrow: VecDeque::new(),
        }
    }

    pub fn process(&mut self, sweep: &Sweep) -> Report {
        let now = sweep.t_ms;
        let floor = noise_floor(&sweep.power_dbm);
        let threshold = floor + self.cfg.threshold_db;
        let segs = segments(&sweep.power_dbm, threshold);

        self.update_wide(&segs, now);
        self.update_fhss(&segs, now);

        let mut tracks: Vec<Track> = self.fhss.iter().cloned().collect();
        tracks.extend(self.wide.iter().filter(|w| w.reported).map(|w| w.track.clone()));
        tracks.sort_by(|a, b| b.active.cmp(&a.active).then(a.id.cmp(&b.id)));

        Report {
            noise_floor_dbm: floor,
            threshold_dbm: threshold,
            tracks,
        }
    }

    fn update_wide(&mut self, segs: &[Segment], now: u64) {
        let mut seen = vec![false; self.wide.len()];
        for seg in segs.iter().filter(|s| s.width_mhz >= self.cfg.wide_min_mhz) {
            match self
                .wide
                .iter()
                .position(|w| (w.track.center_mhz - seg.center_mhz).abs() < 3.0)
            {
                Some(i) => {
                    let t = &mut self.wide[i].track;
                    t.center_mhz += 0.2 * (seg.center_mhz - t.center_mhz);
                    t.bandwidth_mhz += 0.2 * (seg.width_mhz - t.bandwidth_mhz);
                    t.peak_dbm = seg.peak_dbm;
                    t.last_seen_ms = now;
                    seen[i] = true;
                }
                None => {
                    self.wide.push(WideState {
                        track: Track {
                            id: self.next_id,
                            kind: TrackKind::Wideband,
                            center_mhz: seg.center_mhz,
                            bandwidth_mhz: seg.width_mhz,
                            peak_dbm: seg.peak_dbm,
                            confidence: 0.0,
                            first_seen_ms: now,
                            last_seen_ms: now,
                            active: true,
                        },
                        rate: 0.0,
                        reported: false,
                    });
                    self.next_id += 1;
                    seen.push(true);
                }
            }
        }

        let cfg = &self.cfg;
        for (w, hit) in self.wide.iter_mut().zip(seen) {
            w.rate = 0.9 * w.rate + if hit { 0.1 } else { 0.0 };
            w.track.confidence = w.rate;
            w.track.active = now - w.track.last_seen_ms <= 2_000;
            w.reported |= w.rate >= cfg.wide_min_confidence;
        }
        self.wide.retain(|w| now - w.track.last_seen_ms <= cfg.hold_ms);
    }

    fn update_fhss(&mut self, segs: &[Segment], now: u64) {
        let bursts = segs
            .iter()
            .filter(|s| s.width_mhz <= self.cfg.narrow_max_mhz)
            .map(|s| (s.center_mhz, s.peak_dbm))
            .collect();
        self.narrow.push_back(bursts);
        while self.narrow.len() > self.cfg.window {
            self.narrow.pop_front();
        }

        // Hysteresis: an active link is kept while the window meets a lower bar
        // (fhss_release_factor of the detection thresholds).
        let holding = self.fhss.as_ref().is_some_and(|t| t.active);
        let factor = if holding {
            self.cfg.fhss_release_factor
        } else {
            1.0
        };
        let hit = self.narrow.len() == self.cfg.window && {
            let (rate, channels, _, _, _) = self.window_stats();
            rate >= factor * self.cfg.fhss_min_rate
                && channels as f32 >= factor * self.cfg.fhss_min_channels as f32
        };

        if hit {
            let (rate, channels, lo, hi, peak) = self.window_stats();
            let confidence = 0.5 * (rate / (2.0 * self.cfg.fhss_min_rate)).min(1.0)
                + 0.5 * (channels as f32 / (2.0 * self.cfg.fhss_min_channels as f32)).min(1.0);
            let track = self.fhss.get_or_insert_with(|| {
                let id = self.next_id;
                self.next_id += 1;
                Track {
                    id,
                    kind: TrackKind::FhssControl,
                    center_mhz: 0.0,
                    bandwidth_mhz: 0.0,
                    peak_dbm: peak,
                    confidence: 0.0,
                    first_seen_ms: now,
                    last_seen_ms: now,
                    active: true,
                }
            });
            track.center_mhz = (lo + hi) / 2.0;
            track.bandwidth_mhz = hi - lo;
            track.peak_dbm = peak;
            track.confidence = confidence;
            track.last_seen_ms = now;
            track.active = true;
        } else if let Some(track) = &mut self.fhss {
            track.active = now - track.last_seen_ms <= self.cfg.fhss_release_ms;
            track.confidence *= 0.95;
            if now - track.last_seen_ms > self.cfg.hold_ms {
                self.fhss = None;
            }
        }
    }

    /// (bursts per sweep, distinct 1 MHz channels, lowest MHz, highest MHz, peak dBm).
    /// The peak is taken over the last few sweeps, so it follows the signal level
    /// without dropping out when a single sweep happens to catch no burst.
    fn window_stats(&self) -> (f32, usize, f32, f32, f32) {
        const PEAK_SWEEPS: usize = 5;
        let total: usize = self.narrow.iter().map(Vec::len).sum();
        let all = self.narrow.iter().flatten();
        let channels: BTreeSet<i32> = all.clone().map(|&(f, _)| f.round() as i32).collect();
        let lo = all.clone().map(|&(f, _)| f).fold(f32::MAX, f32::min);
        let hi = all.clone().map(|&(f, _)| f).fold(f32::MIN, f32::max);
        let recent = self.narrow.iter().rev().take(PEAK_SWEEPS).flatten();
        let peak = recent
            .map(|&(_, p)| p)
            .reduce(f32::max)
            .or_else(|| all.map(|&(_, p)| p).reduce(f32::max))
            .unwrap_or(f32::NAN);
        (
            total as f32 / self.narrow.len() as f32,
            channels.len(),
            lo,
            hi,
            peak,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Scenario, Simulator};

    fn run(scenario: Scenario, sweeps: usize) -> Vec<Report> {
        let mut sim = Simulator::new(42, scenario, 10);
        let mut det = Detector::new(DetectorConfig::default());
        (0..sweeps).map(|_| det.process(&sim.next_sweep())).collect()
    }

    fn fhss_active(r: &Report) -> bool {
        r.tracks
            .iter()
            .any(|t| t.kind == TrackKind::FhssControl && t.active)
    }

    #[test]
    fn segments_finds_runs_above_threshold() {
        let mut p = vec![-100.0; 20];
        p[3] = -50.0;
        p[10..15].fill(-60.0);
        let segs = segments(&p, -90.0);
        assert_eq!(segs.len(), 2);
        assert!((segs[0].width_mhz - bin_mhz()).abs() < 1e-4);
        assert!((segs[1].width_mhz - 5.0 * bin_mhz()).abs() < 1e-4);
        assert_eq!(segs[1].peak_dbm, -60.0);
    }

    #[test]
    fn segment_touching_the_band_edge_is_closed() {
        let mut p = vec![-100.0; 10];
        p[8..].fill(-50.0);
        assert_eq!(segments(&p, -90.0).len(), 1);
    }

    #[test]
    fn noise_floor_ignores_a_strong_signal() {
        let mut p = vec![-96.0; 100];
        p[40..70].fill(-50.0);
        assert_eq!(noise_floor(&p), -96.0);
    }

    #[test]
    fn noise_only_gives_no_tracks() {
        assert!(
            run(Scenario::noise_only(), 200)
                .iter()
                .all(|r| r.tracks.is_empty())
        );
    }

    #[test]
    fn background_traffic_is_not_a_drone() {
        let reports = run(Scenario::quiet(), 300);
        assert!(!reports.iter().any(fhss_active));
        let last = reports.last().unwrap();
        let wifi = last
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Wideband)
            .expect("wifi track");
        assert!((wifi.center_mhz - 2437.0).abs() < 2.0, "{wifi:?}");
        assert!((wifi.bandwidth_mhz - 18.0).abs() < 3.0, "{wifi:?}");
    }

    #[test]
    fn drone_link_is_detected_within_the_window() {
        let reports = run(Scenario::drone(), 60);
        let first = reports.iter().position(fhss_active).expect("detected");
        assert!(first < 25, "detected at sweep {first}");
        let t = reports[59]
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::FhssControl)
            .unwrap();
        assert!(t.confidence > 0.7, "{t:?}");
        assert!(t.bandwidth_mhz > 40.0, "hop span {t:?}");
    }

    #[test]
    fn reported_peaks_are_real_levels() {
        // Regression: a sweep that caught no burst used to set the peak to f32::MIN.
        for r in run(Scenario::demo(), 600) {
            for t in &r.tracks {
                assert!((-100.0..-30.0).contains(&t.peak_dbm), "{t:?}");
            }
        }
    }

    #[test]
    fn detection_does_not_flap_during_a_flight() {
        // Regression: a weak sweep window in mid-flight dropped the link for a few
        // sweeps, so clients saw "lost" + "detected" again within half a second.
        for seed in 1..=40 {
            let mut sim = Simulator::new(seed, Scenario::demo(), 10);
            let mut det = Detector::new(DetectorConfig::default());
            let active: Vec<bool> = (0..1200)
                .map(|_| fhss_active(&det.process(&sim.next_sweep())))
                .collect();
            let losses = active.windows(2).filter(|w| w[0] && !w[1]).count();
            assert_eq!(
                losses, 2,
                "seed {seed}: expected one loss per 60 s loop, got {losses}"
            );
        }
    }

    #[test]
    fn demo_drone_comes_and_goes() {
        // 60 s loop at 10 fps: drone on air 8..40 s.
        let reports = run(Scenario::demo(), 600);
        assert!(!reports[..80].iter().any(fhss_active), "false alarm before 8 s");
        assert!(reports[150..350].iter().all(fhss_active), "missed while overhead");
        assert!(!fhss_active(&reports[450]), "still active after it left");
        // ...but listed as lost for a while, then dropped
        assert!(
            reports[430]
                .tracks
                .iter()
                .any(|t| t.kind == TrackKind::FhssControl)
        );
        assert!(
            !reports[599]
                .tracks
                .iter()
                .any(|t| t.kind == TrackKind::FhssControl)
        );
    }
}
