//! The origins a browser may call the server from (W1.3, ADR-0045). Besides
//! the server's own, those of `[network] origins`: the dev server's, or the
//! desktop app's. A request whose `Origin` is anywhere else is refused, 403,
//! before it is served, a WebSocket's upgrade among them: teta-wot v0.2.0's
//! CORS lets every origin in.
//!
//! - **The server's own** is an `Origin` whose host and port are the
//!   request's `Host`, or its URI's authority under HTTP/2.
//! - **A request without an `Origin`** is a client that isn't a browser's
//!   page, or a browser's request of its own page: it is let in.
//! - **A preflight** is answered by teta-wot's CORS, before this layer: the
//!   request that follows it is the one refused.

use std::sync::Arc;

use serde_json::json;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::extract::{Request, State};
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::http::header::{CONTENT_TYPE, HOST, ORIGIN};
use teta_wot::http::axum::middleware::Next;
use teta_wot::http::axum::response::{IntoResponse, Response};

/// The origins, besides its own, whose pages may call the server.
#[derive(Clone, Debug, Default)]
pub struct Origins(Arc<Vec<String>>);

impl Origins {
    /// These origins, each as `scheme://host[:port]`, without a final `/`.
    pub fn new(origins: impl IntoIterator<Item = String>) -> Origins {
        let normal = origins
            .into_iter()
            .map(|origin| origin.trim().trim_end_matches('/').to_ascii_lowercase())
            .filter(|origin| !origin.is_empty())
            .collect();
        Origins(Arc::new(normal))
    }

    /// Whether a page at `origin` may call a server that its request names
    /// as `host`.
    pub fn allows(&self, origin: &str, host: Option<&str>) -> bool {
        let origin = origin.trim_end_matches('/').to_ascii_lowercase();
        let own = origin
            .split_once("://")
            .is_some_and(|(_, at)| host.is_some_and(|host| at.eq_ignore_ascii_case(host)));
        own || self.0.contains(&origin)
    }
}

/// The layer: lets a request in, or refuses it, 403, with problem details.
pub(crate) async fn check(
    State(origins): State<Origins>,
    request: Request,
    next: Next,
) -> Response {
    // Read before awaiting: a borrow of the request held across the await
    // would make the future not `Send`.
    let refused = {
        let text = |name| {
            request
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        text(ORIGIN).filter(|origin| {
            let host =
                text(HOST).or_else(|| request.uri().authority().map(|at| at.as_str().to_owned()));
            !origins.allows(origin, host.as_deref())
        })
    };
    match refused {
        None => next.run(request).await,
        Some(origin) => refusal(&origin),
    }
}

/// A refusal of a page at `origin`, with problem details.
fn refusal(origin: &str) -> Response {
    let problem = json!({
        "type": null,
        "title": "origin not allowed",
        "status": 403,
        "detail": format!(
            "a page at {origin} may not call this server: [network] origins lists those that may"
        ),
        "instance": null,
    });
    let mut response = (StatusCode::FORBIDDEN, Json(problem)).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "application/problem+json".parse().expect("a valid header"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::Origins;

    #[test]
    fn its_own_origin_and_the_listed_ones_are_let_in() {
        let origins = Origins::new(["http://127.0.0.1:8280/".to_owned(), " ".to_owned()]);
        assert!(origins.allows("http://127.0.0.1:8210", Some("127.0.0.1:8210")));
        assert!(origins.allows("HTTP://LOCALHOST:8210", Some("localhost:8210")));
        assert!(origins.allows("http://127.0.0.1:8280", Some("127.0.0.1:8210")));
        assert!(!origins.allows("http://127.0.0.1:8281", Some("127.0.0.1:8210")));
        assert!(!origins.allows("http://evil.example", Some("127.0.0.1:8210")));
        assert!(!origins.allows("null", Some("127.0.0.1:8210")));
        assert!(!origins.allows("http://127.0.0.1:8210", None));
    }
}
