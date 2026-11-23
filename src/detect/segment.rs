//! Noise-floor estimate and segmentation of a sweep into runs of bins above a threshold.

use crate::spectrum::Band;

/// Contiguous run of bins above the threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub center_mhz: f32,
    pub width_mhz: f32,
    pub peak_dbm: f32,
}

/// 25th-percentile power: robust against signals occupying part of the band.
pub fn noise_floor(power_dbm: &[f32]) -> f32 {
    let mut sorted = power_dbm.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[sorted.len() / 4]
}

/// Splits a sweep into contiguous runs of bins above `threshold_dbm`.
pub fn segments(band: &Band, power_dbm: &[f32], threshold_dbm: f32) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    // One step past the end closes a run that touches the top of the band.
    for i in 0..=power_dbm.len() {
        let above = power_dbm.get(i).is_some_and(|&p| p > threshold_dbm);
        match (start, above) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                let run = &power_dbm[s..i];
                out.push(Segment {
                    center_mhz: (band.freq_of_bin(s) + band.freq_of_bin(i - 1)) / 2.0,
                    width_mhz: run.len() as f32 * band.bin_mhz(),
                    peak_dbm: run.iter().copied().fold(f32::MIN, f32::max),
                });
                start = None;
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAND: Band = Band::ISM_2G4;

    #[test]
    fn finds_runs_above_threshold() {
        let mut p = vec![-100.0; 20];
        p[3] = -50.0;
        p[10..15].fill(-60.0);
        let segs = segments(&BAND, &p, -90.0);
        assert_eq!(segs.len(), 2);
        assert!((segs[0].width_mhz - BAND.bin_mhz()).abs() < 1e-4);
        assert!((segs[1].width_mhz - 5.0 * BAND.bin_mhz()).abs() < 1e-4);
        assert_eq!(segs[1].peak_dbm, -60.0);
    }

    #[test]
    fn a_run_touching_the_band_edge_is_closed() {
        let mut p = vec![-100.0; 10];
        p[8..].fill(-50.0);
        assert_eq!(segments(&BAND, &p, -90.0).len(), 1);
    }

    #[test]
    fn noise_floor_ignores_a_strong_signal() {
        let mut p = vec![-96.0; 100];
        p[40..70].fill(-50.0);
        assert_eq!(noise_floor(&p), -96.0);
    }
}
