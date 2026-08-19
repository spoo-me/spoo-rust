//! Create a link, check an alias, list your links.
//!
//! Run with: SPOO_API_KEY=spoo_... cargo run --example shorten

use spoo_me::{Client, SortBy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;

    let check = client.links().check_alias("launch").send().await?;
    println!("alias 'launch' available: {}", check.available);

    let link = client
        .links()
        .create("https://example.com/launch")
        .max_clicks(10_000)
        .send()
        .await?;
    println!("created {} -> {}", link.short_url, link.long_url);

    let page = client
        .links()
        .list()
        .page_size(10)
        .sort_by(SortBy::CreatedAt)
        .send()
        .await?;
    println!("{} links total, showing {}:", page.total, page.items.len());
    for item in &page.items {
        println!(
            "  {}  {} clicks",
            item.alias.as_deref().unwrap_or("-"),
            item.total_clicks.unwrap_or(0)
        );
    }

    client.links().delete(&link.id).await?;
    println!("cleaned up {}", link.id);
    Ok(())
}
