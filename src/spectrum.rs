//! Spectrum geometry and the sweeps everything downstream of a source works on.

/// A swept frequency range split into equal-width bins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub start_mhz: f32,
    pub stop_mhz: f32,
    pub bins: usize,
}

impl Band {
    /// The 2.4 GHz ISM band in 256 bins of ~326 kHz.
    pub const ISM_2G4: Band = Band {
        start_mhz: 2400.0,
        stop_mhz: 2483.5,
        bins: 256,
    };

    /// Width of one bin in MHz.
    pub fn bin_mhz(&self) -> f32 {
        (self.stop_mhz - self.start_mhz) / self.bins as f32
    }

    /// Centre frequency of bin `i` in MHz.
    pub fn freq_of_bin(&self, i: usize) -> f32 {
        self.start_mhz + (i as f32 + 0.5) * self.bin_mhz()
    }

    /// The bin that contains `mhz`, or `None` outside the band.
    pub fn bin_of_freq(&self, mhz: f32) -> Option<usize> {
        if !(self.start_mhz..self.stop_mhz).contains(&mhz) {
            return None;
        }
        let i = ((mhz - self.start_mhz) / self.bin_mhz()) as usize;
        Some(i.min(self.bins - 1))
    }
}

/// One pass across a [`Band`]: power per bin in dBm.
#[derive(Debug, Clone, PartialEq)]
pub struct Sweep {
    /// Capture time in ms since the source started.
    pub t_ms: u64,
    pub power_dbm: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bins_cover_the_band() {
        let b = Band::ISM_2G4;
        assert!((b.freq_of_bin(0) - b.start_mhz - b.bin_mhz() / 2.0).abs() < 1e-3);
        assert!((b.freq_of_bin(b.bins - 1) + b.bin_mhz() / 2.0 - b.stop_mhz).abs() < 1e-3);
    }

    #[test]
    fn frequency_maps_back_to_its_bin() {
        let b = Band::ISM_2G4;
        for i in [0, 1, 100, b.bins - 1] {
            assert_eq!(b.bin_of_freq(b.freq_of_bin(i)), Some(i));
        }
        assert_eq!(b.bin_of_freq(2399.9), None);
        assert_eq!(b.bin_of_freq(2483.5), None);
    }
}
