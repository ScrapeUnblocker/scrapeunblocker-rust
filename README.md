# ScrapeUnblocker Rust client

Official Rust client for the [ScrapeUnblocker](https://www.scrapeunblocker.com?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk) web scraping API.

Scrape any page past Cloudflare, DataDome, PerimeterX, Akamai and other
anti-bot protection, as rendered HTML, clean Markdown or parsed JSON; run
Google searches; fetch images; and call ready-made plugins for Amazon,
Walmart, Google Maps, Zillow, Booking.com and 170+ other sites. Async
(Tokio) with an optional blocking client, typed errors and automatic retries.

[![crates.io](https://img.shields.io/crates/v/scrapeunblocker.svg)](https://crates.io/crates/scrapeunblocker)
[![docs.rs](https://img.shields.io/docsrs/scrapeunblocker)](https://docs.rs/scrapeunblocker)

## Install

```bash
cargo add scrapeunblocker
cargo add tokio --features macros,rt-multi-thread
```

Requires Rust 1.88+.

## Quickstart

```rust
use scrapeunblocker::Client;

#[tokio::main]
async fn main() -> scrapeunblocker::Result<()> {
    let su = Client::from_env()?; // reads SCRAPEUNBLOCKER_KEY, or Client::new("YOUR_API_KEY")?

    // Rendered HTML for any URL
    let html = su.get_page_source("https://example.com").await?;

    // Structured JSON instead of HTML (products, listings, search results, ...)
    let page = su.get_parsed("https://www.amazon.com/dp/B08N5WRWNW").await?;
    println!("{:?} {}", page.page_type, page.data["title"]);
    Ok(())
}
```

Get your API key at [app.scrapeunblocker.com](https://app.scrapeunblocker.com?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk). The free trial does not require a credit card.

Every endpoint method returns a request builder: set options on it, then
`.await` it (or call `.send().await`). Options you leave out are not sent, so
the API's own defaults apply. A builder owns a cheap clone of the client, so a
request can be handed straight to `tokio::spawn`.

## Authentication

```rust
use std::time::Duration;
use scrapeunblocker::Client;

let su = Client::new("YOUR_API_KEY")?;
let su = Client::from_env()?;            // SCRAPEUNBLOCKER_KEY
let su = Client::builder()
    .api_key("YOUR_API_KEY")
    .timeout(Duration::from_secs(120))   // default 180 s
    .max_retries(3)                      // default 2
    .user_agent("my-crawler/1.4")        // your app's token, sent after the SDK's
    .build()?;
```

## Fetch rendered HTML

```rust
let html = su
    .get_page_source("https://example.com")
    .proxy_country("de")      // route through a country
    .time_sleep(2)            // extra seconds after load
    .await?;
```

## Markdown for LLMs

```rust
let md = su.get_markdown("https://en.wikipedia.org/wiki/Web_scraping").await?;

// Only the main content by default (no navigation, site header, footer or
// sidebars). Convert the whole page instead:
let full = su.get_markdown(url).main_content_only(false).await?;
```

More in the [Markdown guide](https://docs.scrapeunblocker.com/guides/markdown?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk).

## Browser steps

Run actions in a real browser after the page loads; you get the HTML of the
resulting state.

```rust
use scrapeunblocker::Step;

let html = su
    .get_page_source("https://example.com")
    .steps([
        Step::type_text("input[name=q]", "wireless headphones"),
        Step::press_key("Enter"),
        Step::wait_for(".search-results").timeout_ms(10_000),
    ])
    .await?;
```

`Step` has `wait_for`, `wait_for_text`, `wait`, `click`, `type_text`,
`select`, `press_key`, `scroll_to_bottom` and `scroll_by`, plus
`selector_type`, `timeout_ms` and `clear` modifiers. Find the selectors first
with `list_elements`:

```rust
let page = su.list_elements("https://example.com").await?;
for el in page["elements"].as_array().into_iter().flatten() {
    println!("{} {} {}", el["tag"], el["selector"], el["text"]);
}
```

Full action reference in the [browser steps guide](https://docs.scrapeunblocker.com/guides/browser-steps?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk).

## Parsed JSON

```rust
let page = su.get_parsed("https://www.amazon.com/dp/B08N5WRWNW").await?;
println!("{:?} {:?}", page.page_type, page.source);
println!("{}", page.data["price"]);

// If a parse ever comes back wrong, force a fresh set of rules:
let page = su.get_parsed(url).refresh_rules(true).rules_hint("price is missing").await?;

// The page rendered but held no structured data: billed like any page,
// and the HTML is already here
if !page.data_extracted {
    println!("{:?}", page.detail);
    let html = page.html;
}
```

## Google search (SERP)

```rust
let serp = su.serp("best running shoes").pages_to_check(2).proxy_country("us").await?;
for result in serp["organic"].as_array().into_iter().flatten() {
    println!("{} {} {}", result["position"], result["title"], result["url"]);
}
```

`.full_urls(true)` waits until every result has its real URL (up to about 40
seconds longer, billed the same).

## Plugins

Ready-made scrapers for specific sites that return structured JSON
(`serde_json::Value`). Each method takes the plugin's required input
positionally; every other parameter is a setter. Every parameter is documented
on [docs.rs](https://docs.rs/scrapeunblocker) and on the
[plugin pages](https://docs.scrapeunblocker.com/plugins/overview?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk).

```rust
let products = su.target_search("coffee maker").sort("price_low").max_results(48).await?;
let milk = su.woolworths_product("888137").await?;
let trends = su.google_trends(["coffee", "tea"]).geo("US").await?;      // sent comma-separated
let reviews = su.trustpilot_reviews("www.nike.com").stars([4, 5]).await?;
let flights = su
    .skyscanner_flights()
    .origin("London")
    .dest("New York")
    .depart_date("2026-09-01")
    .currency("USD")
    .await?;
```

<!-- plugin-methods:start -->
| Category | Methods |
| --- | --- |
| Travel | `skyscanner_flights`, `skyscanner_hotels`, `skyscanner_carhire`, `booking_hotels`, `booking_reviews`, `airbnb_search`, `airbnb_room`, `airbnb_reviews`, `tripadvisor_list`, `tripadvisor_reviews` |
| Google | `google_local`, `google_images`, `maps_place_details`, `maps_reviews`, `google_trends`, `google_shopping`, `google_jobs`, `google_hotels`, `google_hotels_property`, `google_news`, `google_ads_transparency`, `google_ads_transparency_advertisers`, `google_ai_mode`, `google_lens` |
| AI | `gemini`, `chatgpt` |
| Search engines | `bing_search`, `bing_images`, `duckduckgo_search`, `yahoo_search`, `yahoo_images`, `brave_search`, `brave_images` |
| E-commerce | `amazon_product`, `amazon_search`, `amazon_bestsellers`, `amazon_reviews`, `amazon_offers`, `amazon_seller`, `walmart_search`, `walmart_product`, `target_search`, `target_category`, `target_product`, `bestbuy_search`, `bestbuy_product`, `homedepot_search`, `costco_search`, `costco_product`, `etsy_search`, `etsy_product`, `shein_search`, `zalando_search`, `aliexpress_search`, `aliexpress_product`, `alibaba_search`, `alibaba_product`, `alibaba1688_search`, `alibaba1688_product`, `tiktok_shop_search`, `tiktok_shop_product`, `tiktok_shop_store`, `oopbuy_search`, `smyths_inventory`, `smyths_details`, `smyths_search`, `smyths_category`, `wish_search`, `wish_product`, `shopee_search`, `shopee_product`, `taobao_search`, `taobao_product`, `flipkart_search`, `flipkart_product`, `lazada_search`, `lazada_product`, `allegro_search`, `allegro_product`, `falabella_search`, `falabella_product`, `magazineluiza_search`, `magazineluiza_product`, `mediamarkt_search`, `mediamarkt_product`, `cdiscount_search`, `cdiscount_product`, `idealo_search`, `idealo_offers`, `rakuten_search`, `rakuten_product`, `indiamart_search`, `indiamart_product`, `bedbathandbeyond_search`, `bedbathandbeyond_product`, `petco_search`, `petco_product`, `neobits_product`, `lowes_search`, `lowes_product`, `menards_search`, `menards_product`, `grainger_search`, `grainger_product` |
| Classifieds & Resale | `ebay_product`, `ebay_search`, `vinted_search`, `leboncoin_search` |
| Groceries | `woolworths_search`, `woolworths_category`, `woolworths_specials`, `woolworths_product`, `coles_search`, `coles_category`, `coles_specials`, `coles_product`, `aldi_au_search`, `aldi_au_category`, `aldi_au_specials`, `aldi_au_product`, `kroger_search`, `kroger_product`, `instacart_search`, `instacart_product`, `publix_search`, `publix_product`, `bodegaaurrera_search`, `bodegaaurrera_product` |
| Real estate | `zillow_search`, `zillow_detail`, `redfin_search`, `redfin_detail`, `rightmove_search`, `idealista_search`, `idealista_detail`, `immobilienscout24_search` |
| Vehicles | `mobile_de`, `autoscout24_search` |
| Jobs | `indeed_jobs`, `glassdoor_jobs`, `glassdoor_reviews`, `glassdoor_salaries`, `naukri_jobs`, `wellfound_jobs`, `wellfound_job`, `wellfound_company`, `wellfound_company_search`, `career_site_jobs`, `upwork_jobs` |
| Reviews & Companies | `trustpilot_reviews`, `g2_reviews`, `g2_search`, `capterra_reviews`, `capterra_search`, `product_hunt`, `crunchbase_company`, `crunchbase_search` |
| Local & Maps | `yelp_search`, `yelp_business`, `yelp_reviews`, `naver_place_search`, `naver_place_details` |
| Apps | `google_play_app`, `google_play_search`, `app_store_app`, `app_store_search`, `app_store_reviews` |
| Video & Music | `youtube_channel`, `youtube_search`, `youtube_transcript`, `spotify_search`, `spotify_artist`, `spotify_album`, `spotify_track`, `spotify_playlist` |
| Sports | `sofascore_events`, `sofascore_match`, `sofascore_search`, `sofascore_team`, `sofascore_player`, `sofascore_tournament` |
| SEO & Ads | `similarweb_overview`, `ahrefs`, `semrush_overview`, `meta_ad_library` |
<!-- plugin-methods:end -->

Any plugin endpoint can also be called by path, with parameters named as the
API names them - handy for an endpoint added after your SDK version:

```rust
let data = su
    .plugin("/marketplace/target-search")
    .param("query", "tv")
    .param("max_results", 24)
    .await?;
```

## Cookies and the serving proxy

```rust
let page = su.get_page_with_cookies("https://example.com").await?;
println!("{:?} {} {:?}", page.html, page.cookies, page.proxy);
```

## Target status, final URL and headers

`get_page_with_headers` returns the page with what a plain proxy would have
shown you: the target's own status code, the URL it ended on after redirects,
its response headers and the request headers that were actually sent.

```rust
let page = su
    .get_page_with_headers("https://example.com")
    .get_cookies(true) // optional: also return the cookies
    .await?;
println!("{:?} {:?} {:?}", page.status_code, page.final_url, page.proxy);
if let Some(ct) = page.header("content-type").and_then(|v| v.as_str()) {
    println!("content-type: {ct}");
}
if let Some(cookies) = page.header("set-cookie") {
    for cookie in cookies.values() {
        println!("set-cookie: {cookie}");
    }
}
```

Header names are lowercase. A header sent more than once is joined with `", "`,
except `set-cookie`, which is always a list: each value is a `HeaderValues`
(`One` or `Many`). Fields the render path could not observe are `None`. A
target's own 401, 451 or 5xx page comes back with that `status_code` (billed);
a target 404 or 410 is `ApiErrorKind::TargetNotFound`, with the full JSON on
`ApiError::body`.

## The page's own API calls (XHR/fetch)

Many sites render from a JSON API the page calls itself, and that API often
refuses direct requests. `get_page_with_xhr` renders the page in a real browser
and returns every XHR/fetch response whose URL contains the fragment you pass,
in the order the page received them. Add `steps` to make the page fire the call
(a search, "load more").

```rust
use scrapeunblocker::Step;

let page = su
    .get_page_with_xhr("https://quotes.toscrape.com/scroll", "/api/quotes")
    .steps([Step::scroll_to_bottom(), Step::wait(1500)])
    .await?;
for call in &page.xhr {
    if call.status == 200 {
        println!("{} {}", call.url, call.json()?["quotes"][0]["text"]);
    }
}
```

Each `XhrCall` has `url`, `method`, `status`, `content_type`, `body` (text) and
`body_omitted` (why there is no body). Up to 20 responses and 10 MB of bodies;
`page.xhr_truncated` is `true` when more matched. When the site answers every
matching call with 403 or 429, the request fails like a block and
is not billed.

## Sticky sessions

Set `session` to keep a flow on one IP, the way you would with a sticky proxy
(a site that ties a cart, a login-free session or a rate limit to the visitor's
address). Calls with the same session id leave through the same proxy IP while
the session is in use; it expires after 10 minutes without calls and belongs to
your API key only. The id is 1-64 letters, digits, `_`, `.` or `-` (anything
else is an HTTP 422, `ApiErrorKind::Validation`). Every page builder has
`.session(id)`, and it combines with their other options: `get_page_source`,
`get_markdown`, `list_elements`, `get_parsed`, `get_page_with_cookies`,
`get_page_with_headers` and `get_page_with_xhr`.

```rust
let first = su
    .get_page_with_headers("https://example.com/cart")
    .session("cart-42")
    .await?;
let second = su
    .get_page_with_headers("https://example.com/checkout")
    .session("cart-42")
    .await?;
if second.session_changed {
    println!("the session moved to a new IP");
}
```

When the session's IP cannot get the page, the call is answered through another
IP, `session_changed` is `true`, and the session moves to the new IP. Up to 4
calls of one session run on its IP at once; more parallel calls are answered
through other IPs (also flagged) without moving the session. A call that names a
different `proxy_country` than the session's gets a new IP in that country
(flagged).

`session_changed` is on `ParsedPage`, `PageResult`, `PageHeaders` and
`CapturedPage`. `get_page_source` and `get_markdown` return a `String` and
`list_elements` a JSON `Value`, which cannot carry it: use one of the other
builders when you need to know whether the IP changed.

## Images

```rust
let bytes = su.get_image("https://example.com/photo.jpg").await?;
std::fs::write("photo.jpg", bytes)?;
```

The bytes come back exactly as the image host serves them, in the original
format (JPEG, PNG, WebP, AVIF, GIF, SVG, ...). Pass the image URL itself, not
the page it sits on.

## Concurrency

Plan limits are concurrency: how many requests run at the same time on a key.
Cap your own in-flight requests to match, e.g. with `futures::stream::iter(urls).map(...).buffer_unordered(N)`
or a `tokio::sync::Semaphore`; a request over the limit is answered 429 and
retried with backoff.

## Blocking client

Enable the `blocking` feature for code without an async runtime. Same methods,
finish a request with `.send()`:

```toml
scrapeunblocker = { version = "0.22", features = ["blocking"] }
```

```rust
use scrapeunblocker::blocking::Client;

let su = Client::from_env()?;
let html = su.get_page_source("https://example.com").send()?;
let results = su.amazon_search("wireless headphones").marketplace("amazon.de").send()?;
```

Like `reqwest::blocking`, it runs its own small runtime, so do not use it
inside async code.

## Error handling

Every call returns `scrapeunblocker::Result<T>`. An error response from the API
is `Error::Api`, whose `kind` says what went wrong:

```rust
use scrapeunblocker::{ApiErrorKind, Error};

match su.get_page_source("https://example.com").await {
    Ok(html) => println!("{} bytes", html.len()),
    Err(Error::Api(e)) => match e.kind {
        ApiErrorKind::Blocked => {}                       // 403: blocked on every bypass path (not billed)
        k if k.is_payment_required() => {}                // 402: quota, credit limit, budget or failed payment
        ApiErrorKind::RateLimit => {}                     // 429: over your concurrency limit
        ApiErrorKind::UpstreamOutage => {}                // 503: the target site itself is down
        ApiErrorKind::TargetNotFound => {
            let t = e.target_not_found.as_ref().unwrap();  // 404/410 from the target (billed)
            println!("{} {:?}", t.origin_status, t.html);
        }
        _ => println!("{e}"),
    },
    Err(Error::Timeout(_)) => {}                          // this client gave up locally
    Err(e) => println!("{e}"),
}
```

| `ApiErrorKind` | Status | Meaning |
|---|---|---|
| `InvalidRequest` | 400 | Bad URL, unsupported scheme, or the API key header was not sent |
| `Authentication` | 401 | Key not recognised - typo, stray whitespace, or a rotated key |
| `NoSubscription` | 401 | Key is fine, but the account has no active plan |
| `QuotaExceeded` | 402 | The plan's requests for this period are used up |
| `CreditLimitExceeded` | 402 | Unpaid balance is past the account's credit limit |
| `BudgetExceeded` | 402 | This period's spend reached the monthly budget limit you set in your profile |
| `PaymentFailed` | 402 | A card payment was declined three times |
| `PaymentRequired` | 402 | Any other billing block |
| `Blocked` | 403 | Blocked by bot protection on every path |
| `NotFound` | 404 | What you asked for does not exist - a plugin lookup found nothing |
| `TargetNotFound` | 404 / 410 | The target page itself does not exist; `target_not_found` has `origin_status`, `html`, `destination_url` (billed) |
| `BrowserTimeout` | 408 | Our browser run timed out before the page was ready |
| `UnsupportedContent` | 415 | The URL serves something other than HTML (`get_page_source`, `get_markdown`) or other than an image (`get_image`) |
| `Validation` | 422 | Missing or wrong-typed parameter; `body` holds the `detail` array |
| `StepFailed` | 422 | A browser step failed; `step_failed` has `step_index`, `action`, `reason`, `selector`, `html` |
| `RateLimit` | 429 | Too many concurrent requests |
| `UpstreamOutage` | 503 | The target origin is down |
| `Server` | 5xx | Unexpected server error, including a 504 upstream timeout |

Outside `Error::Api`: `Error::Timeout` (this client gave up locally),
`Error::Connection` (could not reach the API), `Error::MissingApiKey`,
`Error::Decode` and `Error::Config`.

HTTP 429, 502, 503, 504 and network errors are retried automatically with
exponential backoff (`ClientBuilder::max_retries`). A 401 or 402 is never
retried - it clears when the key or the billing state changes - and neither is
billed.

## TLS

The default TLS backend is reqwest's default. To choose another, turn off
default features:

```toml
scrapeunblocker = { version = "0.22", default-features = false, features = ["rustls"] }  # or "native-tls"
```

## Links

- [Documentation](https://docs.scrapeunblocker.com/sdks/rust?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk)
- [API reference](https://docs.scrapeunblocker.com/api-reference?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk)
- [Dashboard](https://app.scrapeunblocker.com?utm_source=crates.io&utm_medium=integration&utm_campaign=rust-sdk)

## License

MIT
