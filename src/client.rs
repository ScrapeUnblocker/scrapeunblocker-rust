//! The async [`Client`], its builder, and the core endpoints.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::Url;
use serde_json::Value;

use crate::error::{api_error, Error, Result};
use crate::models::{CapturedPage, PageHeaders, PageResult, ParsedPage, Step};
use crate::request::{executable, Call, Decode};

/// The production API host.
pub const DEFAULT_BASE_URL: &str = "https://api.scrapeunblocker.com";
/// Per-request timeout. Rendering heavy, protected pages can take a while.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);
/// Retries for transient failures (HTTP 429/502/503/504 and network errors).
pub const DEFAULT_MAX_RETRIES: u32 = 2;
/// The environment variable the API key is read from when none is given.
pub const API_KEY_ENV: &str = "SCRAPEUNBLOCKER_KEY";
/// This crate's version, sent in the `User-Agent` header.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const API_KEY_HEADER: &str = "x-scrapeunblocker-key";

/// Status codes worth retrying: concurrency limit, transient upstream outage
/// and gateway errors. A 400/401/402/403 is deterministic - retrying only
/// wastes time, and a 402 lifts on a billing change, not on another attempt.
const RETRYABLE_STATUS: [u16; 4] = [429, 502, 503, 504];

/// Client for the ScrapeUnblocker API.
///
/// Cheap to clone: clones share one connection pool. Every endpoint method
/// returns a request builder; set options on it and `.await` it.
///
/// ```no_run
/// # async fn run() -> scrapeunblocker::Result<()> {
/// use scrapeunblocker::Client;
///
/// let su = Client::new("YOUR_API_KEY")?; // or Client::from_env()?
/// let html = su.get_page_source("https://example.com").await?;
/// let results = su.amazon_search("wireless headphones").marketplace("amazon.de").await?;
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.inner.base_url)
            .field("timeout", &self.inner.timeout)
            .field("max_retries", &self.inner.max_retries)
            .finish_non_exhaustive()
    }
}

/// Builds a [`Client`] with non-default settings.
#[derive(Debug, Clone)]
#[must_use]
pub struct ClientBuilder {
    api_key: Option<String>,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
    app: Option<String>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        ClientBuilder {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            app: None,
        }
    }
}

impl ClientBuilder {
    /// The API key. When not set, `SCRAPEUNBLOCKER_KEY` is read.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Another API host (rarely needed).
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Per-request timeout. Default 180 seconds.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// How many times to retry HTTP 429/502/503/504 and network errors, with
    /// exponential backoff (0.5 s, 1 s, 2 s ... at most 8 s). Default 2.
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Your app's own product token, sent after the SDK's in the
    /// `User-Agent` header, e.g. `"my-crawler/1.4"`.
    pub fn user_agent(mut self, app: impl Into<String>) -> Self {
        self.app = Some(app.into());
        self
    }

    /// Build the client.
    ///
    /// Fails with [`Error::MissingApiKey`] when no key was given and
    /// `SCRAPEUNBLOCKER_KEY` is not set.
    pub fn build(self) -> Result<Client> {
        let api_key = self
            .api_key
            .or_else(|| std::env::var(API_KEY_ENV).ok())
            .map(|k| k.trim().to_owned())
            .filter(|k| !k.is_empty())
            .ok_or(Error::MissingApiKey)?;
        let mut key = HeaderValue::from_str(&api_key).map_err(|_| {
            Error::Config("the API key holds characters an HTTP header cannot carry".into())
        })?;
        key.set_sensitive(true);

        let base_url = self.base_url.trim_end_matches('/').to_owned();
        Url::parse(&base_url).map_err(|e| Error::Config(format!("base URL {base_url:?}: {e}")))?;

        let mut headers = HeaderMap::new();
        headers.insert(API_KEY_HEADER, key);
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(user_agent(self.app.as_deref()))
            .timeout(self.timeout)
            .build()
            .map_err(|e| Error::Config(e.to_string()))?;

        Ok(Client {
            inner: Arc::new(Inner {
                http,
                base_url,
                timeout: self.timeout,
                max_retries: self.max_retries,
            }),
        })
    }
}

/// The SDK's product token, then the app's own when it gives one (control
/// characters, which would break the header, become spaces).
fn user_agent(app: Option<&str>) -> String {
    let own = format!("scrapeunblocker-rust/{VERSION}");
    let app: String = app
        .unwrap_or("")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let app = app.split_whitespace().collect::<Vec<_>>().join(" ");
    if app.is_empty() {
        own
    } else {
        format!("{own} {app}")
    }
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs_f64((0.5 * 2f64.powi(attempt.min(10) as i32)).min(8.0))
}

impl Client {
    /// A client with the given API key and default settings.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        ClientBuilder::default().api_key(api_key).build()
    }

    /// A client whose API key comes from the `SCRAPEUNBLOCKER_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        ClientBuilder::default().build()
    }

    /// Start building a client with non-default settings.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Send one call, retrying transient failures, and decode the answer.
    pub(crate) async fn execute<T: Decode>(&self, call: Call) -> Result<T> {
        let mut url = Url::parse(&format!("{}{}", self.inner.base_url, call.path))
            .map_err(|e| Error::Config(format!("endpoint {:?}: {e}", call.path)))?;
        if !call.query.pairs().is_empty() {
            url.query_pairs_mut().extend_pairs(call.query.pairs());
        }

        let mut attempt = 0;
        loop {
            let sent = self.inner.http.post(url.clone()).body("").send().await;
            let response = match sent {
                Ok(response) => response,
                Err(e) if e.is_timeout() => return Err(Error::Timeout(e)),
                Err(_) if attempt < self.inner.max_retries => {
                    tokio::time::sleep(backoff(attempt)).await;
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(Error::Connection(e)),
            };

            let status = response.status().as_u16();
            if RETRYABLE_STATUS.contains(&status) && attempt < self.inner.max_retries {
                drop(response);
                tokio::time::sleep(backoff(attempt)).await;
                attempt += 1;
                continue;
            }

            let header = |name: &str| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned)
            };
            let origin_status = header("x-origin-status");
            let destination_url = header("x-destination-url");
            let success = response.status().is_success();
            let body = response.bytes().await.map_err(|e| {
                if e.is_timeout() {
                    Error::Timeout(e)
                } else {
                    Error::Connection(e)
                }
            })?;

            if success {
                return T::decode(body.to_vec());
            }
            let text = String::from_utf8_lossy(&body).into_owned();
            return Err(api_error(
                status,
                Some(text),
                origin_status.as_deref(),
                destination_url,
            )
            .into());
        }
    }

    // -- core endpoints ---------------------------------------------------

    /// Fetch a URL and return the fully rendered HTML.
    ///
    /// The page is loaded in a real browser behind the anti-bot bypass chain,
    /// so JavaScript-heavy and protected sites come back rendered. When the
    /// target page itself answers 404 or 410 the call fails with
    /// [`ApiErrorKind::TargetNotFound`](crate::ApiErrorKind::TargetNotFound)
    /// (billed; the not-found page is on its `html`).
    ///
    /// Calls `POST /getPageSource`.
    pub fn get_page_source(&self, url: impl Into<String>) -> GetPageSource {
        GetPageSource::new(self.clone(), url.into())
    }

    /// List the page's interactive elements (buttons, inputs, selects, links,
    /// forms) with ready selectors, as JSON (`{"url", "count", "elements": [...]}`).
    ///
    /// Each element's `selector` drops straight into a [`Step`].
    ///
    /// Calls `POST /getPageSource` with `list_elements=true`.
    pub fn list_elements(&self, url: impl Into<String>) -> ListElements {
        ListElements::new(self.clone(), url.into())
    }

    /// Fetch a URL and return the rendered page as clean Markdown, for LLMs,
    /// agents and RAG pipelines.
    ///
    /// Headings, lists, tables, absolute links, images with their alt text and
    /// code blocks, without scripts, styles, navigation, footers, forms or
    /// hidden elements. Billed like an HTML call.
    ///
    /// Calls `POST /getPageSource` with `markdown=true`.
    pub fn get_markdown(&self, url: impl Into<String>) -> GetMarkdown {
        GetMarkdown::new(self.clone(), url.into())
    }

    /// Fetch a URL and return structured JSON instead of HTML.
    ///
    /// The API extracts fields with Schema.org, `__NEXT_DATA__` or
    /// AI-generated rules - no per-site parser to maintain. When the page
    /// rendered but held no structured data, the result has
    /// `data_extracted == false` and the page on `html` (billed like any
    /// delivered page).
    ///
    /// Calls `POST /getPageSource` with `parsed_data=true`.
    pub fn get_parsed(&self, url: impl Into<String>) -> GetParsed {
        GetParsed::new(self.clone(), url.into())
    }

    /// Fetch a URL and also return the cookies and proxy that served it.
    ///
    /// Calls `POST /getPageSource` with `get_cookies=true`.
    pub fn get_page_with_cookies(&self, url: impl Into<String>) -> GetPageWithCookies {
        GetPageWithCookies::new(self.clone(), url.into())
    }

    /// Fetch a URL and also return the target's own status code, the URL it
    /// ended on after redirects, its response headers and the request headers
    /// that were actually sent, as a [`PageHeaders`].
    ///
    /// Useful where a plain proxy used to be: redirects, cache headers and
    /// `set-cookie` stay visible. A target's own 401, 451 or 5xx page is
    /// delivered (billed) with that status on
    /// [`status_code`](PageHeaders::status_code). A target 404 or 410 fails
    /// with [`ApiErrorKind::TargetNotFound`](crate::ApiErrorKind::TargetNotFound)
    /// like [`get_page_source`](Self::get_page_source); the full JSON, headers
    /// included, is on [`ApiError::body`](crate::ApiError::body).
    ///
    /// Calls `POST /getPageSource` with `get_headers=true`.
    pub fn get_page_with_headers(&self, url: impl Into<String>) -> GetPageWithHeaders {
        GetPageWithHeaders::new(self.clone(), url.into())
    }

    /// Fetch a URL and return the page's own XHR/fetch responses with it.
    ///
    /// Many sites render from a JSON API the page calls itself, and that API
    /// often refuses direct requests (the page signs each call). The page is
    /// rendered in a real browser and every XHR/fetch response whose URL
    /// contains `capture_xhr` (e.g. `"/api/search"`) comes back in the order
    /// the page received it: up to 20 responses and 10 MB of bodies. Add
    /// [`steps`](GetPageWithXhr::steps) to make the page fire the call. A
    /// failed step returns [`ApiErrorKind::StepFailed`](crate::ApiErrorKind::StepFailed)
    /// whose [`StepFailure::xhr`](crate::StepFailure::xhr) holds what the page
    /// received before that.
    ///
    /// Calls `POST /getPageSource` with `capture_xhr`.
    pub fn get_page_with_xhr(
        &self,
        url: impl Into<String>,
        capture_xhr: impl Into<String>,
    ) -> GetPageWithXhr {
        GetPageWithXhr::new(self.clone(), url.into(), capture_xhr.into())
    }

    /// Run a Google search and return the parsed SERP as JSON.
    ///
    /// The result has `organic` results and `topAds` / `bottomAds` (title,
    /// real url, description, position, displayedUrl, domain, favicon,
    /// sitelinks; organic adds source, date, rating, richSnippet and
    /// thumbnail, ads advertiser), `aiOverview`, `totalResults`, and the first
    /// page's other blocks: peopleAlsoAsk, relatedSearches,
    /// relatedSearchLinks, knowledgeGraph, topStories, videos, localResults,
    /// shoppingResults and spellingCorrection (each empty or null when Google
    /// shows none).
    ///
    /// Calls `POST /serpApi`.
    pub fn serp(&self, keyword: impl Into<String>) -> Serp {
        Serp::new(self.clone(), keyword.into())
    }

    /// Fetch an image and return its bytes exactly as the image host serves
    /// them - the original format (JPEG, PNG, WebP, AVIF, GIF, SVG, ...),
    /// nothing re-encoded.
    ///
    /// `url` must point at the image itself: a page URL fails with
    /// [`ApiErrorKind::UnsupportedContent`](crate::ApiErrorKind::UnsupportedContent).
    ///
    /// Calls `POST /getImage`.
    pub fn get_image(&self, url: impl Into<String>) -> GetImage {
        GetImage::new(self.clone(), url.into())
    }

    /// Call any plugin endpoint by path, with parameters as the API names
    /// them (snake_case). Use it for an endpoint this SDK version has no
    /// method for yet.
    ///
    /// ```no_run
    /// # async fn run(su: scrapeunblocker::Client) -> scrapeunblocker::Result<()> {
    /// let results = su
    ///     .plugin("/marketplace/target-search")
    ///     .param("query", "tv")
    ///     .param("page", 2)
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn plugin(&self, path: impl Into<String>) -> Plugin {
        Plugin::new(self.clone(), path.into())
    }
}

// -- request builders -----------------------------------------------------

/// Setters shared by the page-rendering builders.
macro_rules! page_setters {
    () => {
        /// ISO country code to route the request through, e.g. `"us"`.
        pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
            self.call.query.set("proxy_country", country.into());
            self
        }

        /// Extra seconds to wait after the page loads, before capture.
        pub fn time_sleep(mut self, seconds: u32) -> Self {
            self.call.query.set("time_sleep", seconds);
            self
        }

        /// Advanced render-wait method (`"css"`, `"js"`, ...), paired with [`value`](Self::value).
        pub fn method(mut self, method: impl Into<String>) -> Self {
            self.call.query.set("method", method.into());
            self
        }

        /// The selector or expression paired with [`method`](Self::method).
        pub fn value(mut self, value: impl Into<String>) -> Self {
            self.call.query.set("value", value.into());
            self
        }

        /// Cap in seconds for the render-wait method.
        pub fn method_timeout(mut self, seconds: u32) -> Self {
            self.call.query.set("method_timeout", seconds);
            self
        }
    };
}

/// Setter for browser steps.
macro_rules! steps_setter {
    () => {
        /// Browser actions to run, in order, after the page loads. See [`Step`].
        pub fn steps(mut self, steps: impl IntoIterator<Item = Step>) -> Self {
            let steps: Vec<Step> = steps.into_iter().collect();
            let json = serde_json::to_string(&steps).expect("steps serialize to JSON");
            self.call.query.set("steps", json);
            self
        }
    };
}

/// Request builder for [`Client::get_page_source`]. Await it for the HTML.
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetPageSource<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetPageSource<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        GetPageSource { client, call }
    }

    page_setters!();
    steps_setter!();
}

executable!(GetPageSource -> String);

/// Request builder for [`Client::list_elements`]. Await it for the JSON.
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct ListElements<C = Client> {
    client: C,
    call: Call,
}

impl<C> ListElements<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        call.query.set("list_elements", true);
        ListElements { client, call }
    }

    page_setters!();
}

executable!(ListElements -> Value);

/// Request builder for [`Client::get_markdown`]. Await it for the Markdown.
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetMarkdown<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetMarkdown<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        call.query.set("markdown", true);
        GetMarkdown { client, call }
    }

    /// Keep only the page's main content, without navigation, the site
    /// header, footer and sidebars (the API default). `false` converts the
    /// whole page.
    pub fn main_content_only(mut self, main_content_only: bool) -> Self {
        self.call.query.set("main_content_only", main_content_only);
        self
    }

    page_setters!();
    steps_setter!();
}

executable!(GetMarkdown -> String);

/// Request builder for [`Client::get_parsed`]. Await it for a [`ParsedPage`].
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetParsed<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetParsed<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        call.query.set("parsed_data", true);
        GetParsed { client, call }
    }

    /// Force-regenerate the cached extraction rules for this domain (use when
    /// a parse comes back clearly wrong).
    pub fn refresh_rules(mut self, refresh: bool) -> Self {
        self.call.query.set("refresh_rules", refresh);
        self
    }

    /// Free-text steer for regenerated rules, e.g. `"price is missing"`.
    pub fn rules_hint(mut self, hint: impl Into<String>) -> Self {
        self.call.query.set("rules_hint", hint.into());
        self
    }

    /// ISO country code to route the request through, e.g. `"us"`.
    pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
        self.call.query.set("proxy_country", country.into());
        self
    }

    /// Extra seconds to wait after the page loads, before capture.
    pub fn time_sleep(mut self, seconds: u32) -> Self {
        self.call.query.set("time_sleep", seconds);
        self
    }
}

executable!(GetParsed -> ParsedPage);

/// Request builder for [`Client::get_page_with_cookies`]. Await it for a [`PageResult`].
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetPageWithCookies<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetPageWithCookies<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        call.query.set("get_cookies", true);
        GetPageWithCookies { client, call }
    }

    /// ISO country code to route the request through, e.g. `"us"`.
    pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
        self.call.query.set("proxy_country", country.into());
        self
    }

    /// Extra seconds to wait after the page loads, before capture.
    pub fn time_sleep(mut self, seconds: u32) -> Self {
        self.call.query.set("time_sleep", seconds);
        self
    }
}

executable!(GetPageWithCookies -> PageResult);

/// Request builder for [`Client::get_page_with_headers`]. Await it for a [`PageHeaders`].
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetPageWithHeaders<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetPageWithHeaders<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        call.query.set("get_headers", true);
        GetPageWithHeaders { client, call }
    }

    /// Also return the cookies set during the visit, on
    /// [`PageHeaders::cookies`].
    pub fn get_cookies(mut self, get_cookies: bool) -> Self {
        if get_cookies {
            self.call.query.set("get_cookies", true);
        } else {
            self.call.query.remove("get_cookies");
        }
        self
    }

    /// ISO country code to route the request through, e.g. `"us"`.
    pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
        self.call.query.set("proxy_country", country.into());
        self
    }

    /// Extra seconds to wait after the page loads, before capture.
    pub fn time_sleep(mut self, seconds: u32) -> Self {
        self.call.query.set("time_sleep", seconds);
        self
    }
}

executable!(GetPageWithHeaders -> PageHeaders);

/// Request builder for [`Client::get_page_with_xhr`]. Await it for a [`CapturedPage`].
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetPageWithXhr<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetPageWithXhr<C> {
    pub(crate) fn new(client: C, url: String, capture_xhr: String) -> Self {
        let mut call = Call::new("/getPageSource");
        call.query.set("url", url);
        call.query.set("capture_xhr", capture_xhr);
        GetPageWithXhr { client, call }
    }

    /// ISO country code to route the request through, e.g. `"us"`.
    pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
        self.call.query.set("proxy_country", country.into());
        self
    }

    steps_setter!();
}

executable!(GetPageWithXhr -> CapturedPage);

/// Request builder for [`Client::serp`]. Await it for the SERP JSON.
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct Serp<C = Client> {
    client: C,
    call: Call,
}

impl<C> Serp<C> {
    pub(crate) fn new(client: C, keyword: String) -> Self {
        let mut call = Call::new("/serpApi");
        call.query.set("keyword", keyword);
        Serp { client, call }
    }

    /// ISO country code to search from, e.g. `"de"`.
    pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
        self.call.query.set("proxy_country", country.into());
        self
    }

    /// How many result pages to fetch and merge (1-10). Each page is billed
    /// as one request. API default: 1.
    pub fn pages_to_check(mut self, pages: u32) -> Self {
        self.call.query.set("pages_to_check", pages);
        self
    }

    /// Extra seconds to wait before extraction. Rarely needed.
    pub fn wait_after_load(mut self, seconds: u32) -> Self {
        self.call.query.set("wait_after_load", seconds);
        self
    }

    /// Wait until Google gives out every result's real URL. Google releases
    /// a results page's links a few at a time, the last ones up to about 40
    /// seconds after the search. Without this the call returns sooner, and a
    /// result whose link has not come yet has the site's home page as its
    /// `url`. Billed the same.
    pub fn full_urls(mut self, full_urls: bool) -> Self {
        self.call.query.set("full_urls", full_urls);
        self
    }
}

executable!(Serp -> Value);

/// Request builder for [`Client::get_image`]. Await it for the image bytes.
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct GetImage<C = Client> {
    client: C,
    call: Call,
}

impl<C> GetImage<C> {
    pub(crate) fn new(client: C, url: String) -> Self {
        let mut call = Call::new("/getImage");
        call.query.set("url", url);
        GetImage { client, call }
    }

    /// ISO country code to route the request through, e.g. `"us"`.
    pub fn proxy_country(mut self, country: impl Into<String>) -> Self {
        self.call.query.set("proxy_country", country.into());
        self
    }
}

executable!(GetImage -> Vec<u8>);

/// Request builder for [`Client::plugin`]. Await it for the JSON.
#[derive(Debug, Clone)]
#[must_use = "a request does nothing until it is awaited or sent"]
pub struct Plugin<C = Client> {
    client: C,
    call: Call,
}

impl<C> Plugin<C> {
    pub(crate) fn new(client: C, path: String) -> Self {
        let path = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        Plugin {
            client,
            call: Call::new(path),
        }
    }

    /// Set one query parameter, as the API names it. Setting a name again
    /// replaces it.
    pub fn param(mut self, name: &str, value: impl ToString) -> Self {
        self.call.query.set(name, value);
        self
    }

    /// A repeated query parameter: one `name=value` pair per item.
    pub fn param_each<T: ToString>(
        mut self,
        name: &str,
        values: impl IntoIterator<Item = T>,
    ) -> Self {
        self.call.query.set_each(name, values);
        self
    }
}

executable!(Plugin -> Value);
