//! Run with: SCRAPEUNBLOCKER_KEY=... cargo run --example quickstart
use scrapeunblocker::Client;

#[tokio::main]
async fn main() -> scrapeunblocker::Result<()> {
    let su = Client::from_env()?;

    let html = su.get_page_source("https://example.com").await?;
    println!("HTML: {} bytes", html.len());

    let md = su.get_markdown("https://example.com").await?;
    println!("Markdown:\n{md}");

    let serp = su.serp("web scraping api").await?;
    let first = &serp["organic"][0];
    println!("Top result: {} {}", first["title"], first["url"]);
    Ok(())
}
