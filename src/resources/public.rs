//! Public, unauthenticated link surfaces: stats pages and previews.

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::client::Client;
use crate::error::Error;
use crate::http::{RequestSpec, encode_segment};

/// Public link surfaces, from [`crate::Client::public`].
pub struct Public {
    pub(crate) client: Client,
}

/// Which generation a link belongs to. Old links carry slightly different
/// stats dimensions than new ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum Generation {
    /// A link from the original platform.
    #[serde(rename = "v1")]
    V1,
    /// A link from the current platform.
    #[serde(rename = "v2")]
    V2,
    /// A generation this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// Lifecycle state as the public surfaces report it (lowercase wire form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PublicStatus {
    /// Redirects are served.
    Active,
    /// Redirects are disabled by the owner.
    Inactive,
    /// The link ran out of time or clicks.
    Expired,
    /// Taken down because the destination was flagged.
    Blocked,
    /// A status this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// Public facts about a link, shown above its charts.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct PublicLinkFacts {
    /// Short code.
    pub alias: String,
    /// Full short URL.
    pub short_url: String,
    /// The destination; withheld while the link is not active.
    #[serde(default)]
    pub long_url: Option<String>,
    /// When the link was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Lifecycle state.
    pub status: PublicStatus,
    /// Click limit, if one is set.
    #[serde(default)]
    pub max_clicks: Option<u64>,
    /// Whether known bots are blocked.
    pub block_bots: bool,
    /// Whether the link is password-protected.
    pub password_protected: bool,
}

/// A public stats page's data.
///
/// `stats` is the modern stats wire shape (summary, `{metric}_by_{dimension}`
/// series, time range) kept as raw JSON: v1 and v2 links carry different
/// dimension sets, so the shape is deliberately open.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct PublicStats {
    /// Which generation the link belongs to.
    pub generation: Generation,
    /// Facts about the link.
    pub link: PublicLinkFacts,
    /// The stats body, raw.
    pub stats: serde_json::Map<String, serde_json::Value>,
}

/// A destination URL split into display parts.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct PreviewDestination {
    /// The full destination URL.
    pub url: String,
    /// Its host.
    pub domain: String,
    /// Its path.
    pub path: String,
    /// Whether it is served over https.
    pub is_https: bool,
}

/// One geo-rule destination group: every rule is listed, nothing
/// summarized.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct PreviewGeoDestination {
    /// The full destination URL.
    pub url: String,
    /// Its host.
    pub domain: String,
    /// Its path.
    pub path: String,
    /// Whether it is served over https.
    pub is_https: bool,
    /// ISO 3166-1 alpha-2 codes this destination serves, sorted.
    pub countries: Vec<String>,
}

/// Where a short link leads, without following it. `destination` and
/// `geo_destinations` are present only while the link is active and not
/// password-protected: the preview never reveals a destination the redirect
/// would refuse to serve.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Preview {
    /// Which generation the link belongs to.
    pub generation: Generation,
    /// Short code.
    pub alias: String,
    /// Full short URL.
    pub short_url: String,
    /// Lifecycle state.
    pub status: PublicStatus,
    /// When the link was created.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Whether the link is password-protected.
    pub password_protected: bool,
    /// The default destination.
    #[serde(default)]
    pub destination: Option<PreviewDestination>,
    /// Per-country destinations, when geo rules exist.
    #[serde(default)]
    pub geo_destinations: Option<Vec<PreviewGeoDestination>>,
}

impl Public {
    /// A link's public stats page data. Chain [`PublicStatsBuilder::password`]
    /// for password-protected links.
    pub fn stats(&self, short_code: impl Into<String>) -> PublicStatsBuilder {
        PublicStatsBuilder {
            client: self.client.clone(),
            short_code: short_code.into(),
            password: None,
        }
    }

    /// Where a short link leads, without following it.
    pub async fn preview(&self, short_code: &str) -> Result<Preview, Error> {
        self.client
            .transport
            .execute(RequestSpec::new(
                Method::GET,
                format!("/api/v1/public/preview/{}", encode_segment(short_code)),
            ))
            .await
    }
}

/// Builder for [`Public::stats`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct PublicStatsBuilder {
    client: Client,
    short_code: String,
    password: Option<String>,
}

impl PublicStatsBuilder {
    /// The link password, for password-protected links' stats.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Fetch the stats page data. Sends a plain GET, or a POST carrying the
    /// password when one was provided: one method, both wire forms.
    pub async fn send(self) -> Result<PublicStats, Error> {
        let path = format!("/api/v1/public/stats/{}", encode_segment(&self.short_code));
        let spec = match self.password {
            Some(password) => RequestSpec::new(Method::POST, path)
                .json(&serde_json::json!({ "password": password }))?,
            None => RequestSpec::new(Method::GET, path),
        };
        self.client.transport.execute(spec).await
    }
}
