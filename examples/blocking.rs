//! Run with: SCRAPEUNBLOCKER_KEY=... cargo run --example blocking --features blocking
use scrapeunblocker::blocking::Client;

fn main() -> scrapeunblocker::Result<()> {
    let su = Client::from_env()?;
    let html = su.get_page_source("https://example.com").send()?;
    println!("HTML: {} bytes", html.len());
    Ok(())
}
