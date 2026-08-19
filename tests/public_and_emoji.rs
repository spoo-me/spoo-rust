//! Public surfaces and the emoji catalogue's ETag cache.

#![cfg(not(target_arch = "wasm32"))]
mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use spoo_me::{Generation, PublicStatus};

#[tokio::test]
async fn public_stats_get_without_password() {
    let (server, _) = common::server_and_client().await;
    let client = common::anonymous_client(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/public/stats/launch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "generation": "v2",
            "link": {
                "alias": "launch",
                "short_url": "https://spoo.me/launch",
                "status": "active",
                "block_bots": false,
                "password_protected": false
            },
            "stats": {"summary": {"total_clicks": 5}}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let stats = client
        .public()
        .stats("launch")
        .send()
        .await
        .expect("public stats load");
    assert_eq!(stats.generation, Generation::V2);
    assert_eq!(stats.link.status, PublicStatus::Active);
    assert_eq!(stats.stats["summary"]["total_clicks"], 5);
}

#[tokio::test]
async fn public_stats_password_switches_to_post() {
    let (server, _) = common::server_and_client().await;
    let client = common::anonymous_client(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/public/stats/locked"))
        .and(body_json(json!({"password": "hunter@22"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "generation": "v1",
            "link": {
                "alias": "locked",
                "short_url": "https://spoo.me/locked",
                "status": "active",
                "block_bots": true,
                "password_protected": true
            },
            "stats": {}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let stats = client
        .public()
        .stats("locked")
        .password("hunter@22")
        .send()
        .await
        .expect("password stats load");
    assert!(stats.link.password_protected);
}

#[tokio::test]
async fn preview_reveals_nothing_for_protected_links() {
    let (server, _) = common::server_and_client().await;
    let client = common::anonymous_client(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/public/preview/locked"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "generation": "v2",
            "alias": "locked",
            "short_url": "https://spoo.me/locked",
            "status": "active",
            "created_at": "2026-01-01T00:00:00Z",
            "password_protected": true,
            "destination": null,
            "geo_destinations": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    let preview = client
        .public()
        .preview("locked")
        .await
        .expect("preview loads");
    assert!(preview.destination.is_none());
}

#[tokio::test]
async fn emoji_set_revalidates_with_etag_and_serves_304_from_cache() {
    let (server, client) = common::server_and_client().await;
    let body = json!({
        "accept_max_version": 15.1,
        "generate_max_version": 14.0,
        "max_graphemes": 15,
        "emoji": [
            {"c": "🚀", "n": "rocket", "g": "Travel & Places", "gen": true},
            {"c": "🎉", "n": "party popper", "g": "Activities", "gen": true, "k": ["tada"]}
        ]
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/emoji-set"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v42\"")
                .set_body_json(body),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/emoji-set"))
        .and(header("if-none-match", "\"v42\""))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;

    let first = client.emoji().set().await.expect("first fetch");
    assert_eq!(first.emoji.len(), 2);
    assert_eq!(
        first.emoji[1].keywords.as_deref(),
        Some(&["tada".to_owned()][..])
    );

    let second = client.emoji().set().await.expect("cached fetch");
    assert_eq!(second.emoji.len(), 2);
    assert_eq!(second.max_graphemes, 15);
}

#[tokio::test]
async fn auth_me_unwraps_the_user_envelope() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/auth/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user": {
                "id": "665f0c2f9e7a4b1d2c3d4e5f",
                "email": "z@spoo.me",
                "email_verified": true,
                "plan": "free",
                "password_set": true,
                "auth_providers": [{"provider": "github"}]
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let user = client.auth().me().await.expect("me loads");
    assert_eq!(user.email.as_deref(), Some("z@spoo.me"));
    assert_eq!(user.auth_providers.len(), 1);
}
