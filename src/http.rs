//! Internal transport: request building, auth, retries, error mapping.

use std::time::Duration;

use chrono::Utc;
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;

use crate::error::{ApiError, Error, RateLimit, parse_retry_after};

/// How the client authenticates outgoing requests.
pub(crate) enum Auth {
    /// Public endpoints only.
    None,
    /// `Authorization: Bearer spoo_<key>`.
    ApiKey(String),
    /// A refreshing Sign in with Spoo session.
    #[cfg(feature = "oauth")]
    Session(std::sync::Arc<crate::oauth::Session>),
}

pub(crate) struct Transport {
    pub(crate) http: reqwest::Client,
    /// Base URL with no trailing slash, e.g. `https://spoo.me`.
    pub(crate) base_url: String,
    pub(crate) auth: Auth,
    pub(crate) client_tag: String,
    pub(crate) max_retries: u32,
    // Read only on native targets: wasm hosts own request lifetimes.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) timeout: Duration,
}

/// A request about to be sent, in transport-neutral form. The body is
/// serialized once up front so retries never re-serialize.
pub(crate) struct RequestSpec {
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) query: Vec<(String, String)>,
    pub(crate) body: Option<serde_json::Value>,
}

impl RequestSpec {
    pub(crate) fn new(method: Method, path: impl Into<String>) -> Self {
        RequestSpec {
            method,
            path: path.into(),
            query: Vec::new(),
            body: None,
        }
    }

    pub(crate) fn query(mut self, key: &str, value: Option<String>) -> Self {
        if let Some(value) = value {
            self.query.push((key.to_owned(), value));
        }
        self
    }

    pub(crate) fn json(mut self, body: &impl serde::Serialize) -> Result<Self, Error> {
        self.body = Some(serde_json::to_value(body).map_err(Error::Decode)?);
        Ok(self)
    }
}

impl Transport {
    /// Send the request and decode a JSON response body.
    pub(crate) async fn execute<T: DeserializeOwned>(&self, spec: RequestSpec) -> Result<T, Error> {
        let response = self.send(spec).await?;
        let bytes = response.bytes().await.map_err(Error::Transport)?;
        serde_json::from_slice(&bytes).map_err(Error::Decode)
    }

    /// One unauthenticated, non-retrying request outside the normal path:
    /// the token-refresh call itself. Kept off [`Transport::send`] so the
    /// refresh triggered from inside a request cannot recurse into another
    /// refresh.
    #[cfg(feature = "oauth")]
    pub(crate) async fn execute_unauthenticated<T: DeserializeOwned>(
        &self,
        spec: RequestSpec,
    ) -> Result<T, Error> {
        let response = self.dispatch(&spec, None).await.map_err(Error::Transport)?;
        if !response.status().is_success() {
            return Err(Error::Api(Box::new(map_error(response).await)));
        }
        let bytes = response.bytes().await.map_err(Error::Transport)?;
        serde_json::from_slice(&bytes).map_err(Error::Decode)
    }

    /// Send the request and return the raw successful response (exports).
    pub(crate) async fn send(&self, spec: RequestSpec) -> Result<Response, Error> {
        let mut attempt: u32 = 0;
        #[cfg(feature = "oauth")]
        let mut refreshed = false;
        loop {
            // Resolve the credential per attempt: a session may rotate
            // between attempts.
            #[cfg(feature = "oauth")]
            let mut session_generation: Option<u64> = None;
            let bearer: Option<String> = match &self.auth {
                Auth::None => None,
                Auth::ApiKey(key) => Some(key.clone()),
                #[cfg(feature = "oauth")]
                Auth::Session(session) => {
                    let (token, generation) = session.fresh_token(self).await?;
                    session_generation = Some(generation);
                    Some(token)
                }
            };
            let result = self.dispatch(&spec, bearer.as_deref()).await;
            match result {
                Ok(response) if response.status().is_success() => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!(
                        method = %spec.method,
                        path = %spec.path,
                        status = response.status().as_u16(),
                        "request completed"
                    );
                    return Ok(response);
                }
                Ok(response) => {
                    // A 401 on a refreshing session gets one rotation and an
                    // immediate replay, outside the retry budget.
                    #[cfg(feature = "oauth")]
                    if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
                        if let (Auth::Session(session), Some(generation)) =
                            (&self.auth, session_generation)
                        {
                            session.refresh_stale(self, generation).await?;
                            refreshed = true;
                            continue;
                        }
                    }
                    if attempt < self.max_retries
                        && retryable_status(&spec.method, response.status())
                    {
                        attempt += 1;
                        let wait = backoff_delay(attempt, retry_after_header(&response));
                        #[cfg(feature = "tracing")]
                        tracing::debug!(
                            path = %spec.path,
                            status = response.status().as_u16(),
                            attempt,
                            wait_ms = wait.as_millis() as u64,
                            "retrying"
                        );
                        sleep(wait).await;
                        continue;
                    }
                    return Err(Error::Api(Box::new(map_error(response).await)));
                }
                Err(err) => {
                    if attempt < self.max_retries && retryable_transport(&spec.method, &err) {
                        attempt += 1;
                        let wait = backoff_delay(attempt, None);
                        #[cfg(feature = "tracing")]
                        tracing::debug!(
                            path = %spec.path,
                            error = %err,
                            attempt,
                            wait_ms = wait.as_millis() as u64,
                            "retrying after transport error"
                        );
                        sleep(wait).await;
                        continue;
                    }
                    return Err(Error::Transport(err));
                }
            }
        }
    }

    async fn dispatch(
        &self,
        spec: &RequestSpec,
        bearer: Option<&str>,
    ) -> Result<Response, reqwest::Error> {
        let url = format!("{}{}", self.base_url, spec.path);
        let mut request = self
            .http
            .request(spec.method.clone(), url)
            .header("X-Spoo-Client", &self.client_tag);
        #[cfg(not(target_arch = "wasm32"))]
        {
            request = request.timeout(self.timeout);
        }
        if !spec.query.is_empty() {
            request = request.query(&spec.query);
        }
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        if let Some(body) = &spec.body {
            request = request.json(body);
        }
        request.send().await
    }
}

/// Whether a failed status is worth another attempt. Idempotent methods
/// retry the full transient set; POST and PATCH retry only where the server
/// provably did no work, so a replay can never duplicate a link.
fn retryable_status(method: &Method, status: StatusCode) -> bool {
    let transient = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504);
    if idempotent(method) {
        transient
    } else {
        matches!(status.as_u16(), 429 | 503)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn retryable_transport(method: &Method, err: &reqwest::Error) -> bool {
    if idempotent(method) {
        err.is_connect() || err.is_timeout() || err.is_request()
    } else {
        // The request may have reached the server: replaying a POST after an
        // ambiguous failure risks duplicate side effects. A connect error is
        // the one case where nothing was sent.
        err.is_connect()
    }
}

// On wasm the host runtime owns connections and reqwest exposes no error
// classification, so nothing distinguishes "never sent" from "maybe
// processed". Transport failures surface immediately; status-based retries
// still apply.
#[cfg(target_arch = "wasm32")]
fn retryable_transport(_method: &Method, _err: &reqwest::Error) -> bool {
    false
}

fn idempotent(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::PUT | Method::DELETE | Method::HEAD
    )
}

fn retry_after_header(response: &Response) -> Option<Duration> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_retry_after(v, Utc::now()))
}

/// Jittered exponential backoff: 0.5s, 1s, 2s, ... capped at 8s, scaled to
/// 50-100% of the base. A server-sent `Retry-After` overrides the computed
/// wait.
fn backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    if let Some(wait) = retry_after {
        return wait;
    }
    let exp = attempt.saturating_sub(1).min(16);
    let base_ms = (500u64.saturating_mul(1u64 << exp)).min(8_000);
    // Cheap jitter without a rand dependency: sub-millisecond clock noise.
    let noise = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()))
        .unwrap_or(0);
    let jittered = base_ms / 2 + noise % (base_ms / 2 + 1);
    Duration::from_millis(jittered)
}

async fn sleep(duration: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(duration).await;
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::sleep(duration).await;
}

/// Wire shape of the backend's error envelope.
#[derive(serde::Deserialize)]
struct ErrorEnvelope {
    error: String,
    code: String,
    #[serde(default)]
    field: Option<String>,
    #[serde(default)]
    details: Option<serde_json::Value>,
}

/// Build an [`ApiError`] from an error response. When the body is not the
/// JSON error envelope (a proxy-composed HTML page, say), the message is
/// `HTTP {status}` and the raw text is preserved separately: a web page is
/// never an error message.
async fn map_error(response: Response) -> ApiError {
    let status = response.status().as_u16();
    let header_code = response
        .headers()
        .get("x-error-code")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let rate_limit = parse_rate_limit(&response);
    let text = response.text().await.unwrap_or_default();
    match serde_json::from_str::<ErrorEnvelope>(&text) {
        Ok(envelope) => ApiError {
            status,
            code: if envelope.code.is_empty() {
                header_code.unwrap_or_default()
            } else {
                envelope.code
            },
            message: envelope.error,
            field: envelope.field,
            details: envelope.details,
            request_id,
            rate_limit,
            body: None,
        },
        Err(_) => ApiError {
            status,
            code: header_code.unwrap_or_default(),
            message: format!("HTTP {status}"),
            field: None,
            details: None,
            request_id,
            rate_limit,
            body: (!text.is_empty()).then_some(text),
        },
    }
}

fn parse_rate_limit(response: &Response) -> RateLimit {
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    RateLimit {
        limit: header("x-ratelimit-limit").and_then(|v| v.parse().ok()),
        remaining: header("x-ratelimit-remaining").and_then(|v| v.parse().ok()),
        reset: header("x-ratelimit-reset")
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0)),
        retry_after: header("retry-after").and_then(|v| parse_retry_after(&v, Utc::now())),
    }
}

/// Reduce a server-suggested filename to a safe bare filename.
///
/// Wire-supplied paths are untrusted: consumers hand this value to
/// `File::create` or path joins, so anything that could escape a directory
/// is rejected and the caller's fallback is used instead.
pub(crate) fn sanitize_filename(raw: &str, fallback: &str) -> String {
    let candidate = raw.trim().trim_matches('"');
    let absolute = candidate.starts_with('/') || candidate.starts_with('\\');
    let base = candidate
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    if absolute || base.is_empty() || base == "." || base == ".." {
        fallback.to_owned()
    } else {
        base.to_owned()
    }
}

/// Extract and sanitize the filename from a `Content-Disposition` header,
/// preferring the RFC 5987 `filename*` form.
pub(crate) fn content_disposition_filename(header: Option<&str>, fallback: &str) -> String {
    let Some(header) = header else {
        return fallback.to_owned();
    };
    let mut plain: Option<String> = None;
    let mut extended: Option<String> = None;
    for part in header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("filename*=") {
            // RFC 5987: charset'lang'percent-encoded-value
            let encoded = value.rsplit('\'').next().unwrap_or_default();
            extended = Some(percent_decode(encoded));
        } else if let Some(value) = part.strip_prefix("filename=") {
            plain = Some(value.trim_matches('"').to_owned());
        }
    }
    match extended.or(plain) {
        Some(name) => sanitize_filename(&name, fallback),
        None => fallback.to_owned(),
    }
}

fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let mut bytes = input.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hi = bytes.peek().copied();
            if let Some(hi) = hi.filter(u8::is_ascii_hexdigit) {
                bytes.next();
                if let Some(lo) = bytes.peek().copied().filter(u8::is_ascii_hexdigit) {
                    bytes.next();
                    let hex = [hi, lo];
                    let hex = std::str::from_utf8(&hex).unwrap_or("");
                    if let Ok(decoded) = u8::from_str_radix(hex, 16) {
                        out.push(decoded);
                        continue;
                    }
                }
                out.push(b'%');
                out.push(hi);
                continue;
            }
        }
        out.push(byte);
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_filenames_fall_back() {
        let f = "spoo-export.json";
        assert_eq!(sanitize_filename("../../../evil.json", f), "evil.json");
        assert_eq!(sanitize_filename("/tmp/absolute-evil.json", f), f);
        assert_eq!(sanitize_filename("..\\..\\evil.json", f), "evil.json");
        assert_eq!(sanitize_filename("..", f), f);
        assert_eq!(sanitize_filename(".", f), f);
        assert_eq!(sanitize_filename("", f), f);
        assert_eq!(sanitize_filename("report.csv", f), "report.csv");
    }

    #[test]
    fn content_disposition_all_forms() {
        let f = "spoo-export.json";
        assert_eq!(
            content_disposition_filename(Some(r#"attachment; filename="stats.json""#), f),
            "stats.json"
        );
        assert_eq!(
            content_disposition_filename(
                Some("attachment; filename*=utf-8''%2e%2e%2f%2e%2e%2fesc.json"),
                f
            ),
            "esc.json"
        );
        assert_eq!(
            content_disposition_filename(Some(r#"attachment; filename="../../../evil.json""#), f),
            "evil.json"
        );
        assert_eq!(content_disposition_filename(None, f), f);
    }

    #[test]
    fn backoff_caps_at_eight_seconds() {
        for attempt in 1..=10 {
            let d = backoff_delay(attempt, None);
            assert!(
                d <= Duration::from_millis(8_000),
                "attempt {attempt}: {d:?}"
            );
            assert!(d >= Duration::from_millis(250), "attempt {attempt}: {d:?}");
        }
        assert_eq!(
            backoff_delay(1, Some(Duration::from_secs(3))),
            Duration::from_secs(3)
        );
    }

    #[test]
    fn post_only_retries_where_server_did_no_work() {
        assert!(retryable_status(
            &Method::GET,
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(retryable_status(&Method::GET, StatusCode::REQUEST_TIMEOUT));
        assert!(!retryable_status(
            &Method::POST,
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!retryable_status(&Method::POST, StatusCode::BAD_GATEWAY));
        assert!(retryable_status(
            &Method::POST,
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(retryable_status(
            &Method::POST,
            StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(!retryable_status(
            &Method::PATCH,
            StatusCode::GATEWAY_TIMEOUT
        ));
    }
}
