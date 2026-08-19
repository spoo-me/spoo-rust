//! Transport behavior: retries, error mapping, headers.

mod common;

use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use spoo_me::Error;

fn ok_link() -> serde_json::Value {
    json!({
        "id": "665f0c2f9e7a4b1d2c3d4e5f",
        "alias": "a",
        "short_url": "https://spoo.me/a",
        "long_url": "https://example.com/",
        "created_at": 1704067200,
        "status": "ACTIVE"
    })
}

#[tokio::test]
async fn client_tag_header_is_sent() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .and(header(
            "x-spoo-client",
            format!("sdk-rust/{}", env!("CARGO_PKG_VERSION")).as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "x", "password_set": false})),
        )
        .expect(1)
        .mount(&server)
        .await;
    client
        .links()
        .get("x")
        .await
        .expect("request carries the tag");
}

#[tokio::test]
async fn get_retries_transient_500_then_succeeds() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "x", "password_set": false})),
        )
        .expect(1)
        .mount(&server)
        .await;

    client.links().get("x").await.expect("retry recovers");
}

#[tokio::test]
async fn post_does_not_retry_500() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": "exploded", "code": "internal"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .links()
        .create("https://example.com/")
        .send()
        .await
        .expect_err("500 on POST must surface, not replay");
    assert_eq!(err.status(), Some(500));
}

#[tokio::test]
async fn post_retries_429_honoring_retry_after() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .set_body_json(json!({"error": "slow down", "code": "rate_limit_exceeded"})),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .respond_with(ResponseTemplate::new(201).set_body_json(ok_link()))
        .expect(1)
        .mount(&server)
        .await;

    let started = std::time::Instant::now();
    client
        .links()
        .create("https://example.com/")
        .send()
        .await
        .expect("429 retry recovers");
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "Retry-After of 1s was not honored: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn envelope_error_maps_fields_and_rate_limit() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .respond_with(
            ResponseTemplate::new(422)
                .insert_header("x-request-id", "req-123")
                .insert_header("x-ratelimit-limit", "50")
                .insert_header("x-ratelimit-remaining", "49")
                .insert_header("x-ratelimit-reset", "1767225599")
                .set_body_json(json!({
                    "error": "alias is taken",
                    "code": "conflict",
                    "field": "alias"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .links()
        .create("https://example.com/")
        .alias("taken")
        .send()
        .await
        .expect_err("422 surfaces");
    let api = err.api().expect("api error");
    assert_eq!(api.code, "conflict");
    assert_eq!(api.field.as_deref(), Some("alias"));
    assert_eq!(api.request_id.as_deref(), Some("req-123"));
    assert_eq!(api.rate_limit.limit, Some(50));
    assert_eq!(
        api.rate_limit.reset.map(|t| t.timestamp()),
        Some(1767225599)
    );
    assert!(api.body.is_none(), "envelope bodies are not preserved raw");
}

#[tokio::test]
async fn non_envelope_body_is_never_the_message() {
    let (server, client) = common::server_and_client().await;
    let html = "<html><body><h1>502 Bad Gateway</h1></body></html>";
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .respond_with(
            ResponseTemplate::new(502)
                .insert_header("x-error-code", "bad_gateway")
                .set_body_string(html),
        )
        .mount(&server)
        .await;

    let err = client.links().get("x").await.expect_err("502 surfaces");
    let api = err.api().expect("api error");
    assert_eq!(api.message, "HTTP 502");
    assert_eq!(
        api.code, "bad_gateway",
        "X-Error-Code header is the fallback"
    );
    assert_eq!(api.body.as_deref(), Some(html));
}

#[tokio::test]
async fn error_predicates_branch_without_string_matching() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/gone"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": "no such URL", "code": "not_found"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/blocked"))
        .respond_with(ResponseTemplate::new(451).set_body_json(json!({
            "error": "unavailable", "code": "blocked"
        })))
        .mount(&server)
        .await;

    let not_found = client.links().get("gone").await.expect_err("404");
    assert!(not_found.is_not_found());
    assert!(!not_found.is_blocked());

    let blocked = client.links().get("blocked").await.expect_err("451");
    assert!(blocked.is_blocked());
}

#[tokio::test]
async fn password_required_predicate_uses_error_code() {
    let (server, _) = common::server_and_client().await;
    let client = common::anonymous_client(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/public/stats/locked"))
        .respond_with(
            ResponseTemplate::new(401)
                .insert_header("x-error-code", "password_required")
                .set_body_json(json!({
                    "error": "password required", "code": "password_required"
                })),
        )
        .mount(&server)
        .await;

    let err = client
        .public()
        .stats("locked")
        .send()
        .await
        .expect_err("401 surfaces");
    assert!(err.is_password_required());
    assert!(!matches!(err, Error::SessionExpired));
}

#[tokio::test]
async fn escape_hatch_reuses_auth_and_error_mapping() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/some/new/endpoint"))
        .and(header("authorization", "Bearer spoo_test_key"))
        .and(wiremock::matchers::query_param("k", "v"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"answer": 42})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/some/new/endpoint"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": "nope", "code": "not_found"
        })))
        .expect(1)
        .mount(&server)
        .await;

    #[derive(Debug, serde::Deserialize)]
    struct Answer {
        answer: u32,
    }
    let got: Answer = client
        .get("/api/v1/some/new/endpoint", &[("k", "v")])
        .await
        .expect("raw get succeeds");
    assert_eq!(got.answer, 42);

    let err = client
        .post::<Answer>("/api/v1/some/new/endpoint", &json!({"x": 1}))
        .await
        .expect_err("raw post maps errors");
    assert!(err.is_not_found());
}
