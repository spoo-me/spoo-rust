//! Link management: create, list, update, delete, bulk operations, claiming.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Error;
use crate::http::RequestSpec;
use crate::page::Page;
use crate::patch::Patch;

/// Link management, from [`crate::Client::links`].
pub struct Links {
    pub(crate) client: Client,
}

/// Lifecycle state of a link. `Expired` and `Blocked` are derived or
/// system-set; callers can only set `Active` and `Inactive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub enum LinkStatus {
    /// Redirects are served.
    #[serde(rename = "ACTIVE")]
    Active,
    /// Redirects are disabled by the owner.
    #[serde(rename = "INACTIVE")]
    Inactive,
    /// The link ran out of time or clicks.
    #[serde(rename = "EXPIRED")]
    Expired,
    /// Taken down because the destination was flagged.
    #[serde(rename = "BLOCKED")]
    Blocked,
    /// A status this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// The two statuses a caller may set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum SettableStatus {
    /// Enable redirects.
    #[serde(rename = "ACTIVE")]
    Active,
    /// Disable redirects.
    #[serde(rename = "INACTIVE")]
    Inactive,
}

/// Alias style to auto-generate when no explicit alias is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum AliasKind {
    /// A short alphanumeric code (the default).
    #[serde(rename = "alphanumeric")]
    Alphanumeric,
    /// An emoji sequence.
    #[serde(rename = "emoji")]
    Emoji,
}

/// A custom social preview (og:title, og:description, og:image,
/// theme-color) served to link-preview crawlers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetaTags {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

impl MetaTags {
    /// A preview with the required headline (og:title).
    pub fn new(title: impl Into<String>) -> Self {
        MetaTags {
            title: title.into(),
            description: None,
            image: None,
            color: None,
        }
    }

    /// og:description. Roughly 200 characters render on most platforms.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// og:image: an https URL, or a `data:image/...;base64,` URI stored on
    /// spoo's CDN. 1200x630 recommended, under 300KB.
    pub fn image(mut self, image: impl Into<String>) -> Self {
        self.image = Some(image.into());
        self
    }

    /// Accent color for Discord embeds, `#RRGGBB`.
    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// Custom social-preview settings stored on a link.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct MetaTagsInfo {
    /// og:title.
    pub title: String,
    /// og:description.
    #[serde(default)]
    pub description: Option<String>,
    /// og:image URL.
    #[serde(default)]
    pub image: Option<String>,
    /// Discord embed accent color.
    #[serde(default)]
    pub color: Option<String>,
    /// Non-fatal quality notes, e.g. an image WhatsApp may drop.
    #[serde(default)]
    pub warnings: Option<Vec<String>>,
}

/// A freshly created link (`POST /api/v1/shorten`).
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Link {
    /// The identifier the management endpoints address this link by.
    pub id: String,
    /// Short code.
    pub alias: String,
    /// Full shortened URL, ready to share.
    pub short_url: String,
    /// The destination.
    pub long_url: String,
    /// Owning user, `None` for anonymous creates.
    #[serde(default)]
    pub owner_id: Option<String>,
    /// When the link was created.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    /// Lifecycle state.
    pub status: LinkStatus,
    /// Whether statistics are owner-only.
    #[serde(default)]
    pub private_stats: Option<bool>,
    /// Per-country destination overrides (ISO alpha-2 code to URL).
    #[serde(default)]
    pub geo_rules: Option<HashMap<String, String>>,
    /// Custom social preview, if configured.
    #[serde(default)]
    pub meta_tags: Option<MetaTagsInfo>,
    /// One-time bearer proof of creation, present only on anonymous
    /// creates and shown exactly once. Store it to later attach the link to
    /// an account via [`Links::claim`].
    #[serde(default)]
    pub claim_token: Option<String>,
}

/// A link as returned by the list, get and get-by-address endpoints.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct LinkItem {
    /// The identifier the management endpoints address this link by.
    pub id: String,
    /// Short code.
    #[serde(default)]
    pub alias: Option<String>,
    /// The destination.
    #[serde(default)]
    pub long_url: Option<String>,
    /// Lifecycle state.
    #[serde(default)]
    pub status: Option<LinkStatus>,
    /// When the link was created.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// When the link expires, if an expiry is set.
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub expire_after: Option<DateTime<Utc>>,
    /// Click limit, if one is set.
    #[serde(default)]
    pub max_clicks: Option<u64>,
    /// Whether statistics are owner-only.
    #[serde(default)]
    pub private_stats: Option<bool>,
    /// Whether known bots are blocked.
    #[serde(default)]
    pub block_bots: Option<bool>,
    /// Whether the link is password-protected.
    pub password_set: bool,
    /// Lifetime click count.
    #[serde(default)]
    pub total_clicks: Option<u64>,
    /// Most recent click.
    #[serde(default)]
    pub last_click: Option<DateTime<Utc>>,
    /// Custom domain the link lives on, `None` for the default namespace.
    #[serde(default)]
    pub domain: Option<String>,
    /// Per-country destination overrides.
    #[serde(default)]
    pub geo_rules: Option<HashMap<String, String>>,
    /// Custom social preview, if configured.
    #[serde(default)]
    pub meta_tags: Option<MetaTagsInfo>,
}

/// The link's state after an update (`PATCH /api/v1/urls/{url_id}`).
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct UpdatedLink {
    /// The link's identifier.
    pub id: String,
    /// Short code.
    #[serde(default)]
    pub alias: Option<String>,
    /// The destination.
    #[serde(default)]
    pub long_url: Option<String>,
    /// Lifecycle state.
    #[serde(default)]
    pub status: Option<LinkStatus>,
    /// Whether the link is password-protected.
    pub password_set: bool,
    /// Click limit, if one is set.
    #[serde(default)]
    pub max_clicks: Option<u64>,
    /// When the link expires, if an expiry is set.
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub expire_after: Option<DateTime<Utc>>,
    /// Whether known bots are blocked.
    #[serde(default)]
    pub block_bots: Option<bool>,
    /// Whether statistics are owner-only.
    #[serde(default)]
    pub private_stats: Option<bool>,
    /// Custom domain the link lives on, `None` for the default namespace.
    #[serde(default)]
    pub domain: Option<String>,
    /// Per-country destination overrides.
    #[serde(default)]
    pub geo_rules: Option<HashMap<String, String>>,
    /// When the update was applied.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
    /// Custom social preview, if configured.
    #[serde(default)]
    pub meta_tags: Option<MetaTagsInfo>,
}

/// Confirmation of a deletion.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct DeletedLink {
    /// Confirmation message.
    pub message: String,
    /// The deleted link's identifier.
    pub id: String,
}

/// Why an alias is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum AliasIssue {
    /// Too short or too long.
    #[serde(rename = "length")]
    Length,
    /// Contains characters outside the accepted set.
    #[serde(rename = "format")]
    Format,
    /// Reserved by the platform.
    #[serde(rename = "reserved")]
    Reserved,
    /// Already in use.
    #[serde(rename = "taken")]
    Taken,
    /// Emoji alias contains sequences outside the accepted set.
    #[serde(rename = "emoji_policy")]
    EmojiPolicy,
    /// A reason this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// Result of an alias availability check.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct AliasCheck {
    /// Whether the alias is free to use: it passes validation AND is not
    /// taken.
    pub available: bool,
    /// When unavailable, why.
    #[serde(default)]
    pub reason: Option<AliasIssue>,
}

/// Counts derived from a bulk operation's result rows.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct BulkSummary {
    /// Unique ids in the request, after deduplication.
    pub total: u64,
    /// Rows that succeeded.
    pub succeeded: u64,
    /// Rows that failed.
    pub failed: u64,
}

/// Machine-readable cause of a per-item bulk failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BulkErrorCode {
    /// No such link in your account (someone else's id answers the same,
    /// deliberately).
    NotFound,
    /// The link is blocked.
    Forbidden,
    /// The operation conflicts with the link's current state.
    Conflict,
    /// The value failed validation for this link.
    ValidationError,
    /// Unexpected per-item failure, logged server-side.
    Internal,
    /// Processing aborted before this item.
    NotAttempted,
    /// A code this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// Per-item verdict of a bulk operation.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct BulkResult {
    /// The requested link id.
    pub id: String,
    /// Echoed when the id resolved to a link you own.
    #[serde(default)]
    pub alias: Option<String>,
    /// Whether the operation succeeded for this id.
    pub ok: bool,
    /// Failure cause to branch on; `None` when ok.
    #[serde(default)]
    pub error_code: Option<BulkErrorCode>,
    /// Display-safe failure message; not stable, branch on `error_code`.
    #[serde(default)]
    pub error: Option<String>,
}

/// Envelope of every bulk link operation. Partial success is data, not an
/// error: inspect `results` row by row.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct BulkOutcome {
    /// Aggregate counts.
    pub summary: BulkSummary,
    /// One row per unique requested id, in request order.
    pub results: Vec<BulkResult>,
}

/// Confirmation of a whole-domain bulk delete.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct DomainPurge {
    /// Confirmation message.
    pub message: String,
    /// Number of links deleted.
    pub count: u64,
    /// Domain whose links were deleted.
    pub domain: String,
}

/// One (link id, claim token) pair to claim.
#[derive(Debug, Clone, Serialize)]
pub struct ClaimRequest {
    url_id: String,
    token: String,
}

impl ClaimRequest {
    /// Pair a link id with the one-time claim token its anonymous create
    /// returned.
    pub fn new(url_id: impl Into<String>, token: impl Into<String>) -> Self {
        ClaimRequest {
            url_id: url_id.into(),
            token: token.into(),
        }
    }
}

/// Per-item outcome of a claim batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ClaimStatus {
    /// Ownership transferred; the token is burned.
    Claimed,
    /// You already own this link (idempotent repeat).
    AlreadyYours,
    /// Unknown id, wrong token, or a link that is not claimable
    /// (deliberately indistinguishable).
    Invalid,
    /// A status this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// One row of a claim batch's outcome.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ClaimResult {
    /// The link id from the request item.
    pub url_id: String,
    /// What happened.
    pub status: ClaimStatus,
}

/// Outcome of a claim batch. The batch never hard-fails: every submitted
/// item gets a result, in request order.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ClaimOutcome {
    /// One outcome per submitted item.
    pub results: Vec<ClaimResult>,
    /// Convenience count of `Claimed` results.
    pub claimed: u64,
}

impl Links {
    /// Shorten a URL. Returns a builder: chain options, then `.send()`.
    ///
    /// ```no_run
    /// # async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
    /// let link = client
    ///     .links()
    ///     .create("https://example.com/launch")
    ///     .alias("launch")
    ///     .max_clicks(10_000)
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(&self, long_url: impl Into<String>) -> CreateLinkBuilder {
        CreateLinkBuilder {
            client: self.client.clone(),
            body: CreateLinkBody {
                long_url: long_url.into(),
                ..Default::default()
            },
        }
    }

    /// Check whether an alias is available before trying to create it.
    pub fn check_alias(&self, alias: impl Into<String>) -> CheckAliasBuilder {
        CheckAliasBuilder {
            client: self.client.clone(),
            alias: alias.into(),
            domain: None,
        }
    }

    /// List your links, paginated. Returns a builder: chain filters, then
    /// `.send()` for a [`Page`].
    pub fn list(&self) -> ListLinksBuilder {
        ListLinksBuilder {
            client: self.client.clone(),
            page: None,
            page_size: None,
            sort_by: None,
            sort_order: None,
            domain: None,
            filter: serde_json::Map::new(),
        }
    }

    /// Fetch one link by its id.
    pub async fn get(&self, id: &str) -> Result<LinkItem, Error> {
        self.client
            .transport
            .execute(RequestSpec::new(Method::GET, format!("/api/v1/urls/{id}")))
            .await
    }

    /// Fetch one link by where it lives: domain plus alias. Pass the system
    /// domain (`spoo.me`) for default-namespace links.
    pub async fn get_by_address(&self, domain: &str, alias: &str) -> Result<LinkItem, Error> {
        self.client
            .transport
            .execute(RequestSpec::new(
                Method::GET,
                format!("/api/v1/urls/{domain}/{alias}"),
            ))
            .await
    }

    /// Update a link. Returns a builder: chain changes, then `.send()`.
    /// Fields you do not touch keep their stored values.
    pub fn update(&self, id: impl Into<String>) -> UpdateLinkBuilder {
        UpdateLinkBuilder {
            client: self.client.clone(),
            id: id.into(),
            body: UpdateLinkBody::default(),
        }
    }

    /// Enable or disable a link's redirects.
    pub async fn set_status(&self, id: &str, status: SettableStatus) -> Result<UpdatedLink, Error> {
        let spec = RequestSpec::new(Method::PATCH, format!("/api/v1/urls/{id}/status"))
            .json(&serde_json::json!({ "status": status }))?;
        self.client.transport.execute(spec).await
    }

    /// Delete a link permanently.
    pub async fn delete(&self, id: &str) -> Result<DeletedLink, Error> {
        self.client
            .transport
            .execute(RequestSpec::new(
                Method::DELETE,
                format!("/api/v1/urls/{id}"),
            ))
            .await
    }

    /// Delete every link on one of your custom domains. Irreversible and
    /// whole-domain: there is deliberately no filter.
    pub async fn delete_all_on_domain(&self, domain: &str) -> Result<DomainPurge, Error> {
        let spec =
            RequestSpec::new(Method::DELETE, "/api/v1/urls").query("domain", Some(domain.into()));
        self.client.transport.execute(spec).await
    }

    /// Delete up to 100 links by id. Partial success is data: check the
    /// outcome's rows.
    pub async fn bulk_delete<I, S>(&self, ids: I) -> Result<BulkOutcome, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.bulk("/api/v1/urls/bulk/delete", ids, serde_json::Map::new())
            .await
    }

    /// Set the status of up to 100 links at once.
    pub async fn bulk_set_status<I, S>(
        &self,
        ids: I,
        status: SettableStatus,
    ) -> Result<BulkOutcome, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "status".into(),
            serde_json::to_value(status).map_err(Error::Decode)?,
        );
        self.bulk("/api/v1/urls/bulk/status", ids, extra).await
    }

    /// Set or clear the expiry of up to 100 links at once. `None` clears.
    pub async fn bulk_set_expiry<I, S>(
        &self,
        ids: I,
        expire_after: Option<DateTime<Utc>>,
    ) -> Result<BulkOutcome, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "expire_after".into(),
            match expire_after {
                Some(when) => serde_json::Value::String(when.to_rfc3339()),
                None => serde_json::Value::Null,
            },
        );
        self.bulk("/api/v1/urls/bulk/expiry", ids, extra).await
    }

    /// Move up to 100 links to a custom domain you own, or back to the
    /// default namespace with `None`.
    pub async fn bulk_move_domain<I, S>(
        &self,
        ids: I,
        domain: Option<&str>,
    ) -> Result<BulkOutcome, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "domain".into(),
            match domain {
                Some(fqdn) => serde_json::Value::String(fqdn.to_owned()),
                None => serde_json::Value::Null,
            },
        );
        self.bulk("/api/v1/urls/bulk/domain", ids, extra).await
    }

    async fn bulk<I, S>(
        &self,
        path: &str,
        ids: I,
        mut extra: serde_json::Map<String, serde_json::Value>,
    ) -> Result<BulkOutcome, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let ids: Vec<String> = ids.into_iter().map(Into::into).collect();
        extra.insert(
            "ids".into(),
            serde_json::Value::Array(ids.into_iter().map(serde_json::Value::String).collect()),
        );
        let spec = RequestSpec::new(Method::POST, path).json(&serde_json::Value::Object(extra))?;
        self.client.transport.execute(spec).await
    }

    /// Attach anonymously created links to this account, up to 16 per call.
    /// Each item resolves independently; the batch never hard-fails.
    pub async fn claim(
        &self,
        claims: impl IntoIterator<Item = ClaimRequest>,
    ) -> Result<ClaimOutcome, Error> {
        let claims: Vec<ClaimRequest> = claims.into_iter().collect();
        let spec = RequestSpec::new(Method::POST, "/api/v1/urls/claim")
            .json(&serde_json::json!({ "claims": claims }))?;
        self.client.transport.execute(spec).await
    }
}

#[derive(Default, Serialize, Clone)]
struct CreateLinkBody {
    long_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias_type: Option<AliasKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_bots: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_clicks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expire_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_stats: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    geo_rules: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta_tags: Option<MetaTags>,
}

/// Builder for [`Links::create`]. Chain options, finish with
/// [`CreateLinkBuilder::send`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct CreateLinkBuilder {
    client: Client,
    body: CreateLinkBody,
}

impl CreateLinkBuilder {
    /// Custom short code: alphanumeric (3-16 chars) or emoji-only (1-15
    /// fully-qualified emoji).
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.body.alias = Some(alias.into());
        self
    }

    /// Alias style to auto-generate when no explicit alias is given.
    pub fn alias_kind(mut self, kind: AliasKind) -> Self {
        self.body.alias_type = Some(kind);
        self
    }

    /// Password-protect the link. Min 8 chars with a letter, a number and a
    /// special character (the server validates).
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.body.password = Some(password.into());
        self
    }

    /// Block known bot user agents.
    pub fn block_bots(mut self, block: bool) -> Self {
        self.body.block_bots = Some(block);
        self
    }

    /// Expire the link after this many clicks.
    pub fn max_clicks(mut self, max: u64) -> Self {
        self.body.max_clicks = Some(max);
        self
    }

    /// Expire the link at a point in time.
    pub fn expire_after(mut self, when: DateTime<Utc>) -> Self {
        self.body.expire_after = Some(when.to_rfc3339());
        self
    }

    /// Make statistics owner-only. Requires authentication.
    pub fn private_stats(mut self, private: bool) -> Self {
        self.body.private_stats = Some(private);
        self
    }

    /// Scope the link under a custom domain you own (must be ACTIVE).
    pub fn domain(mut self, fqdn: impl Into<String>) -> Self {
        self.body.domain = Some(fqdn.into());
        self
    }

    /// Redirect visitors from one country somewhere else. ISO 3166-1
    /// alpha-2 code; repeatable.
    pub fn geo_rule(mut self, country: impl Into<String>, url: impl Into<String>) -> Self {
        self.body
            .geo_rules
            .get_or_insert_with(HashMap::new)
            .insert(country.into(), url.into());
        self
    }

    /// Replace the whole set of per-country destination overrides.
    pub fn geo_rules(mut self, rules: HashMap<String, String>) -> Self {
        self.body.geo_rules = Some(rules);
        self
    }

    /// Attach a custom social preview.
    pub fn meta_tags(mut self, tags: MetaTags) -> Self {
        self.body.meta_tags = Some(tags);
        self
    }

    /// Create the link.
    pub async fn send(self) -> Result<Link, Error> {
        let spec = RequestSpec::new(Method::POST, "/api/v1/shorten").json(&self.body)?;
        self.client.transport.execute(spec).await
    }
}

/// Builder for [`Links::check_alias`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct CheckAliasBuilder {
    client: Client,
    alias: String,
    domain: Option<String>,
}

impl CheckAliasBuilder {
    /// Check availability on a custom domain instead of the default
    /// namespace.
    pub fn domain(mut self, fqdn: impl Into<String>) -> Self {
        self.domain = Some(fqdn.into());
        self
    }

    /// Run the check.
    pub async fn send(self) -> Result<AliasCheck, Error> {
        let spec = RequestSpec::new(Method::GET, "/api/v1/shorten/check-alias")
            .query("alias", Some(self.alias))
            .query("domain", self.domain);
        self.client.transport.execute(spec).await
    }
}

/// Sort key for [`Links::list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SortBy {
    /// Creation time (the default).
    CreatedAt,
    /// Most recent click.
    LastClick,
    /// Lifetime click count.
    TotalClicks,
}

impl SortBy {
    fn as_str(self) -> &'static str {
        match self {
            SortBy::CreatedAt => "created_at",
            SortBy::LastClick => "last_click",
            SortBy::TotalClicks => "total_clicks",
        }
    }
}

/// Sort direction for [`Links::list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SortOrder {
    /// Smallest or oldest first.
    Ascending,
    /// Largest or newest first (the default).
    Descending,
}

impl SortOrder {
    fn as_str(self) -> &'static str {
        match self {
            SortOrder::Ascending => "asc",
            SortOrder::Descending => "desc",
        }
    }
}

/// Builder for [`Links::list`]. Chain filters, finish with
/// [`ListLinksBuilder::send`].
#[must_use = "builders do nothing until .send() is awaited"]
#[derive(Clone)]
pub struct ListLinksBuilder {
    client: Client,
    page: Option<u64>,
    page_size: Option<u64>,
    sort_by: Option<SortBy>,
    sort_order: Option<SortOrder>,
    domain: Option<String>,
    filter: serde_json::Map<String, serde_json::Value>,
}

/// Wire shape of the list response.
#[derive(Deserialize)]
struct ListWire {
    items: Vec<LinkItem>,
    page: u64,
    #[serde(rename = "pageSize")]
    page_size: u64,
    total: u64,
    #[serde(rename = "hasNext")]
    has_next: bool,
}

impl ListLinksBuilder {
    /// 1-based page number.
    pub fn page(mut self, page: u64) -> Self {
        self.page = Some(page);
        self
    }

    /// Items per page, 1 to 100.
    pub fn page_size(mut self, size: u64) -> Self {
        self.page_size = Some(size);
        self
    }

    /// Sort key.
    pub fn sort_by(mut self, key: SortBy) -> Self {
        self.sort_by = Some(key);
        self
    }

    /// Sort direction.
    pub fn sort_order(mut self, order: SortOrder) -> Self {
        self.sort_order = Some(order);
        self
    }

    /// Only links on this custom domain.
    pub fn domain(mut self, fqdn: impl Into<String>) -> Self {
        self.domain = Some(fqdn.into());
        self
    }

    /// Only links with this status.
    pub fn status(mut self, status: SettableStatus) -> Self {
        self.filter.insert(
            "status".into(),
            match status {
                SettableStatus::Active => "ACTIVE".into(),
                SettableStatus::Inactive => "INACTIVE".into(),
            },
        );
        self
    }

    /// Only links created after this time.
    pub fn created_after(mut self, when: DateTime<Utc>) -> Self {
        self.filter
            .insert("createdAfter".into(), when.to_rfc3339().into());
        self
    }

    /// Only links created before this time.
    pub fn created_before(mut self, when: DateTime<Utc>) -> Self {
        self.filter
            .insert("createdBefore".into(), when.to_rfc3339().into());
        self
    }

    /// Only links with (or without) a password.
    pub fn password_set(mut self, set: bool) -> Self {
        self.filter.insert("passwordSet".into(), set.into());
        self
    }

    /// Only links with (or without) a click limit.
    pub fn max_clicks_set(mut self, set: bool) -> Self {
        self.filter.insert("maxClicksSet".into(), set.into());
        self
    }

    /// Case-insensitive search in alias and destination URL.
    pub fn search(mut self, term: impl Into<String>) -> Self {
        self.filter.insert("search".into(), term.into().into());
        self
    }

    /// Fetch the page.
    pub async fn send(self) -> Result<Page<LinkItem>, Error> {
        let next_template = self.clone();
        let spec = self.into_spec()?;
        let wire: ListWire = next_template.client.transport.execute(spec).await?;
        Ok(build_page(wire, next_template))
    }

    fn into_spec(self) -> Result<RequestSpec, Error> {
        let filter = if self.filter.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&self.filter).map_err(Error::Decode)?)
        };
        Ok(RequestSpec::new(Method::GET, "/api/v1/urls")
            .query("page", self.page.map(|p| p.to_string()))
            .query("pageSize", self.page_size.map(|s| s.to_string()))
            .query("sortBy", self.sort_by.map(|s| s.as_str().to_owned()))
            .query("sortOrder", self.sort_order.map(|s| s.as_str().to_owned()))
            .query("domain", self.domain)
            .query("filter", filter))
    }
}

fn build_page(wire: ListWire, template: ListLinksBuilder) -> Page<LinkItem> {
    let current = wire.page;
    let has_next = wire.has_next;
    let next = has_next.then(|| {
        let fetch: crate::page::PageFetcher<LinkItem> = Box::new(move || {
            let mut builder = template.clone();
            builder.page = Some(current + 1);
            Box::pin(async move { builder.send().await })
        });
        fetch
    });
    Page {
        items: wire.items,
        page: wire.page,
        page_size: wire.page_size,
        total: wire.total,
        has_next,
        next,
    }
}

#[derive(Default, Serialize, Clone)]
struct UpdateLinkBody {
    #[serde(skip_serializing_if = "Patch::is_keep")]
    long_url: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    alias: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    password: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    block_bots: Patch<bool>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    max_clicks: Patch<u64>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    expire_after: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    private_stats: Patch<bool>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    status: Patch<SettableStatus>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    domain: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    geo_rules: Patch<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Patch::is_keep")]
    meta_tags: Patch<MetaTags>,
}

/// Builder for [`Links::update`]. Untouched fields keep their stored
/// values; the `remove_*` methods clear a setting explicitly.
#[must_use = "builders do nothing until .send() is awaited"]
pub struct UpdateLinkBuilder {
    client: Client,
    id: String,
    body: UpdateLinkBody,
}

impl UpdateLinkBuilder {
    /// Point the link at a new destination.
    pub fn long_url(mut self, url: impl Into<String>) -> Self {
        self.body.long_url = Patch::Set(url.into());
        self
    }

    /// Rename the short code. Must be available.
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.body.alias = Patch::Set(alias.into());
        self
    }

    /// Set a new password.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.body.password = Patch::Set(password.into());
        self
    }

    /// Remove password protection.
    pub fn remove_password(mut self) -> Self {
        self.body.password = Patch::Null;
        self
    }

    /// Enable or disable bot blocking.
    pub fn block_bots(mut self, block: bool) -> Self {
        self.body.block_bots = Patch::Set(block);
        self
    }

    /// Set a new click limit.
    pub fn max_clicks(mut self, max: u64) -> Self {
        self.body.max_clicks = Patch::Set(max);
        self
    }

    /// Remove the click limit.
    pub fn remove_max_clicks(mut self) -> Self {
        self.body.max_clicks = Patch::Null;
        self
    }

    /// Set a new expiry time.
    pub fn expire_after(mut self, when: DateTime<Utc>) -> Self {
        self.body.expire_after = Patch::Set(when.to_rfc3339());
        self
    }

    /// Remove the expiry.
    pub fn remove_expiry(mut self) -> Self {
        self.body.expire_after = Patch::Null;
        self
    }

    /// Make statistics owner-only, or public again.
    pub fn private_stats(mut self, private: bool) -> Self {
        self.body.private_stats = Patch::Set(private);
        self
    }

    /// Enable or disable redirects.
    pub fn status(mut self, status: SettableStatus) -> Self {
        self.body.status = Patch::Set(status);
        self
    }

    /// Move the link to a custom domain you own.
    pub fn domain(mut self, fqdn: impl Into<String>) -> Self {
        self.body.domain = Patch::Set(fqdn.into());
        self
    }

    /// Move the link back to the default namespace.
    pub fn system_domain(mut self) -> Self {
        self.body.domain = Patch::Null;
        self
    }

    /// Replace all per-country destination overrides.
    pub fn geo_rules(mut self, rules: HashMap<String, String>) -> Self {
        self.body.geo_rules = Patch::Set(rules);
        self
    }

    /// Remove all per-country destination overrides.
    pub fn clear_geo_rules(mut self) -> Self {
        self.body.geo_rules = Patch::Null;
        self
    }

    /// Replace the custom social preview.
    pub fn meta_tags(mut self, tags: MetaTags) -> Self {
        self.body.meta_tags = Patch::Set(tags);
        self
    }

    /// Remove the custom social preview.
    pub fn remove_meta_tags(mut self) -> Self {
        self.body.meta_tags = Patch::Null;
        self
    }

    /// Apply the update.
    pub async fn send(self) -> Result<UpdatedLink, Error> {
        let spec = RequestSpec::new(Method::PATCH, format!("/api/v1/urls/{}", self.id))
            .json(&self.body)?;
        self.client.transport.execute(spec).await
    }
}
