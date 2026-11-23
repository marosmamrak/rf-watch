//! `/ws`: a `hello`, then every frame the scanner publishes.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::sync::broadcast::error::RecvError;

use super::AppState;

pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| client(socket, state))
}

async fn client(mut socket: WebSocket, state: AppState) {
    let mut frames = state.hub.subscribe();
    if socket
        .send(Message::Text(state.hello.as_ref().into()))
        .await
        .is_err()
    {
        return;
    }
    tracing::info!(clients = state.hub.clients(), "client connected");
    loop {
        tokio::select! {
            frame = frames.recv() => match frame {
                Ok(json) => {
                    if socket.send(Message::Text(json.as_ref().into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(n)) => tracing::debug!(n, "slow client, frames skipped"),
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                // The protocol is one-way; anything a client sends is ignored.
                Some(Ok(_)) => {}
            },
        }
    }
    tracing::info!("client disconnected");
}
