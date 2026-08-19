# Migrating from 0.1.x to 0.2.0

0.2.0 is a rewrite. The 0.1.x crate targeted the original spoo.me endpoints,
which were removed in the platform's v1 overhaul, so every 0.1.x method has
stopped working against production regardless of SDK version. 0.2.0 targets
the v1 API, adds authentication, and keeps the original crate's design
language: consuming builders, layered errors, documented everything.

## The shape of the change

| 0.1.x | 0.2.0 |
|---|---|
| `UrlShortenerClient::new()` | `Client::new(api_key)` / `Client::anonymous()` / `Client::builder()` |
| `ShortenRequest::new(url).alias(a)` + `client.shorten(req)` | `client.links().create(url).alias(a).send()` |
| `EmojiRequest::new(url)` + `client.emoji(req)` | `client.links().create(url).alias("🚀🔥").send()` or `.alias_kind(AliasKind::Emoji)` |
| `StatsRequest::new(code)` + `client.stats(req)` | `client.public().stats(code).send()` (public) or `client.stats().for_link(id).send()` (authenticated) |
| `ExportRequest::new(code, fmt)` + `client.export(req)` | `client.stats().export_link(id).format(fmt).send()` |
| `UrlShortenerError::{Validation, Api, Http, Json}` | `Error::{Api, Transport, Decode, SessionExpired, Config}` |
| `blocking` feature replacing async | not yet available in 0.2.0 (planned as an additive feature) |
| `custom_url` feature | always available: `Client::builder().base_url(...)` |

## Behavior changes worth knowing

- Client-side validation of aliases, passwords and URLs is gone. The server
  is the validator; its typed errors carry a machine-readable `code` and the
  offending `field`.
- Requests authenticate with an API key or a Sign in with Spoo session.
  Anonymous creation still works and returns a one-time `claim_token`.
- Retries are built in: transient failures retry twice with jittered
  backoff, honoring `Retry-After`.
- Timestamps are `chrono::DateTime<Utc>` everywhere.
- Exports stream and their filenames are sanitized to safe bare names.
- Links are addressed by their `id` (the value management endpoints use),
  not by short code. `links().get_by_address(domain, alias)` resolves a
  short code to the full record.
