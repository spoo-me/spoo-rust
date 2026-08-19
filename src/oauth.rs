//! The client half of Sign in with Spoo (authorization-code + PKCE).
//!
//! The SDK never opens browsers, renders consent, or stores secrets; it
//! provides the protocol pieces and a self-refreshing [`Session`] credential
//! for [`crate::ClientBuilder::session`]. Enabled by the `oauth` feature.
//!
//! [`Session`]: crate::oauth::Session

use std::time::Duration;

use base64::Engine as _;
use reqwest::Method;
use serde::Deserialize;
use sha2::Digest as _;

use crate::client::Client;
use crate::error::Error;
use crate::http::{RequestSpec, Transport};
use crate::resources::auth::User;

const VERIFIER_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// A PKCE verifier and its S256 challenge.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PkcePair {
    /// The secret half: goes into the token exchange.
    pub verifier: String,
    /// The public half: goes into the authorization URL.
    pub challenge: String,
}

/// Generate a PKCE pair (RFC 7636, S256). The verifier is 64 characters
/// from the unreserved set.
pub fn generate_pkce_pair() -> PkcePair {
    let verifier = random_string(64);
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkcePair {
        verifier,
        challenge,
    }
}

/// Generate a CSRF-binding `state` value for the authorization URL.
pub fn generate_state() -> String {
    random_string(32)
}

fn random_string(len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rand::Rng::random_range(&mut rng, 0..VERIFIER_CHARS.len());
            // The index is in range by construction; the fallback keeps the
            // panic-free guarantee honest.
            char::from(VERIFIER_CHARS.get(idx).copied().unwrap_or(b'a'))
        })
        .collect()
}

/// An access/refresh token pair. Refresh tokens rotate: after a refresh the
/// pair you held before is dead.
///
/// `Debug` deliberately redacts both tokens: a refresh token in a log line
/// is a long-lived credential leak.
#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct TokenPair {
    /// JWT access token.
    pub access_token: String,
    /// JWT refresh token.
    pub refresh_token: String,
}

impl TokenPair {
    /// Assemble a pair from stored values.
    pub fn new(access_token: impl Into<String>, refresh_token: impl Into<String>) -> Self {
        TokenPair {
            access_token: access_token.into(),
            refresh_token: refresh_token.into(),
        }
    }
}

impl std::fmt::Debug for TokenPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenPair")
            .field("access_token", &"[redacted]")
            .field("refresh_token", &"[redacted]")
            .finish()
    }
}

/// The result of a device-code exchange: tokens plus the signed-in user.
///
/// `Debug` deliberately redacts both tokens.
#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct DeviceTokens {
    /// JWT access token.
    pub access_token: String,
    /// JWT refresh token.
    pub refresh_token: String,
    /// The signed-in user's profile.
    pub user: User,
}

impl DeviceTokens {
    /// The token pair, for building a [`Session`].
    pub fn tokens(&self) -> TokenPair {
        TokenPair::new(self.access_token.clone(), self.refresh_token.clone())
    }
}

impl std::fmt::Debug for DeviceTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceTokens")
            .field("access_token", &"[redacted]")
            .field("refresh_token", &"[redacted]")
            .field("user", &self.user)
            .finish()
    }
}

/// Sign in with Spoo protocol calls, from [`crate::Client::oauth`].
pub struct OAuth {
    pub(crate) client: Client,
}

impl OAuth {
    /// Build the consent-page URL your app opens in a browser. S256 is
    /// mandatory; there is no plain fallback.
    pub fn authorization_url(
        &self,
        app_id: impl Into<String>,
        state: impl Into<String>,
        code_challenge: impl Into<String>,
    ) -> AuthorizationUrlBuilder {
        AuthorizationUrlBuilder {
            base_url: self.client.transport.base_url.clone(),
            app_id: app_id.into(),
            state: state.into(),
            code_challenge: code_challenge.into(),
            redirect_uri: None,
        }
    }

    /// Exchange the one-time code from the callback for tokens. The code
    /// and verifier are the credentials; no auth header is involved.
    pub async fn exchange_code(
        &self,
        code: impl Into<String>,
        code_verifier: impl Into<String>,
    ) -> Result<DeviceTokens, Error> {
        let spec =
            RequestSpec::new(Method::POST, "/auth/device/token").json(&serde_json::json!({
                "code": code.into(),
                "code_verifier": code_verifier.into(),
            }))?;
        self.client.transport.execute(spec).await
    }

    /// Trade a refresh token for a fresh pair. The pair you sent is invalid
    /// afterwards. Prefer a [`Session`], which handles rotation, persistence
    /// and retry for you.
    pub async fn refresh_tokens(&self, refresh_token: &str) -> Result<TokenPair, Error> {
        refresh_call(&self.client.transport, refresh_token).await
    }
}

/// Builder for [`OAuth::authorization_url`].
#[must_use = "the URL is only produced by .build()"]
pub struct AuthorizationUrlBuilder {
    base_url: String,
    app_id: String,
    state: String,
    code_challenge: String,
    redirect_uri: Option<String>,
}

impl AuthorizationUrlBuilder {
    /// Must exactly match a redirect URI registered for the app; the server
    /// rejects everything else, including a different port. Omit to use the
    /// app's registered default.
    pub fn redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(uri.into());
        self
    }

    /// Produce the URL.
    pub fn build(self) -> Result<String, Error> {
        let mut url = reqwest::Url::parse(&format!("{}/auth/device/login", self.base_url))
            .map_err(|e| Error::Config(format!("invalid base URL: {e}")))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("app_id", &self.app_id);
            if let Some(redirect_uri) = &self.redirect_uri {
                query.append_pair("redirect_uri", redirect_uri);
            }
            query.append_pair("state", &self.state);
            query.append_pair("code_challenge", &self.code_challenge);
            query.append_pair("code_challenge_method", "S256");
        }
        Ok(url.into())
    }
}

/// The refresh endpoint answers 400 or 401 when the refresh token is dead:
/// both mean the session is over.
async fn refresh_call(transport: &Transport, refresh_token: &str) -> Result<TokenPair, Error> {
    let spec = RequestSpec::new(Method::POST, "/auth/device/refresh")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))?;
    match transport.execute_unauthenticated::<TokenPair>(spec).await {
        Ok(pair) => Ok(pair),
        Err(Error::Api(e)) if e.status == 400 || e.status == 401 => Err(Error::SessionExpired),
        Err(err) => Err(err),
    }
}

type OnRefresh = Box<dyn Fn(&TokenPair) + Send + Sync>;

struct SessionState {
    tokens: TokenPair,
    generation: u64,
    /// Unix seconds the access token expires at, from its `exp` claim.
    expires_at: Option<i64>,
}

/// A self-refreshing Sign in with Spoo credential.
///
/// Attach it with [`crate::ClientBuilder::session`]. The client refreshes
/// proactively shortly before the access token's `exp`, retries once after
/// an unexpected 401, and rotations are single-flight: concurrent requests
/// share one refresh, so a rotated pair is never persisted twice. Every
/// rotation is reported through the `on_refresh` hook; persist the pair
/// there, because the previous refresh token is dead the moment it fires.
pub struct Session {
    state: tokio::sync::Mutex<SessionState>,
    on_refresh: Option<OnRefresh>,
    expiry_skew: Duration,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

impl Session {
    /// A session from a stored or freshly exchanged token pair.
    pub fn new(tokens: TokenPair) -> Session {
        let expires_at = decode_exp(&tokens.access_token);
        Session {
            state: tokio::sync::Mutex::new(SessionState {
                tokens,
                generation: 0,
                expires_at,
            }),
            on_refresh: None,
            expiry_skew: Duration::from_secs(30),
        }
    }

    /// Called after every successful refresh with the rotated pair.
    /// Persist it there: the previous refresh token is already dead. The
    /// hook runs on the async executor, so keep it quick; move slow writes
    /// onto a blocking task.
    pub fn on_refresh(mut self, hook: impl Fn(&TokenPair) + Send + Sync + 'static) -> Session {
        self.on_refresh = Some(Box::new(hook));
        self
    }

    /// How long before the access token's `exp` to refresh proactively.
    /// Default 30 seconds.
    pub fn expiry_skew(mut self, skew: Duration) -> Session {
        self.expiry_skew = skew;
        self
    }

    /// Mark the current access token stale, so the next request refreshes
    /// first.
    pub async fn invalidate(&self) {
        let mut state = self.state.lock().await;
        state.expires_at = Some(0);
    }

    /// A fresh access token and the rotation generation it belongs to.
    /// Refreshes first when the token is at or past `exp - skew`.
    pub(crate) async fn fresh_token(&self, transport: &Transport) -> Result<(String, u64), Error> {
        let mut state = self.state.lock().await;
        let stale = state
            .expires_at
            .is_some_and(|exp| utc_now_secs() + self.expiry_skew.as_secs() as i64 >= exp);
        let rotated = if stale {
            Some(self.rotate_locked(&mut state, transport).await?)
        } else {
            None
        };
        let result = (state.tokens.access_token.clone(), state.generation);
        // The hook runs outside the lock: a synchronous persistence write in
        // it must not stall every other request sharing this session.
        drop(state);
        if let Some(pair) = rotated {
            self.notify(&pair);
        }
        Ok(result)
    }

    /// Refresh after a 401, unless another task already rotated past the
    /// generation this request was sent with.
    pub(crate) async fn refresh_stale(
        &self,
        transport: &Transport,
        seen_generation: u64,
    ) -> Result<(), Error> {
        let mut state = self.state.lock().await;
        if state.generation != seen_generation {
            return Ok(());
        }
        let pair = self.rotate_locked(&mut state, transport).await?;
        drop(state);
        self.notify(&pair);
        Ok(())
    }

    async fn rotate_locked(
        &self,
        state: &mut SessionState,
        transport: &Transport,
    ) -> Result<TokenPair, Error> {
        let pair = refresh_call(transport, &state.tokens.refresh_token).await?;
        state.expires_at = decode_exp(&pair.access_token);
        state.tokens = pair.clone();
        state.generation += 1;
        Ok(pair)
    }

    fn notify(&self, pair: &TokenPair) {
        if let Some(hook) = &self.on_refresh {
            hook(pair);
        }
    }
}

fn utc_now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Best-effort read of a JWT's `exp` claim. `None` disables proactive
/// refresh; the 401-retry path still works.
fn decode_exp(jwt: &str) -> Option<i64> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get("exp")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_matches_rfc_7636_s256() {
        let pair = generate_pkce_pair();
        assert_eq!(pair.verifier.len(), 64);
        assert!(pair.verifier.bytes().all(|b| VERIFIER_CHARS.contains(&b)));
        let digest = sha2::Sha256::digest(pair.verifier.as_bytes());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        assert_eq!(pair.challenge, expected);
        // No padding, URL-safe alphabet.
        assert!(!pair.challenge.contains('='));
    }

    #[test]
    fn exp_claim_decodes() {
        // {"exp": 1234567890}
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1234567890}"#);
        let jwt = format!("h.{payload}.s");
        assert_eq!(decode_exp(&jwt), Some(1234567890));
        assert_eq!(decode_exp("not-a-jwt"), None);
    }
}
