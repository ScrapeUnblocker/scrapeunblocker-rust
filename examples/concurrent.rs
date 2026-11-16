//! Fetch several pages at once, at most `LIMIT` in flight (your plan's concurrency).
//! Run with: SCRAPEUNBLOCKER_KEY=... cargo run --example concurrent
use std::sync::Arc;

use scrapeunblocker::Client;
use tokio::sync::Semaphore;

const LIMIT: usize = 5;

#[tokio::main]
async fn main() -> scrapeunblocker::Result<()> {
    let su = Client::from_env()?;
    let urls = [
        "https://example.com",
        "https://example.org",
        "https://example.net",
    ];
    let slots = Arc::new(Semaphore::new(LIMIT));

    let mut tasks = Vec::new();
    for url in urls {
        let request = su.get_markdown(url);
        let slots = slots.clone();
        tasks.push(tokio::spawn(async move {
            let _slot = slots.acquire().await.expect("semaphore open");
            (url, request.await)
        }));
    }
    for task in tasks {
        let (url, result) = task.await.expect("task finished");
        match result {
            Ok(md) => println!("{url}: {} chars", md.len()),
            Err(e) => println!("{url}: {e}"),
        }
    }
    Ok(())
}
