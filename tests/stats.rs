//! Stats and export wire tests, including the filename law.

#![cfg(not(target_arch = "wasm32"))]
mod common;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use spoo_me::{Dimension, ExportFormat, FilterDimension, Metric};

fn stats_body() -> serde_json::Value {
    json!({
        "scope": "all",
        "filters": {"browser": ["Chrome"]},
        "group_by": ["time"],
        "timezone": "UTC",
        "time_range": {"start_date": "2026-01-01T00:00:00Z", "end_date": "2026-01-08T00:00:00Z"},
        "summary": {
            "total_clicks": 120,
            "unique_clicks": 80,
            "first_click": "2026-01-01T10:00:00Z",
            "last_click": "2026-01-07T22:00:00Z",
            "avg_redirection_time": 42.5
        },
        "metrics": {
            "clicks_by_time": [
                {"time": "2026-01-01", "clicks": 60, "clicks_percentage": 50.0},
                {"time": "2026-01-02", "clicks": 60, "clicks_percentage": 50.0}
            ]
        },
        "generated_at": "2026-01-08T00:00:01Z",
        "time_bucket_info": {
            "strategy": "daily",
            "mongo_format": "%Y-%m-%d",
            "display_format": "MMM D",
            "timezone": "UTC"
        },
        "computed_metrics": {
            "unique_click_rate": 0.66,
            "repeat_click_rate": 0.34,
            "average_clicks_per_visitor": 1.5
        }
    })
}

#[tokio::test]
async fn account_stats_query_encoding() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stats"))
        .and(query_param("group_by", "time,browser"))
        .and(query_param("metrics", "clicks,unique_clicks"))
        .and(query_param("timezone", "Asia/Kolkata"))
        .and(query_param(
            "filters",
            r#"{"browser":["Chrome"],"url_id":["665f0c2f9e7a4b1d2c3d4e5f"]}"#,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(stats_body()))
        .expect(1)
        .mount(&server)
        .await;

    let report = client
        .stats()
        .account()
        .group_by(Dimension::Time)
        .group_by(Dimension::Browser)
        .metric(Metric::Clicks)
        .metric(Metric::UniqueClicks)
        .timezone("Asia/Kolkata")
        .filter(FilterDimension::Browser, ["Chrome"])
        .url_ids(["665f0c2f9e7a4b1d2c3d4e5f"])
        .send()
        .await
        .expect("stats load");
    assert_eq!(report.summary.total_clicks, 120);
    assert_eq!(report.metrics["clicks_by_time"].len(), 2);
    assert_eq!(
        report
            .time_bucket_info
            .expect("bucket info present")
            .strategy,
        "daily"
    );
}

#[tokio::test]
async fn link_stats_carries_identity() {
    let (server, client) = common::server_and_client().await;
    let mut body = stats_body();
    body["url_id"] = json!("665f0c2f9e7a4b1d2c3d4e5f");
    body["alias"] = json!("launch");
    Mock::given(method("GET"))
        .and(path("/api/v1/stats/links/665f0c2f9e7a4b1d2c3d4e5f"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let report = client
        .stats()
        .for_link("665f0c2f9e7a4b1d2c3d4e5f")
        .send()
        .await
        .expect("link stats load");
    assert_eq!(report.alias, "launch");
    assert_eq!(report.stats.summary.unique_clicks, 80);
}

#[tokio::test]
async fn export_uses_server_filename_when_safe() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/export/links/665f0c2f9e7a4b1d2c3d4e5f"))
        .and(query_param("format", "json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "content-disposition",
                    "attachment; filename=\"launch-stats.json\"",
                )
                .set_body_raw("{\"rows\":[]}", "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let export = client
        .stats()
        .export_link("665f0c2f9e7a4b1d2c3d4e5f")
        .format(ExportFormat::Json)
        .send()
        .await
        .expect("export downloads");
    assert_eq!(export.filename, "launch-stats.json");
    assert_eq!(export.content_type, "application/json");
    let bytes = export.bytes().await.expect("body buffers");
    assert_eq!(bytes, b"{\"rows\":[]}");
}

#[tokio::test]
async fn hostile_export_filenames_fall_back() {
    let cases = [
        "attachment; filename=\"../../../evil.json\"",
        "attachment; filename=\"/tmp/absolute-evil.json\"",
        "attachment; filename*=utf-8''%2e%2e%2f%2e%2e%2fesc.json",
        "attachment; filename=\"..\"",
    ];
    for disposition in cases {
        let (server, client) = common::server_and_client().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/export"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-disposition", disposition)
                    .set_body_string("{}"),
            )
            .mount(&server)
            .await;

        let export = client
            .stats()
            .export()
            .format(ExportFormat::Json)
            .send()
            .await
            .expect("export downloads");
        assert!(
            !export.filename.contains('/')
                && !export.filename.contains('\\')
                && export.filename != ".."
                && !export.filename.is_empty(),
            "unsafe filename escaped for {disposition:?}: {}",
            export.filename
        );
    }
}

#[tokio::test]
async fn csv_export_fallback_extension_is_zip() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/export"))
        .and(query_param("format", "csv"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(b"PK".to_vec(), "application/zip"))
        .expect(1)
        .mount(&server)
        .await;

    let export = client
        .stats()
        .export()
        .format(ExportFormat::Csv)
        .send()
        .await
        .expect("export downloads");
    assert_eq!(export.filename, "spoo-export.zip");
}

#[tokio::test]
#[cfg(feature = "stream")]
async fn export_body_streams() {
    let (server, client) = common::server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/export"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 64 * 1024]))
        .mount(&server)
        .await;

    use futures_util::StreamExt as _;
    let export = client
        .stats()
        .export()
        .send()
        .await
        .expect("export downloads");
    let mut total = 0usize;
    let mut stream = export.bytes_stream();
    while let Some(chunk) = stream.next().await {
        total += chunk.expect("chunk ok").len();
    }
    assert_eq!(total, 64 * 1024);
}
