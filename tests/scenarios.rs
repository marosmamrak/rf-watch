//! Simulator → detector, end to end. These check that the detector behaves on the
//! simulated environments the demo runs; the detection rules themselves are unit-tested
//! on hand-built sweeps in `src/detect/`. Neither is a substitute for real captures.

use rf_watch::detect::{Detector, DetectorConfig, Report, TrackKind};
use rf_watch::source::SweepSource;
use rf_watch::source::sim::{Scenario, Simulator};

fn run(seed: u64, scenario: Scenario, sweeps: usize) -> Vec<Report> {
    let mut sim = Simulator::new(seed, scenario, 10);
    let mut det = Detector::new(DetectorConfig::default(), sim.band());
    (0..sweeps).map(|_| det.process(&sim.generate())).collect()
}

fn has_link(r: &Report) -> bool {
    r.tracks.iter().any(|t| t.kind == TrackKind::FhssControl)
}

#[test]
fn noise_only_gives_no_tracks() {
    assert!(
        run(42, Scenario::noise_only(), 200)
            .iter()
            .all(|r| r.tracks.is_empty())
    );
}

#[test]
fn background_traffic_is_not_a_drone() {
    let reports = run(42, Scenario::quiet(), 300);
    assert!(!reports.iter().any(Report::drone_detected));
    let wifi = reports
        .last()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Wideband)
        .cloned()
        .expect("wifi track");
    assert!((wifi.center_mhz - 2437.0).abs() < 2.0, "{wifi:?}");
    assert!((wifi.bandwidth_mhz - 18.0).abs() < 3.0, "{wifi:?}");
}

#[test]
fn an_always_on_link_is_detected_within_the_window() {
    let reports = run(42, Scenario::drone(), 60);
    let first = reports.iter().position(Report::drone_detected).expect("detected");
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
    for r in run(42, Scenario::demo(), 600) {
        for t in &r.tracks {
            assert!((-100.0..-30.0).contains(&t.peak_dbm), "{t:?}");
        }
    }
}

#[test]
fn the_demo_drone_comes_and_goes() {
    // 60 s loop at 10 fps: the drone is on air 8..40 s.
    let reports = run(42, Scenario::demo(), 600);
    assert!(
        !reports[..80].iter().any(Report::drone_detected),
        "false alarm before 8 s"
    );
    assert!(
        reports[150..350].iter().all(Report::drone_detected),
        "missed while overhead"
    );
    assert!(!reports[450].drone_detected(), "still active after it left");
    assert!(
        has_link(&reports[430]),
        "lost link should stay listed for a while"
    );
    assert!(!has_link(&reports[599]), "lost link should be dropped");
}

#[test]
fn detection_does_not_flap_during_a_flight() {
    // Regression: a weak sweep window in mid-flight dropped the link for a few sweeps,
    // so clients saw "lost" + "detected" again within half a second.
    for seed in 1..=40 {
        let active: Vec<bool> = run(seed, Scenario::demo(), 1200)
            .iter()
            .map(Report::drone_detected)
            .collect();
        let losses = active.windows(2).filter(|w| w[0] && !w[1]).count();
        assert_eq!(
            losses, 2,
            "seed {seed}: expected one loss per 60 s loop, got {losses}"
        );
    }
}
