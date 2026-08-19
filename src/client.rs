//! The client and its builder.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::http::{Auth, RequestSpec, Transport};
use crate::resources::{
    auth::AuthResource, emoji::Emoji, links::Links, public::Public, stats::Stats,
};

/// Default production endpoint.
pub const DEFAULT_BASE_URL: &str = "https://spoo.me";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RETRIES: u32 = 2;

/// The spoo.me API client.
///
/// Cheap to clone (`Arc` inside) and safe to share across tasks.
///
/// ```no_run
/// # async fn run() -> Result<(), spoo_me::Error> {
/// let client = spoo_me::Client::new("spoo_your_api_key");
/// let link = client.links().create("https://example.com/launch").send().await?;
/// println!("{}", link.short_url);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Client {
    pub(crate) transport: Arc<Transport>,
    pub(crate) emoji_cache: Arc<std::sync::Mutex<Option<crate::resources::emoji::CachedSet>>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.transport.base_url)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// A client authenticating with an API key (`spoo_...`).
    pub fn new(api_key: impl Into<String>) -> Client {
        Client::builder().api_key(api_key).build_unchecked()
    }

    /// A client with no credentials, for the public endpoints (anonymous
    /// shortening, public stats, previews, the emoji set).
    pub fn anonymous() -> Client {
        Client::builder().build_unchecked()
    }

    /// A client reading its API key from the `SPOO_API_KEY` environment
    /// variable. Environment access happens here and nowhere else: the
    /// constructors never read configuration you did not ask for.
    pub fn from_env() -> Result<Client, Error> {
        match std::env::var("SPOO_API_KEY") {
            Ok(key) if !key.is_empty() => Ok(Client::new(key)),
            _ => Err(Error::Config(
                "SPOO_API_KEY is not set; pass a key to Client::new instead".into(),
            )),
        }
    }

    /// Start configuring a client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Link management: create, list, update, delete, bulk operations,
    /// claiming anonymous links.
    pub fn links(&self) -> Links {
        Links {
            client: self.clone(),
        }
    }

    /// Click statistics and exports.
    pub fn stats(&self) -> Stats {
        Stats {
            client: self.clone(),
        }
    }

    /// Public, unauthenticated link surfaces: stats pages and previews.
    pub fn public(&self) -> Public {
        Public {
            client: self.clone(),
        }
    }

    /// The emoji-alias catalogue and its policy caps.
    pub fn emoji(&self) -> Emoji {
        Emoji {
            client: self.clone(),
        }
    }

    /// Identity: who this client is signed in as.
    pub fn auth(&self) -> AuthResource {
        AuthResource {
            client: self.clone(),
        }
    }

    /// Sign in with Spoo: device-code exchange and refreshing sessions.
    #[cfg(feature = "oauth")]
    pub fn oauth(&self) -> crate::oauth::OAuth {
        crate::oauth::OAuth {
            client: self.clone(),
        }
    }

    /// Raw typed `GET` with the client's auth, retries, timeout and error
    /// mapping applied: the supported pressure valve for endpoints this SDK
    /// does not cover yet. Needing it is a signal worth an issue on the SDK.
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, Error> {
        let mut spec = RequestSpec::new(Method::GET, path);
        for (key, value) in query {
            spec.query.push(((*key).to_owned(), (*value).to_owned()));
        }
        self.transport.execute(spec).await
    }

    /// Raw typed `POST`. See [`Client::get`].
    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T, Error> {
        let spec = RequestSpec::new(Method::POST, path).json(body)?;
        self.transport.execute(spec).await
    }

    /// Raw typed `PATCH`. See [`Client::get`].
    pub async fn patch<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T, Error> {
        let spec = RequestSpec::new(Method::PATCH, path).json(body)?;
        self.transport.execute(spec).await
    }

    /// Raw typed `DELETE`. See [`Client::get`].
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let spec = RequestSpec::new(Method::DELETE, path);
        self.transport.execute(spec).await
    }
}

/// Configures and builds a [`Client`].
#[derive(Default)]
pub struct ClientBuilder {
    api_key: Option<String>,
    #[cfg(feature = "oauth")]
    session: Option<Arc<crate::oauth::Session>>,
    base_url: Option<String>,
    http_client: Option<reqwest::Client>,
    max_retries: Option<u32>,
    timeout: Option<Duration>,
    client_tag: Option<String>,
}

impl ClientBuilder {
    /// Authenticate with an API key (`spoo_...`).
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Authenticate with a refreshing Sign in with Spoo session.
    #[cfg(feature = "oauth")]
    pub fn session(mut self, session: Arc<crate::oauth::Session>) -> Self {
        self.session = Some(session);
        self
    }

    /// Point at a self-hosted instance instead of `https://spoo.me`.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Inject a configured `reqwest::Client` (shared connection pool,
    /// proxies, custom TLS).
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Retries after the first attempt. Default 2.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Per-request timeout. Default 30 seconds. Ignored on wasm, where the
    /// host runtime owns request lifetimes.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Override the `X-Spoo-Client` identification header, e.g.
    /// `"my-app/1.0"`. Defaults to this SDK's own tag.
    pub fn client_tag(mut self, tag: impl Into<String>) -> Self {
        self.client_tag = Some(tag.into());
        self
    }

    /// Build the client, validating configuration.
    pub fn build(self) -> Result<Client, Error> {
        if let Some(url) = &self.base_url {
            let parsed = reqwest::Url::parse(url)
                .map_err(|e| Error::Config(format!("base_url {url:?} is not a valid URL: {e}")))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(Error::Config(format!(
                    "base_url must use http or https, got {url:?}"
                )));
            }
            if parsed.host_str().is_none() {
                return Err(Error::Config(format!("base_url {url:?} has no host")));
            }
        }
        Ok(self.build_unchecked())
    }

    fn build_unchecked(self) -> Client {
        let auth = {
            #[cfg(feature = "oauth")]
            if let Some(session) = self.session {
                Auth::Session(session)
            } else if let Some(key) = self.api_key {
                Auth::ApiKey(key)
            } else {
                Auth::None
            }
            #[cfg(not(feature = "oauth"))]
            if let Some(key) = self.api_key {
                Auth::ApiKey(key)
            } else {
                Auth::None
            }
        };
        let base_url = self
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned())
            .trim_end_matches('/')
            .to_owned();
        let transport = Transport {
            injected_http: self.http_client,
            lazy_http: std::sync::OnceLock::new(),
            base_url,
            auth,
            client_tag: self
                .client_tag
                .unwrap_or_else(|| format!("sdk-rust/{}", env!("CARGO_PKG_VERSION"))),
            max_retries: self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES),
            timeout: self.timeout.unwrap_or(DEFAULT_TIMEOUT),
        };
        Client {
            transport: Arc::new(transport),
            emoji_cache: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_is_send_sync_clone() {
        fn assert_traits<T: Send + Sync + Clone>() {}
        assert_traits::<Client>();
    }

    #[test]
    fn builder_rejects_bad_base_url() {
        assert!(Client::builder().base_url("spoo.me").build().is_err());
        assert!(Client::builder().base_url("http://").build().is_err());
        assert!(Client::builder().base_url("https://?").build().is_err());
        assert!(Client::builder().base_url("ftp://spoo.me").build().is_err());
        assert!(
            Client::builder()
                .base_url("https://spoo.me/")
                .build()
                .is_ok()
        );
    }
}
