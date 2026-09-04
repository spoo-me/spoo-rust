//! Wire-level tests for the tags resource: exact request bytes, exact
//! response decoding.

#![cfg(not(target_arch = "wasm32"))]
mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use spoo_me::{TagColor, TagIcon};

fn tag_body() -> serde_json::Value {
    json!({
        "id": "665f0c2f9e7a4b1d2c3d4e5f",
        "name": "launch",
        "color": "violet",
        "icon": "rocket",
        "link_count": 14,
        "created_at": "2026-01-01T00:00:00+00:00",
        "updated_at": null
    })
}

#[tokio::test]
async fn list_unwraps_items_and_decodes() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tags"))
        .and(header("authorization", "Bearer spoo_test_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [tag_body()]})))
        .expect(1)
        .mount(&server)
        .await;

    let tags = client.tags().list().await.expect("list succeeds");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "launch");
    assert_eq!(tags[0].color, TagColor::Violet);
    assert_eq!(tags[0].icon, TagIcon::Rocket);
    assert_eq!(tags[0].link_count, 14);
    assert_eq!(tags[0].created_at.timestamp(), 1767225600);
    assert!(tags[0].updated_at.is_none());
}

#[tokio::test]
async fn create_sends_exact_body() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/tags"))
        .and(body_json(json!({
            "name": "launch",
            "color": "violet",
            "icon": "bar-chart-3"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(tag_body()))
        .expect(1)
        .mount(&server)
        .await;

    let tag = client
        .tags()
        .create("launch")
        .color(TagColor::Violet)
        .icon(TagIcon::BarChart3)
        .send()
        .await
        .expect("create succeeds");
    assert_eq!(tag.id, "665f0c2f9e7a4b1d2c3d4e5f");
}

#[tokio::test]
async fn create_passes_non_ascii_names_through_untouched() {
    let (server, client) = common::server_and_client().await;
    let mut body = tag_body();
    body["name"] = json!("lançamento");
    Mock::given(method("POST"))
        .and(path("/api/v1/tags"))
        .and(body_json(json!({"name": " Lançamento "})))
        .respond_with(ResponseTemplate::new(201).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let tag = client
        .tags()
        .create(" Lançamento ")
        .send()
        .await
        .expect("create succeeds");
    assert_eq!(tag.name, "lançamento");
}

#[tokio::test]
async fn create_omits_unset_fields() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/tags"))
        .and(body_json(json!({"name": "launch"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(tag_body()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .tags()
        .create("launch")
        .send()
        .await
        .expect("bare create succeeds");
}

#[tokio::test]
async fn update_sends_only_touched_fields() {
    let (server, client) = common::server_and_client().await;
    let mut body = tag_body();
    body["name"] = json!("q3");
    body["icon"] = json!("flag");
    body["updated_at"] = json!("2026-02-01T12:30:00+00:00");
    Mock::given(method("PATCH"))
        .and(path("/api/v1/tags/665f0c2f9e7a4b1d2c3d4e5f"))
        .and(body_json(json!({"name": "q3", "icon": "flag"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let tag = client
        .tags()
        .update("665f0c2f9e7a4b1d2c3d4e5f")
        .name("q3")
        .icon(TagIcon::Flag)
        .send()
        .await
        .expect("update succeeds");
    assert_eq!(tag.icon, TagIcon::Flag);
    assert_eq!(tag.updated_at.map(|t| t.timestamp()), Some(1769949000));
}

#[tokio::test]
async fn delete_reports_links_updated() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/tags/665f0c2f9e7a4b1d2c3d4e5f"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"deleted": true, "links_updated": 3})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let gone = client
        .tags()
        .delete("665f0c2f9e7a4b1d2c3d4e5f")
        .await
        .expect("delete succeeds");
    assert_eq!(gone.links_updated, 3);
}

#[tokio::test]
async fn unknown_palette_values_survive_as_sent() {
    let (server, client) = common::server_and_client().await;
    let mut body = tag_body();
    body["color"] = json!("chartreuse");
    body["icon"] = json!("unicorn");
    Mock::given(method("GET"))
        .and(path("/api/v1/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [body]})))
        .expect(1)
        .mount(&server)
        .await;

    let tags = client.tags().list().await.expect("decoding survives");
    assert_eq!(tags[0].color, TagColor::Other("chartreuse".into()));
    assert_eq!(tags[0].icon, TagIcon::Other("unicorn".into()));
}
