//! HTTP + WebSocket server: `/ws` streams frames, `/api/status` is a JSON snapshot,
//! everything else is the embedded Svelte UI.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use rust_embed::RustEmbed;
use serde::Serialize;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::MissedTickBehavior;

use crate::detect::{Detector, Track};
use crate::protocol::{self, ServerMsg, round_tenth};
use crate::source::SweepSource;
use crate::spectrum::Band;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
#[allow_missing = true]
struct Assets;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    pub device: String,
    pub version: String,
    pub frames: u64,
    pub clients: usize,
    pub drone_detected: bool,
    pub tracks: Vec<Track>,
}

#[derive(Clone)]
pub struct AppState {
    tx: broadcast::Sender<Arc<str>>,
    hello: Arc<str>,
    status: Arc<RwLock<Status>>,
}

impl AppState {
    pub fn new(device: &str, band: Band, fps: u32) -> Self {
        let version = env!("CARGO_PKG_VERSION");
        let hello = ServerMsg::Hello {
            device: device.to_owned(),
            version: version.to_owned(),
            band: protocol::Band {
                start_mhz: band.start_mhz,
                stop_mhz: band.stop_mhz,
                bins: band.bins,
                fps,
            },
        };
        let (tx, _) = broadcast::channel(32);
        Self {
            tx,
            hello: serde_json::to_string(&hello).expect("hello serializes").into(),
            status: Arc::new(RwLock::new(Status {
                device: device.to_owned(),
                version: version.to_owned(),
                ..Status::default()
            })),
        }
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/status", get(status))
        .fallback(static_handler)
        .with_state(state)
}

/// Runs sweep → detect → broadcast at `fps` until the task is dropped or the source ends.
pub async fn run_scanner(state: AppState, mut source: impl SweepSource, mut det: Detector, fps: u32) {
    let mut tick = tokio::time::interval(Duration::from_millis(1000 / u64::from(fps.max(1))));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut seq = 0u64;
    loop {
        tick.tick().await;
        let sweep = match source.next_sweep() {
            Ok(Some(sweep)) => sweep,
            Ok(None) => {
                tracing::info!("sweep source finished");
                return;
            }
            Err(e) => {
                tracing::error!("sweep source failed: {e:#}");
                return;
            }
        };
        let report = det.process(&sweep);
        let drone = report.drone_detected();

        {
            let mut st = state.status.write().expect("status lock");
            if drone && !st.drone_detected {
                tracing::warn!("drone control link detected");
            } else if !drone && st.drone_detected {
                tracing::info!("drone control link lost");
            }
            st.frames = seq + 1;
            st.drone_detected = drone;
            st.tracks = report.tracks.clone();
        }

        let msg = ServerMsg::Frame {
            seq,
            t_ms: sweep.t_ms,
            noise_floor_dbm: round_tenth(report.noise_floor_dbm),
            threshold_dbm: round_tenth(report.threshold_dbm),
            power_dbm: sweep.power_dbm.iter().copied().map(round_tenth).collect(),
            tracks: report.tracks,
        };
        let json = serde_json::to_string(&msg).expect("frame serializes");
        // No receivers is fine: nobody is watching.
        let _ = state.tx.send(json.into());
        seq += 1;
    }
}

async fn status(State(state): State<AppState>) -> Json<Status> {
    let mut st = state.status.read().expect("status lock").clone();
    st.clients = state.tx.receiver_count();
    Json(st)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| client(socket, state))
}

async fn client(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();
    if socket
        .send(Message::Text(state.hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    tracing::info!(clients = state.tx.receiver_count(), "client connected");
    loop {
        tokio::select! {
            frame = rx.recv() => match frame {
                Ok(json) => {
                    if socket.send(Message::Text(json.to_string().into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(n)) => tracing::debug!(n, "slow client, frames skipped"),
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
        }
    }
    tracing::info!("client disconnected");
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([(header::CONTENT_TYPE, mime.as_ref())], file.data).into_response();
    }
    if !path.contains('.') {
        if let Some(index) = Assets::get("index.html") {
            return ([(header::CONTENT_TYPE, "text/html")], index.data).into_response();
        }
    }
    let msg = if Assets::get("index.html").is_none() {
        "UI not built: run `npm ci && npm run build` in web/ and rebuild"
    } else {
        "not found"
    };
    (StatusCode::NOT_FOUND, msg).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn status_reports_device() {
        let state = AppState::new("test-node", Band::ISM_2G4, 10);
        let res = app(state)
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["device"], "test-node");
        assert_eq!(json["drone_detected"], false);
    }

    #[tokio::test]
    async fn unknown_asset_is_404() {
        let res = app(AppState::new("t", Band::ISM_2G4, 10))
            .oneshot(Request::get("/nope.js").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
