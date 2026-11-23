//! Frequency-hopping (FHSS) control links. A single burst says nothing, so narrow bursts
//! are pooled over a sliding window of sweeps: many bursts per sweep spread over many
//! distinct channels is the signature of a hopping remote control; isolated blips
//! (Bluetooth advertising, clicks) are not.

use std::collections::{BTreeSet, VecDeque};

use super::{Segment, Track, TrackIds, TrackKind};

#[derive(Debug, Clone)]
pub struct FhssConfig {
    /// Segments at most this wide (MHz) are narrowband bursts.
    pub max_width_mhz: f32,
    /// Sweeps in the sliding window.
    pub window: usize,
    /// Mean bursts per sweep needed for a detection.
    pub min_rate: f32,
    /// Distinct channels hit within the window needed for a detection.
    pub min_channels: usize,
    /// Channel width (MHz) used to count distinct channels.
    pub channel_mhz: f32,
    /// While a link is active, the window only has to meet this fraction of `min_rate`
    /// and `min_channels` to keep it (hysteresis against flapping)...
    pub release_factor: f32,
    /// ...and the link is reported lost only after this long (ms) without such a window.
    pub release_ms: u64,
    /// Per-sweep decay of the confidence of a link that is no longer confirmed.
    pub confidence_decay: f32,
    /// The reported peak level is the strongest burst of this many most recent sweeps.
    pub peak_sweeps: usize,
}

impl Default for FhssConfig {
    fn default() -> Self {
        Self {
            max_width_mhz: 2.5,
            window: 20,
            min_rate: 2.5,
            min_channels: 15,
            channel_mhz: 1.0,
            release_factor: 0.6,
            release_ms: 1_000,
            confidence_decay: 0.95,
            peak_sweeps: 5,
        }
    }
}

/// Confidence reaches 1 when the window shows this multiple of both detection thresholds.
const CONFIDENCE_SATURATION: f32 = 2.0;

#[derive(Debug, Clone, Copy)]
struct Burst {
    center_mhz: f32,
    peak_dbm: f32,
}

/// What the bursts in a full window add up to.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WindowStats {
    /// Mean bursts per sweep.
    rate: f32,
    /// Distinct channels hit.
    channels: usize,
    lo_mhz: f32,
    hi_mhz: f32,
    /// Strongest burst of the most recent sweeps, so the level follows the signal
    /// without dropping out when a single sweep happens to catch no burst.
    peak_dbm: f32,
}

pub(super) struct FhssDetector {
    cfg: FhssConfig,
    hold_ms: u64,
    /// Narrow bursts of each sweep in the window, oldest first.
    window: VecDeque<Vec<Burst>>,
    track: Option<Track>,
}

impl FhssDetector {
    pub fn new(cfg: FhssConfig, hold_ms: u64) -> Self {
        Self {
            cfg,
            hold_ms,
            window: VecDeque::new(),
            track: None,
        }
    }

    pub fn update(&mut self, segs: &[Segment], now: u64, ids: &mut TrackIds) {
        let bursts = segs
            .iter()
            .filter(|s| s.width_mhz <= self.cfg.max_width_mhz)
            .map(|s| Burst {
                center_mhz: s.center_mhz,
                peak_dbm: s.peak_dbm,
            })
            .collect();
        self.window.push_back(bursts);
        while self.window.len() > self.cfg.window {
            self.window.pop_front();
        }

        let holding = self.track.as_ref().is_some_and(|t| t.active);
        let factor = if holding { self.cfg.release_factor } else { 1.0 };
        match self.stats().filter(|s| self.meets(s, factor)) {
            Some(stats) => self.confirm(&stats, now, ids),
            None => self.miss(now),
        }
    }

    pub fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }

    /// `None` until the window is full, or when it holds no burst at all.
    fn stats(&self) -> Option<WindowStats> {
        if self.window.len() < self.cfg.window {
            return None;
        }
        let all = || self.window.iter().flatten();
        let total = all().count();
        let channels: BTreeSet<i32> = all()
            .map(|b| (b.center_mhz / self.cfg.channel_mhz).round() as i32)
            .collect();
        let peak_dbm = self
            .window
            .iter()
            .rev()
            .take(self.cfg.peak_sweeps)
            .flatten()
            .map(|b| b.peak_dbm)
            .reduce(f32::max)
            .or_else(|| all().map(|b| b.peak_dbm).reduce(f32::max))?;
        Some(WindowStats {
            rate: total as f32 / self.window.len() as f32,
            channels: channels.len(),
            lo_mhz: all().map(|b| b.center_mhz).reduce(f32::min)?,
            hi_mhz: all().map(|b| b.center_mhz).reduce(f32::max)?,
            peak_dbm,
        })
    }

    /// Whether the window reaches `factor` times both detection thresholds.
    fn meets(&self, s: &WindowStats, factor: f32) -> bool {
        s.rate >= factor * self.cfg.min_rate && s.channels as f32 >= factor * self.cfg.min_channels as f32
    }

    fn confirm(&mut self, s: &WindowStats, now: u64, ids: &mut TrackIds) {
        let rate_score = s.rate / (CONFIDENCE_SATURATION * self.cfg.min_rate);
        let channel_score = s.channels as f32 / (CONFIDENCE_SATURATION * self.cfg.min_channels as f32);
        let track = self.track.get_or_insert_with(|| Track {
            id: ids.next(),
            kind: TrackKind::FhssControl,
            center_mhz: 0.0,
            bandwidth_mhz: 0.0,
            peak_dbm: s.peak_dbm,
            confidence: 0.0,
            first_seen_ms: now,
            last_seen_ms: now,
            active: true,
        });
        track.center_mhz = (s.lo_mhz + s.hi_mhz) / 2.0;
        track.bandwidth_mhz = s.hi_mhz - s.lo_mhz;
        track.peak_dbm = s.peak_dbm;
        track.confidence = 0.5 * rate_score.min(1.0) + 0.5 * channel_score.min(1.0);
        track.last_seen_ms = now;
        track.active = true;
    }

    fn miss(&mut self, now: u64) {
        let Some(track) = &mut self.track else { return };
        let since = now.saturating_sub(track.last_seen_ms);
        track.active = since <= self.cfg.release_ms;
        track.confidence *= self.cfg.confidence_decay;
        if since > self.hold_ms {
            self.track = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{detector, sweep};
    use super::super::{Detector, Report};
    use super::*;

    /// Deterministic hop pattern: `hops` bursts per sweep over `channels` channels 1.5 MHz
    /// apart from 2405 MHz, at `level` dBm.
    fn hopping(sweep_no: u64, hops: u64, channels: u64, level: f32) -> Vec<(f32, f32, f32)> {
        (0..hops)
            .map(|j| {
                let ch = (sweep_no * hops + j) * 7 % channels;
                (2405.0 + ch as f32 * 1.5, 1.0, level)
            })
            .collect()
    }

    fn link(r: &Report) -> Option<&Track> {
        r.tracks.iter().find(|t| t.kind == TrackKind::FhssControl)
    }

    /// Feeds sweeps `from..to` (100 ms apart) and returns the last report.
    fn feed(det: &mut Detector, from: u64, to: u64, signals: impl Fn(u64) -> Vec<(f32, f32, f32)>) -> Report {
        (from..to)
            .map(|i| det.process(&sweep(i * 100, &signals(i))))
            .last()
            .expect("at least one sweep")
    }

    #[test]
    fn a_hopping_link_is_detected_once_the_window_is_full() {
        let mut det = detector();
        let r = feed(&mut det, 0, 19, |i| hopping(i, 4, 50, -60.0));
        assert!(link(&r).is_none(), "detected before the window filled");

        let r = feed(&mut det, 19, 20, |i| hopping(i, 4, 50, -60.0));
        let t = link(&r).expect("detected");
        assert!(t.active && r.drone_detected());
        assert!(t.bandwidth_mhz > 70.0, "hop span {t:?}");
        assert!((t.center_mhz - 2441.75).abs() < 1.0, "{t:?}");
        assert_eq!(t.peak_dbm, -60.0);
        // Rate 4 of 5 for saturation, 50 channels (saturates at 30).
        assert!((t.confidence - 0.9).abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn busy_but_few_channels_is_not_a_hop_set() {
        let mut det = detector();
        let r = feed(&mut det, 0, 60, |i| hopping(i, 4, 3, -60.0));
        assert!(link(&r).is_none());
    }

    #[test]
    fn sparse_blips_over_many_channels_are_not_a_link() {
        let mut det = detector();
        let r = feed(&mut det, 0, 60, |i| hopping(i, 1, 50, -60.0));
        assert!(link(&r).is_none());
    }

    #[test]
    fn an_active_link_survives_a_weaker_stretch() {
        // Two hops per sweep is below the detection rate (2.5)...
        let mut fresh = detector();
        assert!(link(&feed(&mut fresh, 0, 60, |i| hopping(i, 2, 50, -60.0))).is_none());

        // ...but above the release rate (0.6 × 2.5) of a link that is already active.
        let mut det = detector();
        feed(&mut det, 0, 20, |i| hopping(i, 4, 50, -60.0));
        for i in 20..80 {
            let r = det.process(&sweep(i * 100, &hopping(i, 2, 50, -60.0)));
            assert!(link(&r).is_some_and(|t| t.active), "flapped at sweep {i}");
        }
    }

    #[test]
    fn a_silent_link_is_released_then_dropped() {
        let mut det = detector();
        let r = feed(&mut det, 0, 40, |i| hopping(i, 4, 50, -60.0));
        let id = link(&r).unwrap().id;

        let mut last_seen = 0;
        let mut released_at = None;
        let mut dropped_at = None;
        for i in 40..200 {
            let now = i * 100;
            let r = det.process(&sweep(now, &[]));
            match link(&r) {
                Some(t) if t.active => {
                    assert_eq!(t.id, id);
                    last_seen = t.last_seen_ms;
                }
                Some(_) => {
                    released_at.get_or_insert(now);
                }
                None => {
                    dropped_at.get_or_insert(now);
                }
            }
        }
        // The window still holds the link's bursts for a while after it stops, so
        // `last_seen` is later than the last burst; timing is relative to it.
        let released_after = released_at.expect("released") - last_seen;
        let dropped_after = dropped_at.expect("dropped") - last_seen;
        assert!(
            (1_001..=1_100).contains(&released_after),
            "released after {released_after} ms"
        );
        assert!(
            (10_001..=10_100).contains(&dropped_after),
            "dropped after {dropped_after} ms"
        );
    }

    #[test]
    fn the_reported_peak_follows_the_recent_bursts() {
        let mut det = detector();
        feed(&mut det, 0, 20, |i| hopping(i, 4, 50, -60.0));
        let r = feed(&mut det, 20, 30, |i| hopping(i, 4, 50, -70.0));
        assert_eq!(link(&r).unwrap().peak_dbm, -70.0);
    }
}
