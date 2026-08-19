//! The emoji-alias catalogue and its policy caps.

use reqwest::Method;
use serde::Deserialize;

use crate::client::Client;
use crate::error::Error;
use crate::http::RequestSpec;

/// The emoji catalogue, from [`crate::Client::emoji`].
pub struct Emoji {
    pub(crate) client: Client,
}

/// One accepted emoji, enriched for client-side search.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct EmojiEntry {
    /// Raw canonical emoji character (no `U+FE0F` variation selector),
    /// matching how aliases are stored and echoed.
    #[serde(rename = "c")]
    pub character: String,
    /// Human-readable name, lowercased with spaces: the primary search key.
    #[serde(rename = "n")]
    pub name: String,
    /// Canonical Unicode category display name, for picker tabs. Entries
    /// arrive sorted by category and within-category order.
    #[serde(rename = "g")]
    pub group: String,
    /// Whether this emoji is in the auto-generation pool.
    #[serde(rename = "gen")]
    pub generates: bool,
    /// Extra search aliases, when the source lists any.
    #[serde(rename = "k", default)]
    pub keywords: Option<Vec<String>>,
}

/// The accepted emoji catalogue and its policy caps.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct EmojiSet {
    /// Newest Unicode emoji version a custom alias may use.
    pub accept_max_version: f64,
    /// Cap for auto-generated aliases (lower, for older platform coverage).
    pub generate_max_version: f64,
    /// Maximum emoji graphemes in one alias.
    pub max_graphemes: u64,
    /// Every single-codepoint emoji a user may choose, with names and
    /// categories. Skin-tone variants are not enumerated: the base emoji
    /// suffices.
    pub emoji: Vec<EmojiEntry>,
}

pub(crate) struct CachedSet {
    pub(crate) etag: String,
    pub(crate) set: EmojiSet,
}

impl Emoji {
    /// Fetch the catalogue. The set changes rarely, so the client caches it
    /// with the server's ETag and revalidates with `If-None-Match`: a 304
    /// answers from cache without re-downloading the list.
    pub async fn set(&self) -> Result<EmojiSet, Error> {
        let cached_etag = {
            let cache = lock(&self.client.emoji_cache);
            cache.as_ref().map(|c| c.etag.clone())
        };
        match cached_etag {
            Some(etag) => self.fetch_with_etag(&etag).await,
            None => {
                let response = self
                    .client
                    .transport
                    .send(RequestSpec::new(Method::GET, "/api/v1/emoji-set"))
                    .await?;
                self.store(response).await
            }
        }
    }

    /// Conditional revalidation through the normal transport, so auth,
    /// retries, timeout and error mapping all still apply; the transport
    /// treats 304 as success on purpose.
    async fn fetch_with_etag(&self, etag: &str) -> Result<EmojiSet, Error> {
        let spec = RequestSpec::new(Method::GET, "/api/v1/emoji-set").header("If-None-Match", etag);
        let response = self.client.transport.send(spec).await?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            {
                let cache = lock(&self.client.emoji_cache);
                if let Some(cached) = cache.as_ref() {
                    return Ok(cached.set.clone());
                }
            }
            // The cache vanished between requests: a 304 has no body, so
            // refetch unconditionally.
            let fresh = self
                .client
                .transport
                .send(RequestSpec::new(Method::GET, "/api/v1/emoji-set"))
                .await?;
            return self.store(fresh).await;
        }
        self.store(response).await
    }

    async fn store(&self, response: reqwest::Response) -> Result<EmojiSet, Error> {
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = response.bytes().await.map_err(Error::Transport)?;
        let set: EmojiSet = serde_json::from_slice(&bytes).map_err(Error::Decode)?;
        if let Some(etag) = etag {
            let mut cache = lock(&self.client.emoji_cache);
            *cache = Some(CachedSet {
                etag,
                set: set.clone(),
            });
        }
        Ok(set)
    }
}

fn lock(
    cache: &std::sync::Mutex<Option<CachedSet>>,
) -> std::sync::MutexGuard<'_, Option<CachedSet>> {
    match cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
