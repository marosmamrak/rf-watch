//! The scan pipeline: source → detector → [`Hub`], on a thread of its own.
//!
//! Reading a front end blocks and detection is CPU work, so neither belongs on the async
//! runtime. The network side only touches the [`Hub`]: a broadcast channel of serialized
//! frames for WebSocket clients and a watch channel with the latest [`Snapshot`].

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use tokio::sync::{broadcast, oneshot, watch};

use crate::detect::{Detector, Report, Track};
use crate::protocol::{Frame, ServerMsg};
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

        let msg = ServerMsg::Frame(Frame::new(seq, sweep, &report));
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
    /// Dropping the sender wakes the thread and stops it; nothing is ever sent.
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    outcome: Option<oneshot::Receiver<Result<()>>>,
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
        let (stop_tx, stop_rx) = mpsc::channel();
        let (outcome_tx, outcome_rx) = oneshot::channel();
        let thread = thread::Builder::new().name("scanner".into()).spawn(move || {
            let _ = outcome_tx.send(run(source, detector, &hub, pace, &stop_rx));
        })?;
        Ok(Self {
            stop: Some(stop_tx),
            thread: Some(thread),
            outcome: Some(outcome_rx),
        })
    }

    /// Resolves when the pipeline stops on its own: `Ok` when the source ran out, the
    /// source's error when it failed; pending while it runs. Cancel-safe.
    pub async fn finished(&mut self) -> Result<()> {
        let Some(rx) = self.outcome.as_mut() else {
            return std::future::pending().await;
        };
        let outcome = rx
            .await
            .unwrap_or_else(|_| Err(anyhow!("scanner thread panicked")));
        self.outcome = None;
        outcome
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Runs until the source ends or fails, or `stop` is disconnected. A source blocked in
/// `next_sweep` is not interrupted; the stop takes effect when it returns.
fn run(
    mut source: impl SweepSource,
    mut detector: Detector,
    hub: &Hub,
    pace: Option<Duration>,
    stop: &mpsc::Receiver<()>,
) -> Result<()> {
    let mut next = Instant::now();
    let mut seq = 0;
    loop {
        let now = Instant::now();
        let wait = match pace {
            Some(_) => next.saturating_duration_since(now),
            None => Duration::ZERO,
        };
        // Sleeps until the next sweep is due, but wakes at once when stopped.
        match stop.recv_timeout(wait) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if let Some(period) = pace {
            // A late sweep does not cause a burst of catch-up sweeps.
            next = next.max(now) + period;
        }
        let Some(sweep) = source.next_sweep()? else {
            tracing::info!(sweeps = seq, "sweep source finished");
            return Ok(());
        };
        let report = detector.process(&sweep);
        hub.publish(seq, &sweep, report);
        seq += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use anyhow::bail;

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

    fn spawn(sweeps: u64, fail_at_end: bool, hub: &Arc<Hub>, pace: Option<Duration>) -> Scanner {
        let sweeps = (0..sweeps)
            .map(|i| Sweep {
                t_ms: i * 100,
                power_dbm: vec![-96.0; Band::ISM_2G4.bins],
            })
            .collect();
        let det = Detector::new(DetectorConfig::default(), Band::ISM_2G4);
        Scanner::spawn(Fixed { sweeps, fail_at_end }, det, hub.clone(), pace).unwrap()
    }

    async fn finish(scanner: &mut Scanner) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), scanner.finished())
            .await
            .expect("scanner did not stop")
    }

    #[tokio::test]
    async fn publishes_every_sweep_and_reports_the_end_of_the_source() {
        let hub = Arc::new(Hub::default());
        let mut rx = hub.subscribe();
        finish(&mut spawn(3, false, &hub, None)).await.unwrap();
        assert_eq!(hub.snapshot().frames, 3);
        for seq in 0..3 {
            let frame: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(frame["type"], "frame");
            assert_eq!(frame["seq"], seq);
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_failing_source_is_reported() {
        let hub = Arc::new(Hub::default());
        let err = finish(&mut spawn(2, true, &hub, None)).await.unwrap_err();
        assert_eq!(err.to_string(), "device unplugged");
        assert_eq!(hub.snapshot().frames, 2);
    }

    #[test]
    fn pacing_limits_the_sweep_rate() {
        let hub = Arc::new(Hub::default());
        let start = Instant::now();
        let scanner = spawn(1000, false, &hub, Some(Duration::from_millis(20)));
        thread::sleep(Duration::from_millis(200));
        drop(scanner);
        let elapsed = start.elapsed();
        let frames = hub.snapshot().frames;
        // One at once, then one per 20 ms of however long the test really took.
        let most = elapsed.as_millis() / 20 + 1;
        assert!(
            frames >= 5 && u128::from(frames) <= most,
            "{frames} frames in {elapsed:?}"
        );
    }

    #[test]
    fn dropping_wakes_a_waiting_scanner_at_once() {
        let hub = Arc::new(Hub::default());
        let scanner = spawn(1000, false, &hub, Some(Duration::from_secs(10)));
        // Wait for the first sweep; the thread then sleeps until the next one is due.
        let deadline = Instant::now() + Duration::from_secs(5);
        while hub.snapshot().frames == 0 {
            assert!(Instant::now() < deadline, "no first sweep");
            thread::sleep(Duration::from_millis(5));
        }
        let start = Instant::now();
        drop(scanner);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "stop took {:?}",
            start.elapsed()
        );
        assert_eq!(hub.snapshot().frames, 1, "no sweep after the stop");
    }
}
