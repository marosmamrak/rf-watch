//! HTTP side of the node: `/ws` streams frames, `/api/status` is a JSON snapshot,
//! everything else is the embedded Svelte UI.

mod assets;
mod ws;

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::response::Json;
use axum::routing::get;

use crate::protocol::{self, Hello, ServerMsg, Status};
use crate::scanner::Hub;
use crate::spectrum::Band;

#[derive(Clone)]
pub struct AppState {
    device: Arc<str>,
    hub: Arc<Hub>,
    /// The `hello` message, serialized once.
    hello: Arc<str>,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

impl AppState {
    pub fn new(device: &str, band: Band, fps: u32, hub: Arc<Hub>) -> Self {
        let hello = ServerMsg::Hello(Hello {
            device: device.to_owned(),
            version: VERSION.to_owned(),
            band: protocol::Band::new(&band, fps),
        });
        Self {
            device: device.into(),
            hub,
            hello: serde_json::to_string(&hello).expect("hello serializes").into(),
        }
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws::handler))
        .route("/api/status", get(status))
        .fallback(assets::handler)
        .with_state(state)
}

async fn status(State(state): State<AppState>) -> Json<Status> {
    let snap = state.hub.snapshot();
    Json(Status {
        device: state.device.to_string(),
        version: VERSION.to_owned(),
        frames: snap.frames,
        clients: state.hub.clients(),
        drone_detected: snap.drone_detected,
        tracks: snap.tracks.iter().map(protocol::Track::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn state() -> AppState {
        AppState::new("test-node", Band::ISM_2G4, 10, Arc::new(Hub::default()))
    }

    #[tokio::test]
    async fn status_reports_device() {
        let res = app(state())
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
        let res = app(state())
            .oneshot(Request::get("/nope.js").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
