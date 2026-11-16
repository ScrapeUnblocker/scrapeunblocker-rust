//! Calls against a real API host. Ignored by default; run with
//! `SCRAPEUNBLOCKER_KEY=... [SCRAPEUNBLOCKER_BASE_URL=...] cargo test --test live -- --ignored`.

use scrapeunblocker::{ApiErrorKind, Client};

fn client() -> Client {
    let mut builder = Client::builder();
    if let Ok(base) = std::env::var("SCRAPEUNBLOCKER_BASE_URL") {
        builder = builder.base_url(base);
    }
    builder.user_agent("sdk-live-test").build().unwrap()
}

#[tokio::test]
#[ignore]
async fn page_markdown_and_parsed() {
    let su = client();
    let html = su.get_page_source("https://example.com").await.unwrap();
    assert!(html.contains("Example Domain"), "{html}");

    let md = su.get_markdown("https://example.com").await.unwrap();
    assert!(md.contains("documentation examples"), "{md}");

    let page = su.get_parsed("https://example.com").await.unwrap();
    println!(
        "parsed: {:?} extracted={}",
        page.page_type, page.data_extracted
    );
}

#[tokio::test]
#[ignore]
async fn serp_and_a_plugin() {
    let su = client();
    let serp = su.serp("scrapeunblocker").await.unwrap();
    assert!(
        serp["organic"].as_array().is_some_and(|r| !r.is_empty()),
        "{serp}"
    );

    let trends = su.google_trends(["coffee", "tea"]).geo("US").await.unwrap();
    assert!(trends.is_object(), "{trends}");
}

#[tokio::test]
#[ignore]
async fn a_bad_key_is_an_authentication_error() {
    let mut builder = Client::builder().api_key("not-a-real-key");
    if let Ok(base) = std::env::var("SCRAPEUNBLOCKER_BASE_URL") {
        builder = builder.base_url(base);
    }
    let err = builder
        .build()
        .unwrap()
        .get_page_source("https://example.com")
        .await
        .unwrap_err();
    assert_eq!(
        err.api().map(|e| e.kind),
        Some(ApiErrorKind::Authentication),
        "{err}"
    );
}
