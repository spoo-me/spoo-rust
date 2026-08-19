# spoo.me Rust SDK

The official Rust SDK for the [spoo.me](https://spoo.me) link management API.

```rust,no_run
# async fn run() -> Result<(), spoo_me::Error> {
let client = spoo_me::Client::new("spoo_your_api_key");

let link = client
    .links()
    .create("https://example.com/launch")
    .alias("launch") // or emoji: "🚀🔥"
    .max_clicks(10_000)
    .send()
    .await?;
println!("{}", link.short_url); // https://spoo.me/launch
# Ok(())
# }
```

- Async-first on `reqwest` with rustls; compiles for wasm32 (checked in CI)
- Typed errors, automatic retries, streaming pagination and exports
- Timestamps in and out as `chrono::DateTime<Utc>`, whatever the wire format
- Anonymous, API key, and Sign in with Spoo authentication
- `#![forbid(unsafe_code)]`, panic-free library paths, thin dependency tree

## Install

```sh
cargo add spoo-me
```

Requires Rust 1.85 or newer. The command installs the SDK only; snippets on
this page also use `chrono` (timestamps in the public types), and some use
`reqwest` (client injection), `futures-util` (stream combinators) or `serde`
with the `derive` feature (escape-hatch response types). Add the ones your
code names.

## Authentication

Create an API key from your [spoo.me dashboard](https://spoo.me) and pass it
explicitly:

```rust,no_run
# fn run() -> Result<(), spoo_me::Error> {
let client = spoo_me::Client::from_env()?;
# Ok(())
# }
```

`Client::from_env()` reads `SPOO_API_KEY` for you; that constructor is the
only place the crate touches the environment. `Client::anonymous()` works
without an account: anonymous shortening and the public endpoints (stats,
previews, the emoji set) need no credentials.

Self-hosting spoo.me, sharing a connection pool, or tagging your app:

```rust,no_run
# fn run() -> Result<(), spoo_me::Error> {
let client = spoo_me::Client::builder()
    .api_key("spoo_...")
    .base_url("https://links.example.com")
    .http_client(reqwest::Client::new())
    .client_tag("my-app/1.0")
    .build()?;
# Ok(())
# }
```

The client is `Send + Sync + Clone` and cheap to clone: share one across
tasks.

## Shorten links

```rust,no_run
# async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
use chrono::{Duration, Utc};

let link = client
    .links()
    .create("https://example.com/launch")
    .alias("launch")
    .password("secure@123")
    .max_clicks(10_000)
    .expire_after(Utc::now() + Duration::days(30))
    .send()
    .await?;
# Ok(())
# }
```

Anonymous creations return a one-time `claim_token`. Store it and the link
can be claimed into an account later:

```rust,no_run
# async fn run(client: spoo_me::Client, link: spoo_me::Link) -> Result<(), spoo_me::Error> {
use spoo_me::ClaimRequest;

let token = link.claim_token.unwrap_or_default();
let outcome = client
    .links()
    .claim([ClaimRequest::new(&link.id, token)])
    .await?;
# Ok(())
# }
```

## Manage links

```rust,no_run
# async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
use spoo_me::{SettableStatus, SortBy};

// Paginated listing with typed filters.
let page = client
    .links()
    .list()
    .page_size(50)
    .sort_by(SortBy::TotalClicks)
    .status(SettableStatus::Active)
    .search("promo")
    .send()
    .await?;

// Or walk everything lazily (`stream` feature, on by default).
use futures_util::TryStreamExt as _;
let mut all = client.links().list().send().await?.stream();
while let Some(link) = all.try_next().await? {
    println!("{}", link.id);
}

// Updates only touch what you set; remove_* clears a setting explicitly.
client
    .links()
    .update("665f0c2f9e7a4b1d2c3d4e5f")
    .long_url("https://example.com/v2")
    .remove_password()
    .send()
    .await?;

// Bulk operations report per-item outcomes instead of failing the batch.
let outcome = client
    .links()
    .bulk_set_status(["id1", "id2"], SettableStatus::Inactive)
    .await?;
for row in &outcome.results {
    if !row.ok {
        println!("{}: {:?}", row.id, row.error_code);
    }
}
# Ok(())
# }
```

## Statistics and exports

```rust,no_run
# async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
use spoo_me::{Dimension, ExportFormat, FilterDimension};

let report = client
    .stats()
    .account()
    .group_by(Dimension::Time)
    .group_by(Dimension::Country)
    .filter(FilterDimension::Browser, ["Chrome"])
    .send()
    .await?;
println!("{} clicks", report.summary.total_clicks);

let per_link = client.stats().for_link("665f0c2f9e7a4b1d2c3d4e5f").send().await?;

// Exports stream; filenames from the server are reduced to a bare name
// (no separators or dot-segments), so joining one into a directory cannot
// traverse out of it. Choosing a safe directory remains your job.
let export = client
    .stats()
    .export_link("665f0c2f9e7a4b1d2c3d4e5f")
    .format(ExportFormat::Csv)
    .send()
    .await?;
println!("saving {}", export.filename);
let bytes = export.bytes().await?; // or .bytes_stream()
# Ok(())
# }
```

Account-wide downloads come from `stats().export()`; per-link downloads with
per-link filenames come from `stats().export_link(id)`.

## Public endpoints

```rust,no_run
# async fn run() -> Result<(), spoo_me::Error> {
let client = spoo_me::Client::anonymous();

let stats = client.public().stats("launch").send().await?;
let locked = client.public().stats("locked").password("hunter@22").send().await?;

// Where does this short link lead? Never reveals what the redirect would
// refuse to serve.
let preview = client.public().preview("launch").await?;

// The emoji-alias catalogue, ETag-cached on the client.
let set = client.emoji().set().await?;
# Ok(())
# }
```

## Errors

Everything returns `Result<T, spoo_me::Error>`, and the library never
panics. API failures carry the parsed error envelope plus predicates so you
can branch without string matching:

```rust,no_run
# async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
match client.links().get("gone").await {
    Ok(link) => println!("{:?}", link.alias),
    Err(err) if err.is_not_found() => println!("no such link"),
    Err(err) if err.is_rate_limited() => println!("wait {:?}", err.retry_after()),
    Err(err) if err.is_blocked() => println!("taken down"),
    Err(err) => return Err(err),
}
# Ok(())
# }
```

Transient failures (408, 429, 5xx, connection errors) are retried twice with
jittered exponential backoff capped at 8 seconds, honoring `Retry-After` in
both its legal forms. Requests that could duplicate work on replay (POST,
PATCH) retry only where the server provably did nothing (429, 503).

## Sign in with Spoo

The `oauth` feature carries the client half of the connected-apps flow:
PKCE, the device-code exchange, and a self-refreshing session.

```rust,ignore
// Requires the `oauth` feature.
# async fn run() -> Result<(), spoo_me::Error> {
use std::sync::Arc;
use spoo_me::oauth::{generate_pkce_pair, generate_state, Session, TokenPair};

let anon = spoo_me::Client::anonymous();
let pkce = generate_pkce_pair();
let state = generate_state();
let url = anon
    .oauth()
    .authorization_url("your_app_id", &state, &pkce.challenge)
    .redirect_uri("http://127.0.0.1:8000/callback")
    .build()?;
// Open `url` in a browser; your callback receives `code` and `state`.
// Verify the echoed state matches the one you sent BEFORE exchanging the
// code, and reject the flow on a mismatch (CSRF protection).

let tokens = anon.oauth().exchange_code("the-code", pkce.verifier).await?;

let session = Arc::new(Session::new(tokens.tokens()).on_refresh(|pair| {
    // Persist the rotated pair: the previous refresh token is dead.
    let _ = pair;
}));
let client = spoo_me::Client::builder().session(session).build()?;
let me = client.auth().me().await?;
# Ok(())
# }
```

Sessions refresh proactively before the access token expires and once more
after an unexpected 401. Refreshes are single-flight across tasks, and a
dead refresh token surfaces as `Error::SessionExpired`.

## Scope

This SDK covers the v1 data plane: shortening (including emoji aliases),
link management, bulk operations, claiming, statistics, exports, public link
surfaces, the emoji catalogue, identity read, and Sign in with Spoo. Account
administration (API key management, profile editing), service endpoints
(health, contact), and the legacy v0 API are deliberately out of scope.

| Area | Methods |
|---|---|
| Shorten | `links().create()`, `links().check_alias()` |
| Manage | `links().list()`, `get()`, `get_by_address()`, `update()`, `set_status()`, `delete()`, `delete_all_on_domain()` |
| Bulk | `bulk_delete()`, `bulk_set_status()`, `bulk_set_expiry()`, `bulk_move_domain()` |
| Claim | `links().claim()` |
| Stats | `stats().account()`, `stats().for_link()` |
| Export | `stats().export()`, `stats().export_link()` |
| Public | `public().stats()`, `public().preview()` |
| Emoji | `emoji().set()` |
| Identity | `auth().me()` |
| Sign in with Spoo | `oauth().authorization_url()`, `exchange_code()`, `refresh_tokens()`, `Session` |

## Raw requests

For v1 endpoints the SDK does not cover yet, the client exposes typed
passthroughs that reuse its auth, retries, timeout and error mapping:

```rust,no_run
# async fn run(client: spoo_me::Client) -> Result<(), spoo_me::Error> {
#[derive(serde::Deserialize)]
struct Whatever { ok: bool }

let value: Whatever = client.get("/api/v1/new-endpoint", &[("k", "v")]).await?;
# Ok(())
# }
```

These are a supported pressure valve. If you need one, the surface has a gap
worth an issue on this repo.

## Features

| Feature | Default | What it adds |
|---|---|---|
| `stream` | yes | Lazy pagination and export bodies as `futures_core::Stream` |
| `oauth` | no | Sign in with Spoo: PKCE, exchange, refreshing sessions |
| `tracing` | no | Per-request spans and retry events |
| `native-tls` | no | Platform TLS instead of rustls |

Using `oauth` on wasm32? getrandom requires the final binary to pick its
backend: add `--cfg getrandom_backend="wasm_js"` to your RUSTFLAGS (usually
via `.cargo/config.toml`).

## Credits

The original `spoo-me` crate was created and designed by
[rdni](https://github.com/rdni). This SDK keeps that crate's design
language: consuming builders, layered errors, and documented-everything
discipline.

## License

MIT. Versions up to 0.1.1 were published under Apache-2.0.
