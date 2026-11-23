//! A capture replayed from CSV gives the same detections as the live source it was
//! recorded from. The capture is written here in `hackrf_sweep` layout (slices out of
//! order, starting just before midnight) from the simulator, so the replay path is
//! checked end to end; it is not a substitute for a real recording.

use std::fmt::Write;

use rf_watch::detect::{Detector, DetectorConfig, Report};
use rf_watch::source::SweepSource;
use rf_watch::source::csv::CsvReplay;
use rf_watch::source::sim::{Scenario, Simulator};
use rf_watch::spectrum::{Band, Sweep};

const SWEEPS: usize = 600;
/// 23:59:30 on 2026-11-19, so the capture crosses midnight.
const START_MS_OF_DAY: u64 = (23 * 3600 + 59 * 60 + 30) * 1000;

fn record(band: &Band, sweeps: &[Sweep]) -> String {
    let bin_hz = f64::from(band.bin_mhz()) * 1e6;
    let start_hz = f64::from(band.start_mhz) * 1e6;
    let slice = band.bins / 4;
    let mut csv = String::new();
    for sweep in sweeps {
        let ms = START_MS_OF_DAY + sweep.t_ms;
        let (date, ms) = if ms < 86_400_000 {
            ("2026-11-19", ms)
        } else {
            ("2026-11-20", ms - 86_400_000)
        };
        let time = format!(
            "{:02}:{:02}:{:02}.{:03}000",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        );
        for k in [0, 2, 1, 3] {
            let bins = &sweep.power_dbm[k * slice..(k + 1) * slice];
            let low = start_hz + (k * slice) as f64 * bin_hz;
            let high = low + slice as f64 * bin_hz;
            let values: Vec<String> = bins.iter().map(f32::to_string).collect();
            writeln!(
                csv,
                "{date}, {time}, {low:.0}, {high:.0}, {bin_hz:.2}, 20, {}",
                values.join(", ")
            )
            .unwrap();
        }
    }
    csv
}

fn detect(band: Band, sweeps: impl IntoIterator<Item = Sweep>) -> Vec<Report> {
    let mut det = Detector::new(DetectorConfig::default(), band);
    sweeps.into_iter().map(|s| det.process(&s)).collect()
}

#[test]
fn a_replayed_capture_detects_like_the_live_source() {
    let mut sim = Simulator::new(7, Scenario::demo(), 10);
    let band = sim.band();
    let live: Vec<Sweep> = (0..SWEEPS).map(|_| sim.generate()).collect();
    let csv = record(&band, &live);

    let mut replay = CsvReplay::new(csv.as_bytes(), band);
    let mut replayed = Vec::new();
    while let Some(s) = replay.next_sweep().unwrap() {
        replayed.push(s);
    }
    assert_eq!(replayed.len(), SWEEPS);
    assert_eq!(
        replayed.last().unwrap().t_ms,
        live.last().unwrap().t_ms,
        "time across midnight"
    );

    let live_reports = detect(band, live);
    let replay_reports = detect(band, replayed);
    let detected = |r: &[Report]| r.iter().map(Report::drone_detected).collect::<Vec<_>>();
    assert_eq!(detected(&live_reports), detected(&replay_reports));
    assert!(replay_reports[150..350].iter().all(Report::drone_detected));
}
