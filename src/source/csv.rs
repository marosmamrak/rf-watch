//! Replays power sweeps recorded by `hackrf_sweep` or `rtl_power`, which share a CSV
//! layout:
//!
//! ```text
//! date, time, hz_low, hz_high, hz_bin_width, num_samples, dB, dB, ...
//! 2026-11-19, 12:00:00.250000, 2400000000, 2405000000, 1000000.00, 20, -71.2, -70.9, ...
//! ```
//!
//! A line covers one slice of the spectrum. The lines of a sweep go up in frequency
//! (`hackrf_sweep` interleaves the slices of a step, so not strictly), and a line starting
//! at or below the first frequency of the current sweep begins the next one.
//!
//! Each sweep is resampled onto the target [`Band`]: a band bin takes the strongest
//! capture bin centred in it, so a narrow burst is not averaged away, or else the capture
//! bin covering its centre (captures coarser than the band). Bins the capture does not
//! cover get the median of the covered ones, so they never cross the detection threshold.
//!
//! Sweep times come from the file, relative to the first sweep. A recording that stops
//! mid-sweep or mid-line (an interrupted `hackrf_sweep`) ends the replay quietly; damage
//! anywhere else is an error naming the line.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

use super::SweepSource;
use crate::spectrum::{Band, Sweep};

pub struct CsvReplay<R> {
    reader: R,
    band: Band,
    line_no: usize,
    /// First line of the next sweep, read while finishing the previous one.
    pending: Option<Line>,
    /// Recorded time of the previous sweep; `None` before the first.
    last_abs_ms: Option<i64>,
    last_t_ms: u64,
    /// Interval between the previous two sweeps, to bridge a clock that steps back.
    last_step_ms: u64,
}

impl CsvReplay<BufReader<File>> {
    pub fn open(path: &Path, band: Band) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        Ok(Self::new(BufReader::new(file), band))
    }
}

impl<R: BufRead> CsvReplay<R> {
    pub fn new(reader: R, band: Band) -> Self {
        Self {
            reader,
            band,
            line_no: 0,
            pending: None,
            last_abs_ms: None,
            last_t_ms: 0,
            last_step_ms: 0,
        }
    }

    fn next_line(&mut self) -> Result<Option<Line>> {
        let mut buf = String::new();
        loop {
            buf.clear();
            let read = self
                .reader
                .read_line(&mut buf)
                .with_context(|| format!("line {}", self.line_no + 1))?;
            if read == 0 {
                return Ok(None);
            }
            self.line_no += 1;
            let text = buf.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            return match Line::parse(text, self.line_no) {
                Ok(line) => Ok(Some(line)),
                // No newline: the recording was cut off while writing this line.
                Err(e) if !buf.ends_with('\n') => {
                    tracing::warn!(line = self.line_no, "ignoring the truncated last line: {e:#}");
                    Ok(None)
                }
                Err(e) => Err(e.context(format!("line {}", self.line_no))),
            };
        }
    }

    /// The lines of the next sweep, `None` at the end of the file.
    fn next_lines(&mut self) -> Result<Option<Vec<Line>>> {
        let Some(first) = self
            .pending
            .take()
            .map_or_else(|| self.next_line(), |l| Ok(Some(l)))?
        else {
            return Ok(None);
        };
        let mut lines = vec![first];
        while let Some(line) = self.next_line()? {
            if line.hz_low <= lines[0].hz_low {
                self.pending = Some(line);
                break;
            }
            lines.push(line);
        }
        Ok(Some(lines))
    }

    /// Time since the first sweep, advanced by the recorded interval between sweeps. When
    /// the recording clock steps back (DST, an NTP correction, concatenated captures),
    /// time keeps moving at the previous interval, so tracks still age.
    fn t_ms(&mut self, abs_ms: i64) -> u64 {
        if let Some(prev) = self.last_abs_ms {
            if abs_ms >= prev {
                let step = (abs_ms - prev) as u64;
                if step > 0 {
                    self.last_step_ms = step;
                }
                self.last_t_ms += step;
            } else {
                self.last_t_ms += self.last_step_ms;
            }
        }
        self.last_abs_ms = Some(abs_ms);
        self.last_t_ms
    }
}

impl<R: BufRead + Send> SweepSource for CsvReplay<R> {
    fn band(&self) -> Band {
        self.band
    }

    fn next_sweep(&mut self) -> Result<Option<Sweep>> {
        let Some(lines) = self.next_lines()? else {
            return Ok(None);
        };
        let last_in_file = self.pending.is_none();
        let started = self.last_abs_ms.is_some();
        let power_dbm = match resample(&self.band, &lines) {
            Ok(p) => p,
            // The recording stopped before this sweep reached the band, e.g. `hackrf_sweep`
            // over a wider range, interrupted.
            Err(e) if last_in_file && started => {
                tracing::warn!(
                    line = lines[0].line_no,
                    "ignoring the incomplete last sweep: {e:#}"
                );
                return Ok(None);
            }
            Err(e) => return Err(e.context(format!("sweep starting at line {}", lines[0].line_no))),
        };
        let t_ms = self.t_ms(lines[0].abs_ms);
        Ok(Some(Sweep { t_ms, power_dbm }))
    }
}

/// One CSV line: a slice of the spectrum.
#[derive(Debug, Clone, PartialEq)]
struct Line {
    line_no: usize,
    /// Milliseconds since 1970-01-01 of the line's date and time (no time zone).
    abs_ms: i64,
    hz_low: f64,
    hz_bin_width: f64,
    db: Vec<f32>,
}

impl Line {
    fn parse(text: &str, line_no: usize) -> Result<Self> {
        let fields: Vec<&str> = text.split(',').map(str::trim).collect();
        ensure!(
            fields.len() >= 7,
            "expected date, time, hz_low, hz_high, hz_bin_width, num_samples and at least one value, \
             got {} fields",
            fields.len()
        );
        let number = |i: usize, name: &str| -> Result<f64> {
            let v: f64 = fields[i]
                .parse()
                .with_context(|| format!("{name} is not a number: {:?}", fields[i]))?;
            ensure!(v.is_finite(), "{name} is not finite: {:?}", fields[i]);
            Ok(v)
        };
        let hz_bin_width = number(4, "hz_bin_width")?;
        ensure!(hz_bin_width > 0.0, "hz_bin_width must be positive");
        let db = fields[6..]
            .iter()
            .map(|v| {
                v.parse::<f32>()
                    .with_context(|| format!("value is not a number: {v:?}"))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            line_no,
            abs_ms: parse_date(fields[0])? * 86_400_000 + parse_time(fields[1])?,
            hz_low: number(2, "hz_low")?,
            hz_bin_width,
            db,
        })
    }

    /// `(centre MHz, dB)` of each finite value.
    fn bins(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.db
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_finite())
            .map(|(k, &v)| {
                let hz = self.hz_low + (k as f64 + 0.5) * self.hz_bin_width;
                ((hz / 1e6) as f32, v)
            })
    }
}

fn resample(band: &Band, lines: &[Line]) -> Result<Vec<f32>> {
    let mut out: Vec<Option<f32>> = vec![None; band.bins];
    // Capture bins centred in a band bin: the strongest wins.
    for (mhz, v) in lines.iter().flat_map(Line::bins) {
        if let Some(i) = band.bin_of_freq(mhz) {
            out[i] = Some(out[i].map_or(v, |o| o.max(v)));
        }
    }
    // Band bins with none centred in them: the capture bin covering the bin's centre.
    for (i, slot) in out.iter_mut().enumerate().filter(|(_, s)| s.is_none()) {
        let f = band.freq_of_bin(i);
        *slot = lines
            .iter()
            .flat_map(|l| {
                let half = (l.hz_bin_width / 2e6) as f32;
                l.bins().filter(move |&(mhz, _)| (mhz - f).abs() <= half)
            })
            .map(|(_, v)| v)
            .reduce(f32::max);
    }

    let mut covered: Vec<f32> = out.iter().flatten().copied().collect();
    if covered.is_empty() {
        bail!(
            "no data inside {}–{} MHz; is this a capture of that band?",
            band.start_mhz,
            band.stop_mhz
        );
    }
    covered.sort_by(f32::total_cmp);
    let median = covered[covered.len() / 2];
    Ok(out.into_iter().map(|v| v.unwrap_or(median)).collect())
}

/// `YYYY-MM-DD` as days since 1970-01-01.
fn parse_date(s: &str) -> Result<i64> {
    let parts: Vec<&str> = s.split('-').collect();
    let [y, m, d] = parts[..] else {
        bail!("date is not YYYY-MM-DD: {s:?}")
    };
    let (y, m, d): (i64, i64, i64) = (y.parse()?, m.parse()?, d.parse()?);
    ensure!(
        (1..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d),
        "date out of range: {s:?}"
    );
    Ok(days_from_civil(y, m, d))
}

/// `HH:MM:SS` with optional fraction as milliseconds since midnight.
fn parse_time(s: &str) -> Result<i64> {
    let parts: Vec<&str> = s.split(':').collect();
    let [h, m, sec] = parts[..] else {
        bail!("time is not HH:MM:SS: {s:?}")
    };
    // Unsigned, so a sign is rejected.
    let (h, m): (u32, u32) = (h.parse()?, m.parse()?);
    let sec: f64 = sec.parse().with_context(|| format!("seconds: {sec:?}"))?;
    ensure!(
        h < 24 && m < 60 && (0.0..61.0).contains(&sec),
        "time out of range: {s:?}"
    );
    Ok(i64::from(h * 3600 + m * 60) * 1000 + (sec * 1000.0).round() as i64)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAND: Band = Band::ISM_2G4;

    fn replay(csv: &str) -> CsvReplay<&[u8]> {
        CsvReplay::new(csv.as_bytes(), BAND)
    }

    fn sweeps(csv: &str) -> Result<Vec<Sweep>> {
        let mut r = replay(csv);
        let mut out = Vec::new();
        while let Some(s) = r.next_sweep()? {
            out.push(s);
        }
        Ok(out)
    }

    /// A line of `n` bins of `width_hz` from `low_hz`, all at `db` except `(index, db)`.
    fn line(time: &str, low_hz: u64, width_hz: u64, n: usize, db: f32, peaks: &[(usize, f32)]) -> String {
        let mut values = vec![db; n];
        for &(k, v) in peaks {
            values[k] = v;
        }
        let values: Vec<String> = values.iter().map(f32::to_string).collect();
        format!(
            "2026-11-19, {time}, {low_hz}, {}, {width_hz}.00, 20, {}\n",
            low_hz + width_hz * n as u64,
            values.join(", ")
        )
    }

    #[test]
    fn dates_count_days_since_the_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2024, 1, 1), 19_723);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
    }

    #[test]
    fn interleaved_lines_are_grouped_into_sweeps() {
        // hackrf_sweep style: 20 MHz steps, slices out of order within a sweep.
        let mut csv = String::new();
        for (time, base_db) in [("12:00:00.000", -90.0), ("12:00:00.250", -80.0)] {
            for low in [2_400, 2_440, 2_420, 2_460, 2_480] {
                csv += &line(
                    time,
                    low * 1_000_000,
                    1_000_000,
                    20.min(2_484 - low as usize),
                    base_db,
                    &[],
                );
            }
        }
        let s = sweeps(&csv).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].t_ms, s[1].t_ms), (0, 250));
        assert!(s[0].power_dbm.iter().all(|&p| p == -90.0));
        assert!(s[1].power_dbm.iter().all(|&p| p == -80.0));
    }

    #[test]
    fn fine_capture_bins_keep_the_strongest() {
        // 50 kHz bins, ~6.5 per band bin; one strong bin must survive.
        let csv = line("12:00:00", 2_400_000_000, 50_000, 1_670, -95.0, &[(1_000, -40.0)]);
        let s = &sweeps(&csv).unwrap()[0];
        let burst_bin = BAND.bin_of_freq(2_400.0 + 1_000.5 * 0.05).unwrap();
        assert_eq!(s.power_dbm[burst_bin], -40.0);
        assert_eq!(s.power_dbm.iter().filter(|&&p| p == -40.0).count(), 1);
    }

    #[test]
    fn coarse_capture_bins_fill_every_band_bin_they_cover() {
        // 1 MHz bins over the whole band: ~3 band bins per capture bin.
        let csv = line("12:00:00", 2_400_000_000, 1_000_000, 84, -95.0, &[(50, -40.0)]);
        let s = &sweeps(&csv).unwrap()[0];
        let hot: Vec<usize> = (0..BAND.bins).filter(|&i| s.power_dbm[i] == -40.0).collect();
        assert!((3..=4).contains(&hot.len()), "{hot:?}");
        assert!(hot.iter().all(|&i| (BAND.freq_of_bin(i) - 2_450.5).abs() <= 0.5));
    }

    #[test]
    fn uncovered_bins_get_the_median() {
        // Covers 2400–2440 MHz only.
        let csv = line("12:00:00", 2_400_000_000, 1_000_000, 40, -95.0, &[(5, -40.0)]);
        let s = &sweeps(&csv).unwrap()[0];
        let top = BAND.bin_of_freq(2_470.0).unwrap();
        assert_eq!(s.power_dbm[top], -95.0);
    }

    #[test]
    fn time_runs_across_midnight() {
        let csv = line("23:59:59.900", 2_400_000_000, 1_000_000, 84, -95.0, &[])
            + &line("00:00:00.100", 2_400_000_000, 1_000_000, 84, -95.0, &[]).replace("11-19", "11-20");
        let s = sweeps(&csv).unwrap();
        assert_eq!(s[1].t_ms, 200);
    }

    #[test]
    fn errors_name_the_line() {
        let csv = line("12:00:00", 2_400_000_000, 1_000_000, 84, -95.0, &[]) + "\n2026-11-19, 12:00:01, x\n";
        let err = format!("{:#}", sweeps(&csv).unwrap_err());
        assert!(err.contains("line 3"), "{err}");

        let csv = line("12:00:00", 2_400_000_000, 1_000_000, 3, -95.0, &[]).replace("-95, -95", "-95, oops");
        let err = format!("{:#}", sweeps(&csv).unwrap_err());
        assert!(err.contains("line 1") && err.contains("oops"), "{err}");
    }

    #[test]
    fn a_capture_of_another_band_is_rejected() {
        let csv = line("12:00:00", 5_725_000_000, 1_000_000, 100, -95.0, &[]);
        let err = format!("{:#}", sweeps(&csv).unwrap_err());
        assert!(err.contains("no data inside"), "{err}");
    }

    #[test]
    fn time_keeps_moving_when_the_recording_clock_steps_back() {
        // DST fall-back between the second and third sweep.
        let csv: String = ["02:59:59.800", "02:59:59.900", "02:00:00.000", "02:00:00.100"]
            .iter()
            .map(|t| line(t, 2_400_000_000, 1_000_000, 84, -95.0, &[]))
            .collect();
        let t: Vec<u64> = sweeps(&csv).unwrap().iter().map(|s| s.t_ms).collect();
        assert_eq!(t, [0, 100, 200, 300]);
    }

    #[test]
    fn an_interrupted_recording_ends_quietly() {
        // A sweep over 2300–2500 MHz, the second one cut off before reaching the band...
        let mut csv = String::new();
        for low in [2_300, 2_340, 2_380, 2_420, 2_460] {
            csv += &line("12:00:00", low * 1_000_000, 1_000_000, 40, -95.0, &[]);
        }
        csv += &line("12:00:01", 2_300_000_000, 1_000_000, 40, -95.0, &[]);
        assert_eq!(sweeps(&csv).unwrap().len(), 1);

        // ...or in the middle of writing a line.
        csv.truncate(csv.len() - 20);
        assert_eq!(sweeps(&csv).unwrap().len(), 1);
    }

    #[test]
    fn errors_point_at_the_first_line_of_the_sweep() {
        // Line 2 starts below line 1, so it begins sweep 2 (read ahead while closing sweep
        // 1); line 3 starts lower still and begins sweep 3. Sweep 2 has no data in the band.
        let csv = line("12:00:00", 2_400_000_000, 1_000_000, 84, -95.0, &[])
            + &line("12:00:01", 1_000_000_000, 1_000_000, 84, -95.0, &[])
            + &line("12:00:02", 900_000_000, 1_000_000, 84, -95.0, &[]);
        let err = format!("{:#}", sweeps(&csv).unwrap_err());
        assert!(err.starts_with("sweep starting at line 2:"), "{err}");
    }

    #[test]
    fn an_unreadable_line_is_named() {
        let mut bytes = line("12:00:00", 2_400_000_000, 1_000_000, 84, -95.0, &[]).into_bytes();
        bytes.extend_from_slice(b"2026-11-19, 12:00:01, \xff\n");
        let mut r = CsvReplay::new(bytes.as_slice(), BAND);
        let err = format!("{:#}", r.next_sweep().unwrap_err());
        assert!(err.starts_with("line 2:"), "{err}");
    }

    #[test]
    fn implausible_fields_are_rejected() {
        let ok = line("12:00:00", 2_400_000_000, 1_000_000, 84, -95.0, &[]);
        for bad in [
            ok.replace("12:00:00", "-3:00:00"),
            ok.replace("2026-11-19", "99999999999999-01-01"),
            ok.replace(", 2400000000,", ", nan,"),
            ok.replace("1000000.00", "inf"),
        ] {
            let err = format!("{:#}", sweeps(&(bad.clone() + &ok)).unwrap_err());
            assert!(err.starts_with("line 1:"), "{bad} → {err}");
        }
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let csv = String::from("# hackrf_sweep -f 2400:2484\n\n")
            + &line("12:00:00", 2_400_000_000, 1_000_000, 84, -95.0, &[]);
        assert_eq!(sweeps(&csv).unwrap().len(), 1);
    }
}
