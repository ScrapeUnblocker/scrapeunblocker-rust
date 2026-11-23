//! Official Rust client for the [ScrapeUnblocker](https://www.scrapeunblocker.com) API.
//!
//! Scrape any page past Cloudflare, DataDome, PerimeterX, Akamai and other
//! anti-bot protection, as rendered HTML, clean Markdown or parsed JSON; run
//! Google searches; fetch images; and call ready-made plugins for sites such
//! as Amazon, Walmart, Google Maps, Zillow and Booking.com.
//!
//! ```no_run
//! use scrapeunblocker::{Client, Step};
//!
//! # async fn run() -> scrapeunblocker::Result<()> {
//! let su = Client::from_env()?; // reads SCRAPEUNBLOCKER_KEY
//!
//! // Rendered HTML
//! let html = su.get_page_source("https://example.com").await?;
//!
//! // Clean Markdown for an LLM
//! let md = su.get_markdown("https://en.wikipedia.org/wiki/Web_scraping").await?;
//!
//! // Structured JSON, no parser to write
//! let page = su.get_parsed("https://www.amazon.com/dp/B08N5WRWNW").proxy_country("us").await?;
//! println!("{:?} {}", page.page_type, page.data["title"]);
//!
//! // Google results
//! let serp = su.serp("best running shoes").pages_to_check(2).await?;
//!
//! // A plugin: every option is a setter on the request
//! let results = su
//!     .amazon_search("wireless headphones")
//!     .marketplace("amazon.de")
//!     .sort("price_asc")
//!     .await?;
//!
//! // Drive the browser before the HTML is captured
//! let html = su
//!     .get_page_source("https://example.com")
//!     .steps([Step::click("#more"), Step::wait_for(".results")])
//!     .await?;
//! # Ok(()) }
//! ```
//!
//! Every endpoint method returns a request builder: set options on it, then
//! `.await` it (or call `.send()`). Builders own a cheap clone of the client,
//! so a request can be handed to `tokio::spawn`. Plugin calls return the
//! API's JSON as a [`serde_json::Value`].
//!
//! Errors are [`Error`]; an error response from the API is [`Error::Api`] with
//! an [`ApiErrorKind`]. HTTP 429/502/503/504 and network errors are retried
//! twice with backoff (see [`ClientBuilder::max_retries`]).
//!
//! The blocking client (`blocking::Client`, feature `blocking`) has the same
//! methods; finish a request with `.send()`.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]

mod client;
mod error;
mod models;
#[rustfmt::skip]
pub mod plugins;
mod request;

#[cfg(feature = "blocking")]
pub mod blocking;

pub use client::{
    Client, ClientBuilder, GetImage, GetMarkdown, GetPageSource, GetPageWithCookies,
    GetPageWithHeaders, GetPageWithXhr, GetParsed, ListElements, Plugin, Serp, API_KEY_ENV,
    DEFAULT_BASE_URL, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT, VERSION,
};
pub use error::{ApiError, ApiErrorKind, Error, Result, StepFailure, TargetNotFound};
pub use models::{CapturedPage, HeaderValues, PageHeaders, PageResult, ParsedPage, Step, XhrCall};
pub use request::ResponseFuture;

pub use serde_json;
