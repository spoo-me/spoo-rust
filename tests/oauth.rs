//! Sign in with Spoo: PKCE, exchange, and the refreshing session's
//! guarantees (rotation, single-flight, expiry).
#![cfg(feature = "oauth")]

mod common;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use spoo_me::oauth::{Session, TokenPair};
use spoo_me::{Client, Error};

fn jwt_with_exp(exp: i64) -> String {
    let payload =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
    format!("header.{payload}.sig")
}

fn far_future() -> i64 {
    chrono::Utc::now().timestamp() + 3600
}

fn session_client(server: &wiremock::MockServer, session: Arc<Session>) -> Client {
    Client::builder()
        .base_url(server.uri())
        .session(session)
        .build()
        .expect("test client config is valid")
}

#[tokio::test]
async fn authorization_url_carries_the_pkce_contract() {
    let client = Client::anonymous();
    let url = client
        .oauth()
        .authorization_url("app_123", "state_abc", "challenge_xyz")
        .redirect_uri("http://127.0.0.1:8000/callback")
        .build()
        .expect("url builds");
    assert!(url.starts_with("https://spoo.me/auth/device/login?"));
    assert!(url.contains("app_id=app_123"));
    assert!(url.contains("state=state_abc"));
    assert!(url.contains("code_challenge=challenge_xyz"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8000%2Fcallback"));
}

#[tokio::test]
async fn exchange_code_sends_verifier() {
    let (server, _) = common::server_and_client().await;
    let client = common::anonymous_client(&server).await;
    Mock::given(method("POST"))
        .and(path("/auth/device/token"))
        .and(body_json(json!({
            "code": "one-time-code",
            "code_verifier": "the-verifier"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": jwt_with_exp(far_future()),
            "refresh_token": "refresh_1",
            "user": {
                "id": "u1",
                "email_verified": true,
                "plan": "free",
                "password_set": false,
                "auth_providers": []
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let tokens = client
        .oauth()
        .exchange_code("one-time-code", "the-verifier")
        .await
        .expect("exchange succeeds");
    assert_eq!(tokens.refresh_token, "refresh_1");
    assert_eq!(tokens.user.id, "u1");
}

#[tokio::test]
async fn session_refreshes_once_on_401_and_replays() {
    let (server, _) = common::server_and_client().await;
    let access_1 = jwt_with_exp(far_future());
    let access_2 = jwt_with_exp(far_future() + 1);

    // First request with the stale token: 401.
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {access_1}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "token revoked", "code": "authentication_error"
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The rotation.
    Mock::given(method("POST"))
        .and(path("/auth/device/refresh"))
        .and(body_json(json!({"refresh_token": "refresh_1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": access_2,
            "refresh_token": "refresh_2"
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The replay with the rotated token.
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {access_2}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "x", "password_set": false})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let rotations: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = rotations.clone();
    let session = Arc::new(
        Session::new(TokenPair::new(access_1.clone(), "refresh_1")).on_refresh(move |pair| {
            seen.lock()
                .expect("hook lock")
                .push(pair.refresh_token.clone());
        }),
    );
    let client = session_client(&server, session);

    client
        .links()
        .get("x")
        .await
        .expect("401 rotate replay succeeds");
    assert_eq!(
        rotations.lock().expect("hook lock").as_slice(),
        ["refresh_2"],
        "rotation persisted exactly once"
    );
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh() {
    let (server, _) = common::server_and_client().await;
    // An expired access token forces every request through the proactive
    // refresh path at once.
    let expired = jwt_with_exp(chrono::Utc::now().timestamp() - 10);
    let fresh = jwt_with_exp(far_future());

    let refresh_count = Arc::new(AtomicU32::new(0));
    let counter = refresh_count.clone();
    Mock::given(method("POST"))
        .and(path("/auth/device/refresh"))
        .respond_with(move |_: &wiremock::Request| {
            counter.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({
                "access_token": jwt_with_exp(chrono::Utc::now().timestamp() + 3600),
                "refresh_token": "refresh_2"
            }))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/x"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "x", "password_set": false})),
        )
        .mount(&server)
        .await;
    let _ = fresh;

    let session = Arc::new(Session::new(TokenPair::new(expired, "refresh_1")));
    let client = session_client(&server, session);

    let mut handles = Vec::new();
    for _ in 0..8 {
        let client = client.clone();
        handles.push(tokio::spawn(async move { client.links().get("x").await }));
    }
    for handle in handles {
        handle.await.expect("task joins").expect("request succeeds");
    }
    assert_eq!(
        refresh_count.load(Ordering::SeqCst),
        1,
        "stampede must share a single rotation"
    );
}

#[tokio::test]
async fn dead_refresh_token_is_session_expired() {
    let (server, _) = common::server_and_client().await;
    let expired = jwt_with_exp(chrono::Utc::now().timestamp() - 10);
    Mock::given(method("POST"))
        .and(path("/auth/device/refresh"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "refresh token revoked", "code": "authentication_error"
        })))
        .mount(&server)
        .await;

    let session = Arc::new(Session::new(TokenPair::new(expired, "refresh_dead")));
    let client = session_client(&server, session);

    let err = client.links().get("x").await.expect_err("refresh fails");
    assert!(matches!(err, Error::SessionExpired));
}

#[tokio::test]
async fn invalidate_forces_a_refresh_before_the_next_request() {
    let (server, _) = common::server_and_client().await;
    let valid = jwt_with_exp(far_future());
    Mock::given(method("POST"))
        .and(path("/auth/device/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": jwt_with_exp(far_future()),
            "refresh_token": "refresh_2"
        })))
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

    let session = Arc::new(Session::new(TokenPair::new(valid, "refresh_1")));
    let client = session_client(&server, session.clone());

    session.invalidate().await;
    client.links().get("x").await.expect("request succeeds");
}
