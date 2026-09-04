//! Wire-level tests for the links resource: exact request bytes, exact
//! response decoding.

#![cfg(not(target_arch = "wasm32"))]
mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use spoo_me::{
    AliasIssue, AliasKind, BulkErrorCode, ClaimRequest, ClaimStatus, LinkStatus, MetaTags,
    SettableStatus, SortBy, SortOrder, TagColor, TagIcon, TagsMatch,
};

fn link_body() -> serde_json::Value {
    json!({
        "id": "665f0c2f9e7a4b1d2c3d4e5f",
        "alias": "launch",
        "short_url": "https://spoo.me/launch",
        "long_url": "https://example.com/launch",
        "owner_id": null,
        "created_at": 1704067200,
        "status": "ACTIVE",
        "claim_token": "tok_once"
    })
}

#[tokio::test]
async fn create_sends_exact_body_and_decodes() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .and(header("authorization", "Bearer spoo_test_key"))
        .and(body_json(json!({
            "long_url": "https://example.com/launch",
            "alias": "launch",
            "alias_type": "alphanumeric",
            "password": "secure@123",
            "block_bots": true,
            "max_clicks": 100,
            "expire_after": "2027-01-01T00:00:00+00:00",
            "private_stats": true,
            "geo_rules": {"IN": "https://example.in/"},
            "meta_tags": {"title": "Launch", "color": "#FF5733"}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(link_body()))
        .expect(1)
        .mount(&server)
        .await;

    let when = chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
        .expect("valid test timestamp")
        .with_timezone(&chrono::Utc);
    let link = client
        .links()
        .create("https://example.com/launch")
        .alias("launch")
        .alias_kind(AliasKind::Alphanumeric)
        .password("secure@123")
        .block_bots(true)
        .max_clicks(100)
        .expire_after(when)
        .private_stats(true)
        .geo_rule("IN", "https://example.in/")
        .meta_tags(MetaTags::new("Launch").color("#FF5733"))
        .send()
        .await
        .expect("create succeeds");

    assert_eq!(link.id, "665f0c2f9e7a4b1d2c3d4e5f");
    assert_eq!(link.status, LinkStatus::Active);
    assert_eq!(link.created_at.timestamp(), 1704067200);
    assert_eq!(link.claim_token.as_deref(), Some("tok_once"));
}

#[tokio::test]
async fn create_omits_unset_fields() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .and(body_json(json!({"long_url": "https://example.com/"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(link_body()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .links()
        .create("https://example.com/")
        .send()
        .await
        .expect("bare create succeeds");
}

#[tokio::test]
async fn create_with_tags_sends_ids_and_decodes_refs() {
    let (server, client) = common::server_and_client().await;
    let mut body = link_body();
    body["tags"] = json!([
        {"id": "t1", "name": "launch", "color": "violet", "icon": "rocket"}
    ]);
    Mock::given(method("POST"))
        .and(path("/api/v1/shorten"))
        .and(body_json(json!({
            "long_url": "https://example.com/launch",
            "tag_ids": ["t1"]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let link = client
        .links()
        .create("https://example.com/launch")
        .tag_ids(["t1"])
        .send()
        .await
        .expect("create succeeds");
    assert_eq!(link.tags.len(), 1);
    assert_eq!(link.tags[0].name, "launch");
    assert_eq!(link.tags[0].color, TagColor::Violet);
    assert_eq!(link.tags[0].icon, TagIcon::Rocket);
}

#[tokio::test]
async fn links_without_tags_field_decode_to_empty() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/665f0c2f9e7a4b1d2c3d4e5f"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "665f0c2f9e7a4b1d2c3d4e5f",
            "password_set": false
        })))
        .expect(1)
        .mount(&server)
        .await;

    let link = client
        .links()
        .get("665f0c2f9e7a4b1d2c3d4e5f")
        .await
        .expect("get succeeds");
    assert!(link.tags.is_empty());
}

#[tokio::test]
async fn update_tag_ids_replaces_and_clear_sends_null() {
    let (server, client) = common::server_and_client().await;
    let updated = json!({
        "id": "665f0c2f9e7a4b1d2c3d4e5f",
        "password_set": false,
        "updated_at": 1704067300,
        "tags": [{"id": "t2", "name": "q3", "color": "teal", "icon": "flag"}]
    });
    Mock::given(method("PATCH"))
        .and(path("/api/v1/urls/665f0c2f9e7a4b1d2c3d4e5f"))
        .and(body_json(json!({"tag_ids": ["t2"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(updated))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/urls/665f0c2f9e7a4b1d2c3d4e5f"))
        .and(body_json(json!({"tag_ids": null})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "665f0c2f9e7a4b1d2c3d4e5f",
            "password_set": false,
            "updated_at": 1704067300,
            "tags": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let link = client
        .links()
        .update("665f0c2f9e7a4b1d2c3d4e5f")
        .tag_ids(["t2"])
        .send()
        .await
        .expect("replace succeeds");
    assert_eq!(link.tags[0].name, "q3");

    let link = client
        .links()
        .update("665f0c2f9e7a4b1d2c3d4e5f")
        .clear_tags()
        .send()
        .await
        .expect("clear succeeds");
    assert!(link.tags.is_empty());
}

#[tokio::test]
async fn update_patch_tristate_wire_bytes() {
    let (server, client) = common::server_and_client().await;
    // password cleared (explicit null), max_clicks set, everything else
    // absent from the body entirely.
    Mock::given(method("PATCH"))
        .and(path("/api/v1/urls/665f0c2f9e7a4b1d2c3d4e5f"))
        .and(body_json(json!({"password": null, "max_clicks": 500})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "665f0c2f9e7a4b1d2c3d4e5f",
            "password_set": false,
            "max_clicks": 500,
            "updated_at": 1704067300
        })))
        .expect(1)
        .mount(&server)
        .await;

    let updated = client
        .links()
        .update("665f0c2f9e7a4b1d2c3d4e5f")
        .remove_password()
        .max_clicks(500)
        .send()
        .await
        .expect("update succeeds");
    assert!(!updated.password_set);
    assert_eq!(updated.updated_at.timestamp(), 1704067300);
}

#[tokio::test]
async fn update_system_domain_sends_null() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/urls/665f0c2f9e7a4b1d2c3d4e5f"))
        .and(body_json(json!({"domain": null})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "665f0c2f9e7a4b1d2c3d4e5f",
            "password_set": false,
            "updated_at": 1704067300
        })))
        .expect(1)
        .mount(&server)
        .await;

    client
        .links()
        .update("665f0c2f9e7a4b1d2c3d4e5f")
        .system_domain()
        .send()
        .await
        .expect("domain move succeeds");
}

#[tokio::test]
async fn list_pagination_walks_and_streams() {
    let (server, client) = common::server_and_client().await;
    let item = |id: &str| {
        json!({
            "id": id,
            "alias": id,
            "created_at": "2026-01-01T00:00:00Z",
            "expire_after": 1767225599,
            "password_set": false
        })
    };
    Mock::given(method("GET"))
        .and(path("/api/v1/urls"))
        .and(query_param("page", "1"))
        .and(query_param("sortBy", "created_at"))
        .and(query_param("sortOrder", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [item("a"), item("b")],
            "page": 1, "pageSize": 2, "total": 3, "hasNext": true,
            "sortBy": "created_at", "sortOrder": "descending"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [item("c")],
            "page": 2, "pageSize": 2, "total": 3, "hasNext": false,
            "sortBy": "created_at", "sortOrder": "descending"
        })))
        .mount(&server)
        .await;

    let first = client
        .links()
        .list()
        .page(1)
        .page_size(2)
        .sort_by(SortBy::CreatedAt)
        .sort_order(SortOrder::Descending)
        .send()
        .await
        .expect("page 1 loads");
    assert_eq!(first.items.len(), 2);
    assert!(first.has_next);
    assert_eq!(
        first.items[0].expire_after.map(|d| d.timestamp()),
        Some(1767225599)
    );

    let second = first
        .next_page()
        .await
        .expect("page 2 loads")
        .expect("page 2 exists");
    assert_eq!(second.items.len(), 1);
    assert!(!second.has_next);
    assert!(second.next_page().await.expect("no error").is_none());

    // The stream sees all three items across the page boundary.
    #[cfg(feature = "stream")]
    {
        use futures_util::StreamExt as _;
        let fresh = client
            .links()
            .list()
            .page(1)
            .page_size(2)
            .sort_by(SortBy::CreatedAt)
            .sort_order(SortOrder::Descending)
            .send()
            .await
            .expect("page 1 reloads");
        let ids: Vec<String> = fresh
            .stream()
            .map(|item| item.expect("stream item ok").id)
            .collect()
            .await;
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
}

#[tokio::test]
async fn list_filter_is_one_json_param() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls"))
        .and(query_param(
            "filter",
            r#"{"passwordSet":true,"search":"promo","status":"ACTIVE"}"#,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [], "page": 1, "pageSize": 20, "total": 0, "hasNext": false,
            "sortBy": "created_at", "sortOrder": "descending"
        })))
        .expect(1)
        .mount(&server)
        .await;

    client
        .links()
        .list()
        .status(SettableStatus::Active)
        .password_set(true)
        .search("promo")
        .send()
        .await
        .expect("filtered list succeeds");
}

#[tokio::test]
async fn list_tag_filters_share_the_filter_param() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls"))
        .and(query_param(
            "filter",
            r#"{"status":"ACTIVE","tagIds":["t1"],"tagNames":["launch","q3"],"tagsMatch":"all"}"#,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [], "page": 1, "pageSize": 20, "total": 0, "hasNext": false,
            "sortBy": "created_at", "sortOrder": "descending"
        })))
        .expect(1)
        .mount(&server)
        .await;

    client
        .links()
        .list()
        .status(SettableStatus::Active)
        .tag_ids(["t1"])
        .tag_names(["launch", "q3"])
        .tags_match(TagsMatch::All)
        .send()
        .await
        .expect("tag-filtered list succeeds");
}

#[tokio::test]
async fn check_alias_decodes_typed_reason() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/shorten/check-alias"))
        .and(query_param("alias", "taken-one"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"available": false, "reason": "taken"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let check = client
        .links()
        .check_alias("taken-one")
        .send()
        .await
        .expect("check succeeds");
    assert!(!check.available);
    assert_eq!(check.reason, Some(AliasIssue::Taken));
}

#[tokio::test]
async fn bulk_partial_success_is_data() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/urls/bulk/status"))
        .and(body_json(json!({
            "ids": ["665f0c2f9e7a4b1d2c3d4e5f", "665f0c2f9e7a4b1d2c3d4e60"],
            "status": "INACTIVE"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "summary": {"total": 2, "succeeded": 1, "failed": 1},
            "results": [
                {"id": "665f0c2f9e7a4b1d2c3d4e5f", "alias": "a", "ok": true},
                {"id": "665f0c2f9e7a4b1d2c3d4e60", "ok": false,
                 "error_code": "not_found", "error": "no such URL"}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = client
        .links()
        .bulk_set_status(
            ["665f0c2f9e7a4b1d2c3d4e5f", "665f0c2f9e7a4b1d2c3d4e60"],
            SettableStatus::Inactive,
        )
        .await
        .expect("bulk call itself succeeds");
    assert_eq!(outcome.summary.failed, 1);
    assert_eq!(outcome.results[1].error_code, Some(BulkErrorCode::NotFound));
}

#[tokio::test]
async fn bulk_expiry_clear_sends_null() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/urls/bulk/expiry"))
        .and(body_json(json!({
            "ids": ["665f0c2f9e7a4b1d2c3d4e5f"],
            "expire_after": null
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "summary": {"total": 1, "succeeded": 1, "failed": 0},
            "results": [{"id": "665f0c2f9e7a4b1d2c3d4e5f", "ok": true}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    client
        .links()
        .bulk_set_expiry(["665f0c2f9e7a4b1d2c3d4e5f"], None)
        .await
        .expect("bulk expiry clear succeeds");
}

#[tokio::test]
async fn bulk_update_tags_sends_add_and_remove() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/urls/bulk/tags"))
        .and(body_json(json!({
            "ids": ["665f0c2f9e7a4b1d2c3d4e5f", "665f0c2f9e7a4b1d2c3d4e60"],
            "add": ["t1"],
            "remove": ["t2", "t3"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "summary": {"total": 2, "succeeded": 1, "failed": 1},
            "results": [
                {"id": "665f0c2f9e7a4b1d2c3d4e5f", "alias": "a", "ok": true},
                {"id": "665f0c2f9e7a4b1d2c3d4e60", "alias": "b", "ok": false,
                 "error_code": "validation_error", "error": "over 10 tags"}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = client
        .links()
        .bulk_update_tags(
            ["665f0c2f9e7a4b1d2c3d4e5f", "665f0c2f9e7a4b1d2c3d4e60"],
            ["t1"],
            ["t2", "t3"],
        )
        .await
        .expect("bulk tags call itself succeeds");
    assert_eq!(outcome.summary.succeeded, 1);
    assert_eq!(
        outcome.results[1].error_code,
        Some(BulkErrorCode::ValidationError)
    );
}

#[tokio::test]
async fn bulk_update_tags_remove_only_sends_empty_add() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/urls/bulk/tags"))
        .and(body_json(json!({
            "ids": ["665f0c2f9e7a4b1d2c3d4e5f"],
            "add": [],
            "remove": ["t2"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "summary": {"total": 1, "succeeded": 1, "failed": 0},
            "results": [{"id": "665f0c2f9e7a4b1d2c3d4e5f", "ok": true}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    client
        .links()
        .bulk_update_tags(["665f0c2f9e7a4b1d2c3d4e5f"], [], ["t2"])
        .await
        .expect("remove-only bulk succeeds");
}

#[tokio::test]
async fn claim_wire_field_is_token() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/urls/claim"))
        .and(body_json(json!({
            "claims": [{"url_id": "665f0c2f9e7a4b1d2c3d4e5f", "token": "tok_once"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"url_id": "665f0c2f9e7a4b1d2c3d4e5f", "status": "claimed"}],
            "claimed": 1
        })))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = client
        .links()
        .claim([ClaimRequest::new("665f0c2f9e7a4b1d2c3d4e5f", "tok_once")])
        .await
        .expect("claim succeeds");
    assert_eq!(outcome.claimed, 1);
    assert_eq!(outcome.results[0].status, ClaimStatus::Claimed);
}

#[tokio::test]
async fn unknown_enum_values_do_not_break_decoding() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/urls/665f0c2f9e7a4b1d2c3d4e5f"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "665f0c2f9e7a4b1d2c3d4e5f",
            "status": "QUARANTINED",
            "password_set": false,
            "brand_new_field": {"nested": true}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let link = client
        .links()
        .get("665f0c2f9e7a4b1d2c3d4e5f")
        .await
        .expect("decoding survives unknown values");
    assert_eq!(link.status, Some(LinkStatus::Unknown));
}
