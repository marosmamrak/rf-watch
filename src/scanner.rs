//! The scan pipeline: source → detector → [`Hub`], on a thread of its own.
//!
//! Reading a front end blocks and detection is CPU work, so neither belongs on the async
//! runtime. The network side only touches the [`Hub`]: a broadcast channel of serialized
//! frames for WebSocket clients and a watch channel with the latest [`Snapshot`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, watch};

use crate::detect::{Detector, Report, Track};
use crate::protocol::{ServerMsg, round_tenth};
use crate::source::SweepSource;
use crate::spectrum::Sweep;

/// Frames a slow WebSocket client may fall behind before it skips some.
const FRAME_BUFFER: usize = 32;

/// State after the latest sweep, for `/api/status`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub frames: u64,
    pub drone_detected: bool,
    pub tracks: Vec<Track>,
}

/// Hand-off from the scan thread to the network side.
pub struct Hub {
    frames: broadcast::Sender<Arc<str>>,
    snapshot: watch::Sender<Snapshot>,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            frames: broadcast::channel(FRAME_BUFFER).0,
            snapshot: watch::Sender::new(Snapshot::default()),
        }
    }
}

impl Hub {
    /// Serialized `frame` messages from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.frames.subscribe()
    }

    /// Connected frame subscribers.
    pub fn clients(&self) -> usize {
        self.frames.receiver_count()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    fn publish(&self, seq: u64, sweep: &Sweep, report: Report) {
        let drone = report.drone_detected();
        let was = self.snapshot.borrow().drone_detected;
        if drone && !was {
            tracing::warn!("drone control link detected");
        } else if was && !drone {
            tracing::info!("drone control link lost");
        }

        let msg = ServerMsg::Frame {
            seq,
            t_ms: sweep.t_ms,
            noise_floor_dbm: round_tenth(report.noise_floor_dbm),
            threshold_dbm: round_tenth(report.threshold_dbm),
            power_dbm: sweep.power_dbm.iter().copied().map(round_tenth).collect(),
            tracks: report.tracks.clone(),
        };
        let json = serde_json::to_string(&msg).expect("frame serializes");
        self.snapshot.send_replace(Snapshot {
            frames: seq + 1,
            drone_detected: drone,
            tracks: report.tracks,
        });
        // No subscribers is fine: nobody is watching.
        let _ = self.frames.send(json.into());
    }
}

/// The running pipeline. Dropping it stops the thread and waits for it.
pub struct Scanner {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Scanner {
    /// Starts reading `source` on a new thread. With `pace`, sweeps are taken at most once
    /// per period (the simulator, replaying a capture); without it, as fast as the source
    /// delivers them (a live front end paces itself).
    pub fn spawn(
        source: impl SweepSource + 'static,
        detector: Detector,
        hub: Arc<Hub>,
        pace: Option<Duration>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new().name("scanner".into()).spawn({
            let stop = stop.clone();
            move || run(source, detector, &hub, pace, &stop)
        })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// Whether the pipeline ended on its own (source exhausted or failed).
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    mut source: impl SweepSource,
    mut detector: Detector,
    hub: &Hub,
    pace: Option<Duration>,
    stop: &AtomicBool,
) {
    let mut next = Instant::now();
    let mut seq = 0;
    while !stop.load(Ordering::Relaxed) {
        if let Some(period) = pace {
            let now = Instant::now();
            if next > now {
                thread::sleep(next - now);
            }
            // A late sweep does not cause a burst of catch-up sweeps.
            next = next.max(now) + period;
        }
        let sweep = match source.next_sweep() {
            Ok(Some(sweep)) => sweep,
            Ok(None) => {
                tracing::info!(sweeps = seq, "sweep source finished");
                return;
            }
            Err(e) => {
                tracing::error!("sweep source failed: {e:#}");
                return;
            }
        };
        let report = detector.process(&sweep);
        hub.publish(seq, &sweep, report);
        seq += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use anyhow::{Result, bail};

    use super::*;
    use crate::detect::DetectorConfig;
    use crate::spectrum::Band;

    /// Plays back a fixed list of sweeps, then fails or ends.
    struct Fixed {
        sweeps: VecDeque<Sweep>,
        fail_at_end: bool,
    }

    impl SweepSource for Fixed {
        fn band(&self) -> Band {
            Band::ISM_2G4
        }

        fn next_sweep(&mut self) -> Result<Option<Sweep>> {
            match self.sweeps.pop_front() {
                Some(s) => Ok(Some(s)),
                None if self.fail_at_end => bail!("device unplugged"),
                None => Ok(None),
            }
        }
    }

    fn quiet_sweeps(n: u64) -> VecDeque<Sweep> {
        (0..n)
            .map(|i| Sweep {
                t_ms: i * 100,
                power_dbm: vec![-96.0; Band::ISM_2G4.bins],
            })
            .collect()
    }

    fn run_to_end(source: Fixed, hub: &Arc<Hub>) {
        let det = Detector::new(DetectorConfig::default(), Band::ISM_2G4);
        let scanner = Scanner::spawn(source, det, hub.clone(), None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !scanner.is_finished() {
            assert!(Instant::now() < deadline, "scanner did not stop");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn publishes_every_sweep_and_stops_at_the_end_of_the_source() {
        let hub = Arc::new(Hub::default());
        let mut rx = hub.subscribe();
        run_to_end(
            Fixed {
                sweeps: quiet_sweeps(3),
                fail_at_end: false,
            },
            &hub,
        );
        assert_eq!(hub.snapshot().frames, 3);
        for seq in 0..3 {
            let frame: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(frame["type"], "frame");
            assert_eq!(frame["seq"], seq);
        }
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_failing_source_stops_the_pipeline() {
        let hub = Arc::new(Hub::default());
        run_to_end(
            Fixed {
                sweeps: quiet_sweeps(2),
                fail_at_end: true,
            },
            &hub,
        );
        assert_eq!(hub.snapshot().frames, 2);
    }

    #[test]
    fn pacing_limits_the_sweep_rate() {
        let hub = Arc::new(Hub::default());
        let det = Detector::new(DetectorConfig::default(), Band::ISM_2G4);
        let source = Fixed {
            sweeps: quiet_sweeps(1000),
            fail_at_end: false,
        };
        let scanner = Scanner::spawn(source, det, hub.clone(), Some(Duration::from_millis(20))).unwrap();
        thread::sleep(Duration::from_millis(210));
        drop(scanner);
        let frames = hub.snapshot().frames;
        // 1 immediately + one per 20 ms; generous bounds for a loaded CI machine.
        assert!((5..=12).contains(&frames), "{frames} frames in 210 ms");
    }
}
