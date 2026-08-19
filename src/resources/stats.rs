//! Click statistics and exports.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::Deserialize;

use crate::client::Client;
use crate::error::Error;
use crate::http::{RequestSpec, content_disposition_filename};

/// Statistics and exports, from [`crate::Client::stats`].
pub struct Stats {
    pub(crate) client: Client,
}

/// A grouping dimension for statistics breakdowns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Dimension {
    /// Time buckets (day/week/month, auto-selected from the range).
    Time,
    /// Browser name.
    Browser,
    /// Operating system.
    Os,
    /// Device type: mobile, tablet, desktop, unknown.
    Device,
    /// Country.
    Country,
    /// City.
    City,
    /// Referrer URL.
    Referrer,
    /// URL alias.
    ShortCode,
    /// `utm_source` tag; untagged clicks appear as `(none)`.
    UtmSource,
    /// `utm_medium` tag.
    UtmMedium,
    /// `utm_campaign` tag.
    UtmCampaign,
}

impl Dimension {
    fn as_str(self) -> &'static str {
        match self {
            Dimension::Time => "time",
            Dimension::Browser => "browser",
            Dimension::Os => "os",
            Dimension::Device => "device",
            Dimension::Country => "country",
            Dimension::City => "city",
            Dimension::Referrer => "referrer",
            Dimension::ShortCode => "short_code",
            Dimension::UtmSource => "utm_source",
            Dimension::UtmMedium => "utm_medium",
            Dimension::UtmCampaign => "utm_campaign",
        }
    }
}

/// A metric to include in the breakdown. Defaults to both when unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Metric {
    /// Total click count.
    Clicks,
    /// Unique visitor count.
    UniqueClicks,
}

impl Metric {
    fn as_str(self) -> &'static str {
        match self {
            Metric::Clicks => "clicks",
            Metric::UniqueClicks => "unique_clicks",
        }
    }
}

/// A dimension clicks can be filtered by. Values are case-sensitive, exact
/// as stored. The aggregate-only `short_code`/`url_id` slicers live on
/// [`AccountStatsBuilder`] instead, so they cannot be sent where the
/// endpoint rejects them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilterDimension {
    /// Browser name (Chrome, Firefox, Safari, ...).
    Browser,
    /// Operating system (Windows, macOS, iOS, ...).
    Os,
    /// Device type: mobile, tablet, desktop, unknown.
    Device,
    /// Country name.
    Country,
    /// City name.
    City,
    /// Referrer URL.
    Referrer,
    /// `utm_source` tag; `(none)` matches untagged clicks.
    UtmSource,
    /// `utm_medium` tag.
    UtmMedium,
    /// `utm_campaign` tag.
    UtmCampaign,
}

impl FilterDimension {
    fn as_str(self) -> &'static str {
        match self {
            FilterDimension::Browser => "browser",
            FilterDimension::Os => "os",
            FilterDimension::Device => "device",
            FilterDimension::Country => "country",
            FilterDimension::City => "city",
            FilterDimension::Referrer => "referrer",
            FilterDimension::UtmSource => "utm_source",
            FilterDimension::UtmMedium => "utm_medium",
            FilterDimension::UtmCampaign => "utm_campaign",
        }
    }
}

/// Response scope marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum Scope {
    /// The owner's aggregate.
    #[serde(rename = "all")]
    All,
    /// The public stats page's frozen contract for anonymous links.
    #[serde(rename = "anon")]
    Anon,
    /// A scope this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// Summary block of a statistics response.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Summary {
    /// Total clicks in the range.
    pub total_clicks: u64,
    /// Unique visitors in the range.
    pub unique_clicks: u64,
    /// First click in the range.
    #[serde(default)]
    pub first_click: Option<DateTime<Utc>>,
    /// Most recent click in the range.
    #[serde(default)]
    pub last_click: Option<DateTime<Utc>>,
    /// Average redirection latency, milliseconds.
    #[serde(default)]
    pub avg_redirection_time: Option<f64>,
}

/// The time range a response covers.
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
pub struct TimeRange {
    /// Range start.
    #[serde(default)]
    pub start_date: Option<DateTime<Utc>>,
    /// Range end.
    #[serde(default)]
    pub end_date: Option<DateTime<Utc>>,
}

/// Time bucketing metadata, present when grouping by time.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct TimeBucketInfo {
    /// Bucketing strategy the server chose.
    pub strategy: String,
    /// The server's bucket format string.
    pub mongo_format: String,
    /// Suggested display format.
    pub display_format: String,
    /// Timezone the buckets are aligned to.
    pub timezone: String,
    /// Bucket width in minutes, for sub-daily strategies.
    #[serde(default)]
    pub interval_minutes: Option<u64>,
}

/// Derived rates the server computes over the range.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ComputedMetrics {
    /// unique_clicks / total_clicks.
    pub unique_click_rate: f64,
    /// 1 - unique_click_rate.
    pub repeat_click_rate: f64,
    /// total_clicks / unique visitors.
    pub average_clicks_per_visitor: f64,
}

/// A statistics breakdown (`GET /api/v1/stats`).
///
/// `metrics` is keyed `{metric}_by_{dimension}` (for example
/// `clicks_by_browser`); each value is a list of data points whose keys are
/// the dimension name, the metric name and `{metric}_percentage`. The keys
/// are dynamic by design, so data points stay [`serde_json::Value`] maps.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct StatsReport {
    /// Response scope marker.
    pub scope: Scope,
    /// The filters the server applied.
    #[serde(default)]
    pub filters: HashMap<String, Vec<String>>,
    /// The grouping dimensions applied.
    #[serde(default)]
    pub group_by: Vec<String>,
    /// Timezone of the response.
    pub timezone: String,
    /// The covered range.
    pub time_range: TimeRange,
    /// Totals over the range.
    pub summary: Summary,
    /// Breakdown series, keyed `{metric}_by_{dimension}`.
    #[serde(default)]
    pub metrics: HashMap<String, Vec<serde_json::Map<String, serde_json::Value>>>,
    /// When the server produced the response.
    #[serde(default)]
    pub generated_at: Option<DateTime<Utc>>,
    /// Alias echo, present when sliced to one link.
    #[serde(default)]
    pub short_code: Option<String>,
    /// Time bucketing metadata, when grouping by time.
    #[serde(default)]
    pub time_bucket_info: Option<TimeBucketInfo>,
    /// Derived rates, when the server includes them.
    #[serde(default)]
    pub computed_metrics: Option<ComputedMetrics>,
}

/// A single link's statistics (`GET /api/v1/stats/links/{url_id}`): the
/// standard wire plus the identity of the selected link.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct LinkStatsReport {
    /// The standard statistics body.
    #[serde(flatten)]
    pub stats: StatsReport,
    /// The selected link's id.
    pub url_id: String,
    /// The selected link's alias.
    pub alias: String,
}

/// Export file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportFormat {
    /// JSON document.
    Json,
    /// CSV files, zipped together.
    Csv,
    /// Excel workbook.
    Xlsx,
    /// XML document.
    Xml,
}

impl ExportFormat {
    fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Xml => "xml",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            // CSV exports arrive zipped.
            ExportFormat::Csv => "zip",
            other => other.as_str(),
        }
    }
}

/// A downloaded export. The body is consumed as a stream or buffered once;
/// `filename` is reduced to a safe bare filename before you see it, so it
/// can be joined into a directory path as-is.
pub struct Export {
    /// Server-suggested filename, sanitized to a bare name.
    pub filename: String,
    /// The response content type.
    pub content_type: String,
    response: reqwest::Response,
}

impl std::fmt::Debug for Export {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Export")
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .finish_non_exhaustive()
    }
}

impl Export {
    /// Buffer the whole body. Fine for typical exports; use
    /// [`Export::bytes_stream`] for very large accounts.
    pub async fn bytes(self) -> Result<Vec<u8>, Error> {
        Ok(self
            .response
            .bytes()
            .await
            .map_err(Error::Transport)?
            .to_vec())
    }

    /// Stream the body without buffering it.
    #[cfg(feature = "stream")]
    pub fn bytes_stream(
        self,
    ) -> impl futures_core::Stream<Item = Result<bytes::Bytes, reqwest::Error>> {
        self.response.bytes_stream()
    }
}

impl Stats {
    /// Account-wide statistics across all your links, optionally sliced to
    /// specific links.
    pub fn account(&self) -> AccountStatsBuilder {
        AccountStatsBuilder {
            client: self.client.clone(),
            core: QueryCore::default(),
            short_codes: Vec::new(),
            url_ids: Vec::new(),
        }
    }

    /// One link's statistics, addressed by id.
    pub fn for_link(&self, url_id: impl Into<String>) -> LinkStatsBuilder {
        LinkStatsBuilder {
            client: self.client.clone(),
            url_id: url_id.into(),
            core: QueryCore::default(),
        }
    }

    /// Download the account-wide export. Only this aggregate route carries
    /// the account-level slicers; per-link downloads with per-link filenames
    /// come from [`Stats::export_link`].
    pub fn export(&self) -> ExportBuilder {
        ExportBuilder {
            client: self.client.clone(),
            path: "/api/v1/export".to_owned(),
            format: None,
            core: QueryCore::default(),
        }
    }

    /// Download one link's export, named after the link by the server.
    pub fn export_link(&self, url_id: impl Into<String>) -> ExportBuilder {
        ExportBuilder {
            client: self.client.clone(),
            path: format!("/api/v1/export/links/{}", url_id.into()),
            format: None,
            core: QueryCore::default(),
        }
    }
}

/// Query state shared by the stats and export builders.
#[derive(Default, Clone)]
struct QueryCore {
    start_date: Option<String>,
    end_date: Option<String>,
    group_by: Vec<Dimension>,
    metrics: Vec<Metric>,
    timezone: Option<String>,
    // BTreeMap so the serialized filters JSON has a stable key order.
    filters: std::collections::BTreeMap<&'static str, Vec<String>>,
}

impl QueryCore {
    fn apply(self, mut spec: RequestSpec) -> Result<RequestSpec, Error> {
        spec = spec
            .query("start_date", self.start_date)
            .query("end_date", self.end_date)
            .query(
                "group_by",
                (!self.group_by.is_empty()).then(|| {
                    self.group_by
                        .iter()
                        .map(|d| d.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                }),
            )
            .query(
                "metrics",
                (!self.metrics.is_empty()).then(|| {
                    self.metrics
                        .iter()
                        .map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                }),
            )
            .query("timezone", self.timezone);
        if !self.filters.is_empty() {
            let filters = serde_json::to_string(&self.filters).map_err(Error::Decode)?;
            spec = spec.query("filters", Some(filters));
        }
        Ok(spec)
    }
}

macro_rules! stats_query_methods {
    () => {
        /// Start of the time range. Defaults to 7 days before the end.
        pub fn start_date(mut self, when: DateTime<Utc>) -> Self {
            self.core.start_date = Some(when.to_rfc3339());
            self
        }

        /// End of the time range. Defaults to now.
        pub fn end_date(mut self, when: DateTime<Utc>) -> Self {
            self.core.end_date = Some(when.to_rfc3339());
            self
        }

        /// Add a grouping dimension. Repeatable; defaults to time.
        pub fn group_by(mut self, dimension: Dimension) -> Self {
            self.core.group_by.push(dimension);
            self
        }

        /// Add a metric. Repeatable; defaults to both.
        pub fn metric(mut self, metric: Metric) -> Self {
            self.core.metrics.push(metric);
            self
        }

        /// IANA timezone for bucketing and display. Defaults to UTC.
        pub fn timezone(mut self, tz: impl Into<String>) -> Self {
            self.core.timezone = Some(tz.into());
            self
        }

        /// Only clicks matching one of these values on a dimension.
        /// Values are case-sensitive, exact as stored. Repeatable.
        pub fn filter<I, S>(mut self, dimension: FilterDimension, values: I) -> Self
        where
            I: IntoIterator<Item = S>,
            S: Into<String>,
        {
            self.core
                .filters
                .entry(dimension.as_str())
                .or_default()
                .extend(values.into_iter().map(Into::into));
            self
        }
    };
}

/// Builder for [`Stats::account`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct AccountStatsBuilder {
    client: Client,
    core: QueryCore,
    short_codes: Vec<String>,
    url_ids: Vec<String>,
}

impl AccountStatsBuilder {
    stats_query_methods!();

    /// Slice the aggregate to these aliases. Repeatable.
    pub fn short_codes<I, S>(mut self, codes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.short_codes.extend(codes.into_iter().map(Into::into));
        self
    }

    /// Slice the aggregate to these link ids. Ids you do not own match
    /// nothing. Repeatable.
    pub fn url_ids<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.url_ids.extend(ids.into_iter().map(Into::into));
        self
    }

    /// Fetch the breakdown.
    pub async fn send(mut self) -> Result<StatsReport, Error> {
        if !self.short_codes.is_empty() {
            self.core.filters.insert("short_code", self.short_codes);
        }
        if !self.url_ids.is_empty() {
            self.core.filters.insert("url_id", self.url_ids);
        }
        let spec = self
            .core
            .apply(RequestSpec::new(Method::GET, "/api/v1/stats"))?;
        self.client.transport.execute(spec).await
    }
}

/// Builder for [`Stats::for_link`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct LinkStatsBuilder {
    client: Client,
    url_id: String,
    core: QueryCore,
}

impl LinkStatsBuilder {
    stats_query_methods!();

    /// Fetch the breakdown.
    pub async fn send(self) -> Result<LinkStatsReport, Error> {
        let spec = self.core.apply(RequestSpec::new(
            Method::GET,
            format!("/api/v1/stats/links/{}", self.url_id),
        ))?;
        self.client.transport.execute(spec).await
    }
}

/// Builder for [`Stats::export`] and [`Stats::export_link`].
#[must_use = "builders do nothing until .send() is awaited"]
pub struct ExportBuilder {
    client: Client,
    path: String,
    format: Option<ExportFormat>,
    core: QueryCore,
}

impl ExportBuilder {
    stats_query_methods!();

    /// File format. Defaults to JSON server-side.
    pub fn format(mut self, format: ExportFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Download the export.
    pub async fn send(self) -> Result<Export, Error> {
        let fallback = format!(
            "spoo-export.{}",
            self.format.map(ExportFormat::extension).unwrap_or("json")
        );
        let spec = self.core.apply(
            RequestSpec::new(Method::GET, self.path)
                .query("format", self.format.map(|f| f.as_str().to_owned())),
        )?;
        let response = self.client.transport.send(spec).await?;
        let filename = content_disposition_filename(
            response
                .headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok()),
            &fallback,
        );
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        Ok(Export {
            filename,
            content_type,
            response,
        })
    }
}
