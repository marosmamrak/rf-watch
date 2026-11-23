//! Wideband emitters (Wi-Fi, video downlinks), each tracked by its centre frequency.

use super::{Segment, Track, TrackIds, TrackKind};

#[derive(Debug, Clone)]
pub struct WidebandConfig {
    /// Segments at least this wide (MHz) are wideband signals.
    pub min_width_mhz: f32,
    /// A segment centred within this distance (MHz) of a track updates that track.
    pub match_mhz: f32,
    /// Weight of a new observation in the moving average of centre and bandwidth.
    pub smoothing: f32,
    /// Weight of each sweep in the moving hit rate, which is the track's confidence.
    pub rate_weight: f32,
    /// A track is reported once its hit rate reaches this.
    pub min_confidence: f32,
    /// A track is active while it was seen within this many ms.
    pub active_ms: u64,
}

impl Default for WidebandConfig {
    fn default() -> Self {
        Self {
            min_width_mhz: 8.0,
            match_mhz: 3.0,
            smoothing: 0.2,
            rate_weight: 0.1,
            min_confidence: 0.2,
            active_ms: 2_000,
        }
    }
}

struct Candidate {
    track: Track,
    /// Moving average of "seen in this sweep".
    rate: f32,
    /// Once confident enough, the track is reported until it is dropped.
    reported: bool,
}

pub(super) struct WidebandTracker {
    cfg: WidebandConfig,
    hold_ms: u64,
    candidates: Vec<Candidate>,
}

impl WidebandTracker {
    pub fn new(cfg: WidebandConfig, hold_ms: u64) -> Self {
        Self {
            cfg,
            hold_ms,
            candidates: Vec::new(),
        }
    }

    pub fn update(&mut self, segs: &[Segment], now: u64, ids: &mut TrackIds) {
        let cfg = &self.cfg;
        let mut seen = vec![false; self.candidates.len()];
        for seg in segs.iter().filter(|s| s.width_mhz >= cfg.min_width_mhz) {
            let matched = self
                .candidates
                .iter()
                .position(|c| (c.track.center_mhz - seg.center_mhz).abs() < cfg.match_mhz);
            match matched {
                Some(i) => {
                    let t = &mut self.candidates[i].track;
                    t.center_mhz += cfg.smoothing * (seg.center_mhz - t.center_mhz);
                    t.bandwidth_mhz += cfg.smoothing * (seg.width_mhz - t.bandwidth_mhz);
                    t.peak_dbm = seg.peak_dbm;
                    t.last_seen_ms = now;
                    seen[i] = true;
                }
                None => {
                    self.candidates.push(Candidate {
                        track: Track {
                            id: ids.next(),
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
                    seen.push(true);
                }
            }
        }

        for (c, hit) in self.candidates.iter_mut().zip(seen) {
            let w = cfg.rate_weight;
            c.rate = (1.0 - w) * c.rate + if hit { w } else { 0.0 };
            c.track.confidence = c.rate;
            c.track.active = now.saturating_sub(c.track.last_seen_ms) <= cfg.active_ms;
            c.reported |= c.rate >= cfg.min_confidence;
        }
        let hold_ms = self.hold_ms;
        self.candidates
            .retain(|c| now.saturating_sub(c.track.last_seen_ms) <= hold_ms);
    }

    /// Tracks confident enough to report, active or recently lost.
    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.candidates.iter().filter(|c| c.reported).map(|c| &c.track)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{detector, sweep};
    use super::*;

    const WIFI: (f32, f32, f32) = (2437.0, 20.0, -60.0);

    fn wideband(r: &super::super::Report) -> Vec<&Track> {
        r.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Wideband)
            .collect()
    }

    #[test]
    fn a_steady_emitter_is_reported_after_a_few_hits() {
        let mut det = detector();
        // Hit rate after n sweeps is 1 - 0.9^n: 0.19 after two, 0.27 after three.
        assert!(wideband(&det.process(&sweep(0, &[WIFI]))).is_empty());
        assert!(wideband(&det.process(&sweep(100, &[WIFI]))).is_empty());
        let r = det.process(&sweep(200, &[WIFI]));
        let t = wideband(&r)[0];
        assert!(t.active);
        assert!((t.center_mhz - 2437.0).abs() < 0.5, "{t:?}");
        assert!((t.bandwidth_mhz - 20.0).abs() < 1.0, "{t:?}");
        assert!((t.confidence - 0.271).abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn a_lost_emitter_goes_inactive_then_is_dropped() {
        let mut det = detector();
        for t in (0..1000).step_by(100) {
            det.process(&sweep(t, &[WIFI]));
        }
        // Last seen at 900 ms.
        assert!(wideband(&det.process(&sweep(2_900, &[])))[0].active);
        assert!(!wideband(&det.process(&sweep(3_000, &[])))[0].active);
        assert_eq!(wideband(&det.process(&sweep(10_900, &[]))).len(), 1);
        assert!(wideband(&det.process(&sweep(11_000, &[]))).is_empty());
    }

    #[test]
    fn a_drifting_emitter_keeps_its_track_and_a_distant_one_gets_its_own() {
        let mut det = detector();
        for i in 0..5 {
            det.process(&sweep(i * 100, &[WIFI]));
        }
        let r = det.process(&sweep(500, &[(2438.5, 20.0, -60.0)]));
        let id = wideband(&r)[0].id;
        let mut last = r;
        for i in 6..12 {
            last = det.process(&sweep(i * 100, &[(2438.5, 20.0, -60.0), (2470.0, 10.0, -60.0)]));
        }
        let tracks = wideband(&last);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].id, id);
        assert_ne!(tracks[1].id, id);
    }

    #[test]
    fn narrow_bursts_are_not_wideband() {
        let mut det = detector();
        for i in 0..20 {
            assert!(wideband(&det.process(&sweep(i * 100, &[(2450.0, 1.0, -60.0)]))).is_empty());
        }
    }
}
