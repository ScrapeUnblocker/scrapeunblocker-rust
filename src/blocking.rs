//! A blocking client, for code without an async runtime (feature `blocking`).
//!
//! It has the same endpoint methods as the async [`crate::Client`] and returns
//! the same request builders; finish a request with `.send()` instead of
//! `.await`:
//!
//! ```no_run
//! # fn run() -> scrapeunblocker::Result<()> {
//! use scrapeunblocker::blocking::Client;
//!
//! let su = Client::from_env()?;
//! let html = su.get_page_source("https://example.com").send()?;
//! let results = su.amazon_search("wireless headphones").marketplace("amazon.de").send()?;
//! # Ok(()) }
//! ```
//!
//! Like `reqwest::blocking`, it runs its own small runtime, so it must not be
//! used (or dropped) inside an async context - use the async client there.

use std::fmt;
use std::sync::Arc;

use crate::client::{
    ClientBuilder, GetImage, GetMarkdown, GetPageSource, GetPageWithCookies, GetPageWithHeaders,
    GetPageWithXhr, GetParsed, ListElements, Plugin, Serp,
};
use crate::error::{Error, Result};
use crate::request::{Call, Decode};

/// Blocking client for the ScrapeUnblocker API. Cheap to clone.
#[derive(Clone)]
pub struct Client {
    inner: crate::Client,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("blocking::Client")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// A client with the given API key and default settings.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::wrap(crate::Client::new(api_key)?)
    }

    /// A client whose API key comes from the `SCRAPEUNBLOCKER_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        Self::wrap(crate::Client::from_env()?)
    }

    /// A blocking client with the settings of a [`ClientBuilder`].
    pub fn from_builder(builder: ClientBuilder) -> Result<Self> {
        Self::wrap(builder.build()?)
    }

    fn wrap(inner: crate::Client) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Config(format!("could not start the blocking runtime: {e}")))?;
        Ok(Client {
            inner,
            runtime: Arc::new(runtime),
        })
    }

    pub(crate) fn execute<T: Decode>(&self, call: Call) -> Result<T> {
        self.runtime.block_on(self.inner.execute(call))
    }

    /// Fetch a URL and return the fully rendered HTML. See [`crate::Client::get_page_source`].
    pub fn get_page_source(&self, url: impl Into<String>) -> GetPageSource<Client> {
        GetPageSource::new(self.clone(), url.into())
    }

    /// List the page's interactive elements. See [`crate::Client::list_elements`].
    pub fn list_elements(&self, url: impl Into<String>) -> ListElements<Client> {
        ListElements::new(self.clone(), url.into())
    }

    /// Fetch a URL as clean Markdown. See [`crate::Client::get_markdown`].
    pub fn get_markdown(&self, url: impl Into<String>) -> GetMarkdown<Client> {
        GetMarkdown::new(self.clone(), url.into())
    }

    /// Fetch a URL as structured JSON. See [`crate::Client::get_parsed`].
    pub fn get_parsed(&self, url: impl Into<String>) -> GetParsed<Client> {
        GetParsed::new(self.clone(), url.into())
    }

    /// Fetch a URL with its cookies and proxy. See [`crate::Client::get_page_with_cookies`].
    pub fn get_page_with_cookies(&self, url: impl Into<String>) -> GetPageWithCookies<Client> {
        GetPageWithCookies::new(self.clone(), url.into())
    }

    /// Fetch a URL with the target's status, final URL and headers. See
    /// [`crate::Client::get_page_with_headers`].
    pub fn get_page_with_headers(&self, url: impl Into<String>) -> GetPageWithHeaders<Client> {
        GetPageWithHeaders::new(self.clone(), url.into())
    }

    /// Fetch a URL with the page's own XHR/fetch responses. See
    /// [`crate::Client::get_page_with_xhr`].
    pub fn get_page_with_xhr(
        &self,
        url: impl Into<String>,
        capture_xhr: impl Into<String>,
    ) -> GetPageWithXhr<Client> {
        GetPageWithXhr::new(self.clone(), url.into(), capture_xhr.into())
    }

    /// Run a Google search. See [`crate::Client::serp`].
    pub fn serp(&self, keyword: impl Into<String>) -> Serp<Client> {
        Serp::new(self.clone(), keyword.into())
    }

    /// Fetch an image's bytes. See [`crate::Client::get_image`].
    pub fn get_image(&self, url: impl Into<String>) -> GetImage<Client> {
        GetImage::new(self.clone(), url.into())
    }

    /// Call any plugin endpoint by path. See [`crate::Client::plugin`].
    pub fn plugin(&self, path: impl Into<String>) -> Plugin<Client> {
        Plugin::new(self.clone(), path.into())
    }
}
