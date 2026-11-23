//! End to end: start the server on a free port, connect over WebSocket, expect a hello
//! and then frames, and a drone detection with the always-on scenario.

use futures_util::StreamExt;
use rf_watch::detect::{Detector, DetectorConfig};
use rf_watch::server::{AppState, app, run_scanner};
use rf_watch::source::SweepSource;
use rf_watch::source::sim::{Scenario, Simulator};
use serde_json::Value;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn streams_hello_then_frames_with_a_detection() {
    let fps = 50;
    let source = Simulator::new(5, Scenario::drone(), fps);
    let band = source.band();
    let state = AppState::new("it-node", band, fps);
    tokio::spawn(run_scanner(
        state.clone(),
        source,
        Detector::new(DetectorConfig::default(), band),
        fps,
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    let mut next = async || -> Value {
        let msg = timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(&msg.into_text().unwrap()).unwrap()
    };

    let hello = next().await;
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["device"], "it-node");
    assert_eq!(hello["band"]["bins"], band.bins);

    let mut detected = false;
    for _ in 0..100 {
        let frame = next().await;
        assert_eq!(frame["type"], "frame");
        assert_eq!(frame["power_dbm"].as_array().unwrap().len(), band.bins);
        if frame["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["kind"] == "fhss_control" && t["active"] == true)
        {
            detected = true;
            break;
        }
    }
    assert!(detected, "no FHSS detection within 100 frames");
}
