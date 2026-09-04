#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod client;
mod error;
mod http;
mod page;
mod patch;

pub mod resources;

/// Sign in with Spoo: PKCE, device-code exchange, refreshing sessions.
#[cfg(feature = "oauth")]
pub mod oauth;

pub use client::{Client, ClientBuilder, DEFAULT_BASE_URL};
pub use error::{ApiError, Error, RateLimit};
pub use page::Page;
#[cfg(feature = "stream")]
pub use page::PageStream;
pub use patch::Patch;

pub use resources::auth::{AuthProvider, AuthResource, ProfilePicture, User};
pub use resources::emoji::{Emoji, EmojiEntry, EmojiSet};
pub use resources::links::{
    AliasCheck, AliasIssue, AliasKind, BulkErrorCode, BulkOutcome, BulkResult, BulkSummary,
    CheckAliasBuilder, ClaimOutcome, ClaimRequest, ClaimResult, ClaimStatus, CreateLinkBuilder,
    DeletedLink, DomainPurge, Link, LinkItem, LinkStatus, Links, ListLinksBuilder, MetaTags,
    MetaTagsInfo, SettableStatus, SortBy, SortOrder, TagsMatch, UpdateLinkBuilder, UpdatedLink,
};
pub use resources::public::{
    Generation, Preview, PreviewDestination, PreviewGeoDestination, Public, PublicLinkFacts,
    PublicStats, PublicStatsBuilder, PublicStatus,
};
pub use resources::stats::{
    AccountExportBuilder, AccountStatsBuilder, ComputedMetrics, Dimension, Export, ExportBuilder,
    ExportFormat, FilterDimension, LinkExportBuilder, LinkStatsBuilder, LinkStatsReport, Metric,
    Scope, Stats, StatsReport, Summary, TimeBucketInfo, TimeRange,
};
pub use resources::tags::{
    CreateTagBuilder, Tag, TagColor, TagDeleted, TagIcon, TagRef, Tags, UpdateTagBuilder,
};
