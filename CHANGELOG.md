# Changelog

## 0.2.0 (unreleased)

Rewrite against the spoo.me v1 API. Every endpoint the 0.1.x crate called
was removed in the platform's v1 overhaul, so this is a clean break; see
MIGRATION.md. The design language of the original crate by
[rdni](https://github.com/rdni) carries over: consuming builders, layered
errors, documentation on every public item.

- v1 coverage: shorten (alphanumeric and emoji aliases), alias check, link
  management (list/get/get-by-address/update/set-status/delete), bulk
  delete/status/expiry/domain, claiming anonymous links, account and
  per-link statistics, streaming exports, public stats and previews, the
  emoji catalogue (ETag-cached), identity read.
- Link tags: the `tags()` resource (list/create/update/delete), `tag_ids`
  on link create and update, tag filters on listings, stats and exports,
  and `bulk_update_tags()`.
- Authentication: API keys, anonymous mode, and Sign in with Spoo behind
  the `oauth` feature (PKCE, device-code exchange, self-refreshing
  single-flight sessions).
- Tri-state PATCH updates: untouched fields keep their stored values,
  `remove_*` methods clear a setting explicitly.
- Typed errors with the backend's machine-readable codes, rate-limit
  metadata, and predicates (`is_not_found`, `is_rate_limited`,
  `is_blocked`, `is_password_required`).
- Automatic retries with jittered backoff capped at 8 seconds, honoring
  both legal `Retry-After` forms; POST and PATCH replay only where the
  server provably did no work.
- Server-suggested export filenames are sanitized to safe bare names.
- Raw typed passthroughs (`client.get/post/patch/delete`) for endpoints the
  SDK does not cover yet.
- `#![forbid(unsafe_code)]`, panic-free library paths enforced by lints,
  wasm32 support, `stream`/`oauth`/`tracing`/`native-tls` features.
- License changed to MIT (0.1.x releases remain Apache-2.0).

## 0.1.1 and earlier

The original crate by rdni, targeting the pre-2026 spoo.me API
(form-encoded shorten/stats/export endpoints). Apache-2.0.
