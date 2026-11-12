//! The media plane: OScope's own routes beside the Things. For now, `ping`.

use teta_wot::http::ShutdownToken;
use teta_wot::http::axum::Router;
use teta_wot::http::axum::extract::ws::rejection::WebSocketUpgradeRejection;
use teta_wot::http::axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;

use crate::MEDIA_PREFIX;

/// The close code a media WebSocket ends with when the server shuts down:
/// 1001, going away.
const GOING_AWAY: u16 = 1001;

/// The media plane's routes.
pub(crate) fn router() -> Router {
    Router::new().route(&format!("{MEDIA_PREFIX}/ping"), get(ping))
}

/// `GET /api/v1/media/ping`: `pong`. As a WebSocket, it sends back each text
/// or binary message, until the client or the server ends it.
async fn ping(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    shutdown: ShutdownToken,
) -> Response {
    match upgrade {
        Ok(upgrade) => upgrade.on_upgrade(|socket| echo(socket, shutdown)),
        Err(_) => "pong".into_response(),
    }
}

/// Sends back each message. When the server starts shutting down, it closes
/// the socket, so that the shutdown needn't wait for it.
async fn echo(mut socket: WebSocket, shutdown: ShutdownToken) {
    loop {
        let message = tokio::select! {
            () = shutdown.cancelled() => {
                let frame = CloseFrame {
                    code: GOING_AWAY,
                    reason: "the server is shutting down".into(),
                };
                let _ = socket.send(Message::Close(Some(frame))).await;
                return;
            }
            message = socket.recv() => message,
        };
        match message {
            Some(Ok(message @ (Message::Text(_) | Message::Binary(_)))) => {
                if socket.send(message).await.is_err() {
                    return;
                }
            }
            // axum answers pings; a close is followed by the end.
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return,
        }
    }
}
