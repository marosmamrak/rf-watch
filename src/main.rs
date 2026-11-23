use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use rf_watch::detect::{Detector, DetectorConfig};
use rf_watch::scanner::{Hub, Scanner};
use rf_watch::source::SweepSource;
use rf_watch::source::csv::CsvReplay;
use rf_watch::source::sim::{Scenario, Simulator};
use rf_watch::spectrum::Band;
use rf_watch::{mdns, server};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScenarioArg {
    /// 60 s loop, a drone flies through between 8 and 40 s
    Demo,
    /// Wi-Fi and blips, no drone
    Quiet,
    /// Drone link always on
    Drone,
}

/// Passive RF drone-link detector demo (simulated 2.4 GHz front end).
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Address to serve the UI, /ws and /api/status on
    #[arg(long, default_value = "0.0.0.0:8787")]
    listen: SocketAddr,
    /// Sweeps per second
    #[arg(long, default_value_t = 10)]
    fps: u32,
    /// Device name shown in the UI and advertised over mDNS
    #[arg(long, default_value = "rf-watch")]
    name: String,
    #[arg(long, value_enum, default_value = "demo")]
    scenario: ScenarioArg,
    /// Random seed for the simulator (default: time based)
    #[arg(long)]
    seed: Option<u64>,
    /// Do not advertise over mDNS
    #[arg(long)]
    no_mdns: bool,
    /// Replay a `hackrf_sweep` / `rtl_power` CSV capture of the 2.4 GHz band instead of
    /// simulating, at --fps sweeps per second
    #[arg(long, value_name = "CSV", conflicts_with_all = ["scenario", "seed"])]
    input: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
    let args = Args::parse();

    let fps = args.fps.max(1);
    let source: Box<dyn SweepSource> = match &args.input {
        Some(path) => {
            tracing::info!(file = %path.display(), "replaying capture");
            Box::new(CsvReplay::open(path, Band::ISM_2G4)?)
        }
        None => Box::new(simulator(&args, fps)),
    };
    let band = source.band();
    let hub = Arc::new(Hub::default());
    let detector = Detector::new(DetectorConfig::default(), band);
    let pace = Duration::from_secs(1) / fps;
    let scanner = Scanner::spawn(source, detector, hub.clone(), Some(pace))?;
    let state = server::AppState::new(&args.name, band, fps, hub);

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let port = listener.local_addr()?.port();

    let _mdns = if args.no_mdns {
        None
    } else {
        match mdns::advertise(&args.name, port) {
            Ok(daemon) => {
                tracing::info!(service = mdns::SERVICE_TYPE, "advertising over mDNS");
                Some(daemon)
            }
            Err(e) => {
                tracing::warn!("mDNS disabled: {e:#}");
                None
            }
        }
    };

    tracing::info!("UI on http://localhost:{port}  (stream: /ws, status: /api/status)");
    axum::serve(listener, server::app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    drop(scanner);
    Ok(())
}

fn simulator(args: &Args, fps: u32) -> Simulator {
    let scenario = match args.scenario {
        ScenarioArg::Demo => Scenario::demo(),
        ScenarioArg::Quiet => Scenario::quiet(),
        ScenarioArg::Drone => Scenario::drone(),
    };
    let seed = args.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
    });
    Simulator::new(seed, scenario, fps)
}
