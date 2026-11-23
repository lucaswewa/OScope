//! The media plane: OScope's own routes beside the Things: `ping`, and the
//! logs' live tail.

use serde_json::{Value, json};
use teta_wot::http::ShutdownToken;
use teta_wot::http::axum::Router;
use teta_wot::http::axum::extract::State;
use teta_wot::http::axum::extract::ws::rejection::WebSocketUpgradeRejection;
use teta_wot::http::axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;
use tokio::sync::broadcast::error::RecvError;

use crate::MEDIA_PREFIX;
use crate::logs::{LogStore, Query};

/// The close code a media WebSocket ends with when the server shuts down:
/// 1001, going away.
const GOING_AWAY: u16 = 1001;

/// The media plane's routes.
pub(crate) fn router(logs: LogStore) -> Router {
    Router::new()
        .route(&format!("{MEDIA_PREFIX}/ping"), get(ping))
        .route(&format!("{MEDIA_PREFIX}/logs"), get(tail))
        .with_state(logs)
}

/// Closes a socket because the server is shutting down.
async fn going_away(socket: &mut WebSocket) {
    let frame = CloseFrame {
        code: GOING_AWAY,
        reason: "the server is shutting down".into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

/// Sends a JSON message.
async fn send(socket: &mut WebSocket, message: Value) -> bool {
    socket
        .send(Message::text(message.to_string()))
        .await
        .is_ok()
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
            () = shutdown.cancelled() => return going_away(&mut socket).await,
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

/// `GET /api/v1/media/logs`, a WebSocket: the logs' live tail.
///
/// - **When it opens,** and after each query the client sends (a JSON
///   [`Query`]), it sends `{"query": …}`, the query it now follows. Then it
///   sends the newest records that match, up to the query's `limit`, oldest
///   first, and `{"caught_up": n}`, where `n` is how many it sent. Then it
///   sends each new record that matches, as it is made.
/// - **Its messages** are `{"record": …}`. `{"skipped": n}` means the client
///   fell behind and missed `n` records. `{"error": "…"}` answers a wrong
///   query.
///
/// A client that changes its query can drop what it shows when `query`
/// arrives: every record after it matches the new query.
async fn tail(
    upgrade: WebSocketUpgrade,
    State(store): State<LogStore>,
    shutdown: ShutdownToken,
) -> Response {
    upgrade.on_upgrade(|socket| follow(socket, store, shutdown))
}

/// Serves the live tail until the client or the server ends it.
async fn follow(mut socket: WebSocket, store: LogStore, shutdown: ShutdownToken) {
    let mut query = Query::default();
    let (mut backlog, mut live) = store.follow(&query);
    let mut caught_up = false;
    loop {
        if !caught_up {
            let sent = backlog.len();
            if !send(&mut socket, json!({ "query": query })).await {
                return;
            }
            for record in backlog.drain(..) {
                if !send(&mut socket, json!({ "record": record })).await {
                    return;
                }
            }
            if !send(&mut socket, json!({ "caught_up": sent })).await {
                return;
            }
            caught_up = true;
        }
        let message = tokio::select! {
            () = shutdown.cancelled() => return going_away(&mut socket).await,
            message = socket.recv() => message,
            record = live.recv() => {
                let message = match record {
                    Ok(record) if query.matches(&record) => json!({ "record": *record }),
                    Ok(_) => continue,
                    Err(RecvError::Lagged(skipped)) => json!({ "skipped": skipped }),
                    Err(RecvError::Closed) => return,
                };
                if !send(&mut socket, message).await {
                    return;
                }
                continue;
            }
        };
        match message {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<Query>(&text) {
                Ok(new) => {
                    query = new;
                    (backlog, live) = store.follow(&query);
                    caught_up = false;
                }
                Err(error) => {
                    if !send(&mut socket, json!({ "error": error.to_string() })).await {
                        return;
                    }
                }
            },
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return,
        }
    }
}
