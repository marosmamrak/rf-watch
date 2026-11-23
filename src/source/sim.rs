//! Simulated passive RF front end: power-spectrum sweeps of the 2.4 GHz ISM band.
//!
//! Stands in for an SDR (e.g. an `rtl_power`/HackRF sweep). Each sweep contains a noise
//! floor plus, depending on the [`Scenario`], a bursty Wi-Fi channel, random short
//! narrowband blips and a drone remote-control link that hops across ~50 channels.

use std::f32::consts::PI;

use anyhow::Result;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::SweepSource;
use crate::spectrum::{Band, Sweep};

pub const NOISE_FLOOR_DBM: f32 = -96.0;

/// What the simulated environment contains.
#[derive(Debug, Clone)]
pub struct Scenario {
    /// A Wi-Fi access point on channel 6, transmitting in bursts.
    pub wifi: bool,
    /// Occasional single narrowband blips (Bluetooth, microwave clicks...).
    pub blips: bool,
    /// The drone control link is on air between these seconds of every cycle.
    pub drone: Option<(f32, f32)>,
    /// When true the drone flies in and out (signal rises, then fades).
    pub approach: bool,
    pub cycle_s: f32,
}

impl Scenario {
    /// A minute-long loop: background traffic, a drone appears at 8 s and leaves at 40 s.
    pub fn demo() -> Self {
        Self {
            wifi: true,
            blips: true,
            drone: Some((8.0, 40.0)),
            approach: true,
            cycle_s: 60.0,
        }
    }

    /// Background traffic only.
    pub fn quiet() -> Self {
        Self {
            wifi: true,
            blips: true,
            drone: None,
            approach: false,
            cycle_s: 60.0,
        }
    }

    /// A drone link that is always on, at a steady level.
    pub fn drone() -> Self {
        Self {
            wifi: true,
            blips: true,
            drone: Some((0.0, f32::MAX)),
            approach: false,
            cycle_s: f32::MAX,
        }
    }

    /// Nothing but thermal noise.
    pub fn noise_only() -> Self {
        Self {
            wifi: false,
            blips: false,
            drone: None,
            approach: false,
            cycle_s: 60.0,
        }
    }
}

/// Number of hop channels of the simulated control link, 1.5 MHz apart from 2405 MHz.
pub const HOP_CHANNELS: u32 = 50;
/// Bursts of the control link captured in one sweep.
pub const HOPS_PER_SWEEP: u32 = 4;

pub struct Simulator {
    band: Band,
    rng: StdRng,
    scenario: Scenario,
    frame: u64,
    fps: u64,
}

impl Simulator {
    pub fn new(seed: u64, scenario: Scenario, fps: u32) -> Self {
        Self {
            band: Band::ISM_2G4,
            rng: StdRng::seed_from_u64(seed),
            scenario,
            frame: 0,
            fps: u64::from(fps.max(1)),
        }
    }

    /// Whether the drone link is on air at `t` seconds.
    pub fn drone_on_air(&self, t: f32) -> bool {
        self.scenario.drone.is_some_and(|(a, b)| {
            let ct = t % self.scenario.cycle_s;
            ct >= a && ct < b
        })
    }

    /// Generates the next sweep; simulated time advances by one frame per call.
    pub fn generate(&mut self) -> Sweep {
        // From the frame count rather than a whole-ms frame period, so time keeps pace with
        // the scanner at any fps (1000 / fps truncates: 0 ms per frame above 1000 fps).
        let t_ms = self.frame * 1000 / self.fps;
        self.frame += 1;
        let t = t_ms as f32 / 1000.0;

        // Sum signals in linear power (mW), convert back to dBm at the end.
        let band = self.band;
        let mut lin: Vec<f32> = (0..band.bins)
            .map(|_| dbm_to_mw(NOISE_FLOOR_DBM + 1.2 * self.gauss()))
            .collect();

        if self.scenario.wifi && self.rng.random_bool(0.45) {
            let level = -62.0 + 2.0 * self.gauss();
            add_signal(&band, &mut lin, 2437.0, 18.0, level);
        }

        if self.scenario.blips && self.rng.random_bool(0.35) {
            let f = self.rng.random_range(2402.0..2481.0);
            let level = self.rng.random_range(-82.0..-72.0);
            add_signal(&band, &mut lin, f, 1.0, level);
        }

        if self.drone_on_air(t) {
            let (a, b) = self.scenario.drone.unwrap_or_default();
            let level = if self.scenario.approach {
                let progress = (t % self.scenario.cycle_s - a) / (b - a);
                -80.0 + 22.0 * (PI * progress).sin()
            } else {
                -66.0
            };
            for _ in 0..HOPS_PER_SWEEP {
                let ch = self.rng.random_range(0..HOP_CHANNELS);
                let f = 2405.0 + ch as f32 * 1.5;
                let burst = level + 2.0 * self.gauss();
                add_signal(&band, &mut lin, f, 1.0, burst);
            }
        }

        Sweep {
            t_ms,
            power_dbm: lin.into_iter().map(mw_to_dbm).collect(),
        }
    }

    /// Standard normal sample (Box–Muller).
    fn gauss(&mut self) -> f32 {
        let u1: f32 = self.rng.random_range(f32::EPSILON..1.0);
        let u2: f32 = self.rng.random();
        (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
    }
}

impl SweepSource for Simulator {
    fn band(&self) -> Band {
        self.band
    }

    fn next_sweep(&mut self) -> Result<Option<Sweep>> {
        Ok(Some(self.generate()))
    }
}

/// Adds a flat-topped signal of `width_mhz` with steep skirts at `center_mhz`.
fn add_signal(band: &Band, lin: &mut [f32], center_mhz: f32, width_mhz: f32, level_dbm: f32) {
    let bin = band.bin_mhz();
    for (i, p) in lin.iter_mut().enumerate() {
        let d = (band.freq_of_bin(i) - center_mhz).abs() - width_mhz / 2.0;
        let dbm = if d <= 0.0 {
            level_dbm
        } else if d < 3.0 * bin {
            level_dbm - 15.0 * d / bin
        } else {
            continue;
        };
        *p += dbm_to_mw(dbm);
    }
}

pub fn dbm_to_mw(dbm: f32) -> f32 {
    10f32.powf(dbm / 10.0)
}

pub fn mw_to_dbm(mw: f32) -> f32 {
    10.0 * mw.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeps_have_the_band_and_advance_in_time() {
        let mut sim = Simulator::new(1, Scenario::demo(), 10);
        let a = sim.generate();
        let b = sim.generate();
        assert_eq!(a.power_dbm.len(), sim.band().bins);
        assert_eq!(b.t_ms - a.t_ms, 100);
    }

    #[test]
    fn time_keeps_pace_at_any_rate() {
        // 1000 / fps would be 333 ms per frame at 3 fps (drifting) and 0 above 1000 fps.
        for (fps, frames, ms) in [(3, 3, 1_000), (7, 70, 10_000), (2_000, 2_000, 1_000)] {
            let mut sim = Simulator::new(1, Scenario::noise_only(), fps);
            let t = (0..=frames).map(|_| sim.generate().t_ms).last().unwrap();
            assert_eq!(t, ms, "{fps} fps");
        }
    }

    #[test]
    fn noise_only_stays_near_the_floor() {
        let mut sim = Simulator::new(2, Scenario::noise_only(), 10);
        for _ in 0..50 {
            let s = sim.generate();
            let max = s.power_dbm.iter().copied().fold(f32::MIN, f32::max);
            assert!(max < NOISE_FLOOR_DBM + 8.0, "max {max}");
        }
    }

    #[test]
    fn demo_drone_follows_the_cycle() {
        let sim = Simulator::new(3, Scenario::demo(), 10);
        assert!(!sim.drone_on_air(5.0));
        assert!(sim.drone_on_air(20.0));
        assert!(!sim.drone_on_air(45.0));
        assert!(sim.drone_on_air(80.0));
    }

    #[test]
    fn same_seed_same_sweeps() {
        let mut a = Simulator::new(7, Scenario::demo(), 10);
        let mut b = Simulator::new(7, Scenario::demo(), 10);
        for _ in 0..5 {
            assert_eq!(a.generate(), b.generate());
        }
    }
}
