//! Error types returned by the client.

use std::time::Duration;

use chrono::{DateTime, Utc};

/// Rate-limit state parsed from the `X-RateLimit-*` and `Retry-After`
/// response headers. The backend reports the shortest rate-limit window
/// that applies to the endpoint.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct RateLimit {
    /// Request budget of the reported window.
    pub limit: Option<u64>,
    /// How much of the budget is left.
    pub remaining: Option<u64>,
    /// When the reported window resets.
    pub reset: Option<DateTime<Utc>>,
    /// Server-mandated wait, sent on 429 responses.
    pub retry_after: Option<Duration>,
}

/// The backend's error envelope (`{error, code, field, details}`) plus the
/// response metadata that matters for handling a failure programmatically.
#[derive(Debug)]
#[non_exhaustive]
pub struct ApiError {
    /// HTTP status of the response.
    pub status: u16,
    /// The backend's machine-readable error code: an open string enum in
    /// lowercase snake_case such as `conflict`, `authentication_error`,
    /// `not_found`, `rate_limit_exceeded`, `payload_too_large`, `blocked`,
    /// `gone` (the one uppercase outlier is `EMAIL_NOT_VERIFIED`). Read from
    /// the body, with the `X-Error-Code` header as fallback for
    /// edge-composed responses whose bodies carry no envelope.
    pub code: String,
    /// Human-readable error message. When the response body is not the JSON
    /// error envelope (for example proxy-composed HTML), this is
    /// `HTTP {status}` and the raw text is preserved in [`ApiError::body`].
    pub message: String,
    /// Names the offending request field on validation errors.
    pub field: Option<String>,
    /// Structured context for the error, when the backend attaches any.
    pub details: Option<serde_json::Value>,
    /// The `X-Request-ID` header, for support correlation.
    pub request_id: Option<String>,
    /// Parsed rate-limit headers.
    pub rate_limit: RateLimit,
    /// Raw response text, preserved only when the body was not the JSON
    /// error envelope.
    pub body: Option<String>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{} (field {:?})", self.message, field),
            None => write!(f, "{}", self.message),
        }
    }
}

/// Errors returned by every fallible operation in this crate.
///
/// This crate does not panic: every failure surfaces as one of these
/// variants. Callers who only want to branch on common conditions can use
/// the predicate methods ([`Error::is_not_found`], [`Error::is_rate_limited`],
/// [`Error::is_blocked`], ...) instead of matching variants.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The API answered with an error status. Carries the parsed error
    /// envelope and response metadata (boxed to keep `Result` small).
    #[error("spoo.me API error ({status}): {inner}", status = .0.status, inner = .0)]
    Api(Box<ApiError>),

    /// The request never produced a usable response: connection failures,
    /// timeouts, TLS errors.
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    /// A 2xx response body did not decode into the expected shape. Usually
    /// means the SDK is behind the server: check for a newer release.
    #[error("failed to decode response body: {0}")]
    Decode(#[source] serde_json::Error),

    /// A refreshing session's refresh token no longer works. The only
    /// recovery is a fresh sign-in.
    #[error("session expired: refresh token was rejected")]
    SessionExpired,

    /// The client was misconfigured (for example an unparsable base URL).
    /// Returned by builders before any request goes out.
    #[error("configuration error: {0}")]
    Config(String),
}

impl Error {
    /// The parsed API error, when this is [`Error::Api`].
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Error::Api(e) => Some(e),
            _ => None,
        }
    }

    /// HTTP status of the failing response, when the API answered at all.
    pub fn status(&self) -> Option<u16> {
        self.api().map(|e| e.status)
    }

    /// The backend's machine-readable error code, when the API answered.
    pub fn code(&self) -> Option<&str> {
        self.api().map(|e| e.code.as_str())
    }

    /// Whether this is an API 404: no such resource, or not yours (the
    /// resolve-first endpoints deliberately answer both the same way).
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }

    /// Whether this is an API 429. The client has already retried, so a 429
    /// surfacing here means the budget is truly gone; the wait to observe is
    /// in [`Error::retry_after`].
    pub fn is_rate_limited(&self) -> bool {
        self.status() == Some(429)
    }

    /// Whether this is an API 451: the link was taken down because its
    /// destination was flagged. A verdict on the link, not a transient
    /// failure.
    pub fn is_blocked(&self) -> bool {
        self.status() == Some(451)
    }

    /// Whether this failure is a property of the link, not the session: the
    /// link's stats require the link password.
    pub fn is_password_required(&self) -> bool {
        self.status() == Some(401)
            && self
                .code()
                .is_some_and(|c| c == "password_required" || c == "invalid_password")
    }

    /// The server-mandated wait from the `Retry-After` header, when present.
    pub fn retry_after(&self) -> Option<Duration> {
        self.api().and_then(|e| e.rate_limit.retry_after)
    }
}

/// Parse a `Retry-After` header value. RFC 9110 allows two forms:
/// delay-seconds and an HTTP-date. Never fails: an unparsable value is
/// `None`.
pub(crate) fn parse_retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    if let Ok(when) = DateTime::parse_from_rfc2822(value) {
        let delta = when.with_timezone(&Utc) - now;
        return delta.to_std().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_parses_both_legal_forms() {
        let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            parse_retry_after("120", now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            parse_retry_after("Thu, 01 Jan 2026 00:00:30 GMT", now),
            Some(Duration::from_secs(30))
        );
        // A date in the past yields no wait rather than an error.
        assert_eq!(
            parse_retry_after("Wed, 31 Dec 2025 23:59:00 GMT", now),
            None
        );
        assert_eq!(parse_retry_after("not-a-value", now), None);
        assert_eq!(parse_retry_after("", now), None);
    }
}
