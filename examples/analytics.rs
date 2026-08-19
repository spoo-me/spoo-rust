//! Pull a statistics breakdown and download an export.
//!
//! Run with: SPOO_API_KEY=spoo_... cargo run --example analytics

use spoo_me::{Client, Dimension, ExportFormat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;

    let report = client
        .stats()
        .account()
        .group_by(Dimension::Time)
        .group_by(Dimension::Country)
        .send()
        .await?;
    println!(
        "{} clicks ({} unique) between {:?} and {:?}",
        report.summary.total_clicks,
        report.summary.unique_clicks,
        report.time_range.start_date,
        report.time_range.end_date,
    );
    for (series, points) in &report.metrics {
        println!("  {series}: {} data points", points.len());
    }

    let export = client
        .stats()
        .export()
        .format(ExportFormat::Json)
        .send()
        .await?;
    // The filename is sanitized by the SDK, so joining it is safe.
    let filename = export.filename.clone();
    let bytes = export.bytes().await?;
    let path = std::env::temp_dir().join("spoo").join(filename);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes)?;
    println!("export saved to {}", path.display());
    Ok(())
}
