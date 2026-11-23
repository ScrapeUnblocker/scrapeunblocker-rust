//! Errors returned by the client.
//!
//! Every fallible call returns [`Error`]. An error response from the API is
//! [`Error::Api`], whose [`ApiError::kind`] says what went wrong, so a hard block
//! can be handled differently from a transient upstream outage without
//! matching on status codes by hand:
//!
//! ```no_run
//! # async fn run(su: scrapeunblocker::Client) {
//! use scrapeunblocker::{ApiErrorKind, Error};
//!
//! match su.get_page_source("https://example.com").await {
//!     Ok(html) => println!("{} bytes", html.len()),
//!     Err(Error::Api(e)) if e.kind == ApiErrorKind::Blocked => println!("blocked, not billed"),
//!     Err(Error::Api(e)) if e.kind.is_payment_required() => println!("billing: {e}"),
//!     Err(e) => println!("other error: {e}"),
//! }
//! # }
//! ```

use std::fmt;

use crate::models::{xhr_calls, XhrCall};

/// A `Result` whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong in a call.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No API key was given and `SCRAPEUNBLOCKER_KEY` is not set.
    #[error(
        "no API key provided: pass one to Client::new or set the SCRAPEUNBLOCKER_KEY \
         environment variable (get your key at https://app.scrapeunblocker.com)"
    )]
    MissingApiKey,

    /// The API answered with an error status.
    #[error(transparent)]
    Api(Box<ApiError>),

    /// The request did not complete within the client's timeout.
    ///
    /// Distinct from [`ApiErrorKind::BrowserTimeout`], where the API answered
    /// that its own browser run ran out of time.
    #[error("request timed out: {0}")]
    Timeout(#[source] reqwest::Error),

    /// The client could not reach the API (DNS, connection refused, TLS).
    #[error("could not reach the ScrapeUnblocker API: {0}")]
    Connection(#[source] reqwest::Error),

    /// The response body was not what the call expects (e.g. not JSON).
    #[error("could not decode the API response: {0}")]
    Decode(String),

    /// The client could not be built (invalid base URL, header value or TLS setup).
    #[error("invalid client configuration: {0}")]
    Config(String),
}

impl Error {
    /// The API error, when the API answered with an error status.
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Error::Api(e) => Some(e),
            _ => None,
        }
    }

    /// The HTTP status code, when the API answered with an error status.
    pub fn status(&self) -> Option<u16> {
        self.api().map(|e| e.status)
    }
}

impl From<ApiError> for Error {
    fn from(e: ApiError) -> Self {
        Error::Api(Box::new(e))
    }
}

/// What kind of error response the API sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ApiErrorKind {
    /// HTTP 400: a malformed URL or unsupported scheme, a missing API key
    /// header, or a URL that belongs to a dedicated plugin (the message names
    /// the endpoint to use instead).
    InvalidRequest,
    /// HTTP 401: the key is not recognised (a typo, a truncated copy, trailing
    /// whitespace, a rotated key). Not billed.
    Authentication,
    /// HTTP 401: the key is valid, but the account has no active plan. Pick a
    /// plan at <https://app.scrapeunblocker.com>; the key does not change.
    NoSubscription,
    /// HTTP 402 with a billing reason this version does not recognise.
    PaymentRequired,
    /// HTTP 402: every request the plan allows this period has been used.
    QuotaExceeded,
    /// HTTP 402: the unpaid balance has passed the account's credit limit.
    CreditLimitExceeded,
    /// HTTP 402: the account reached the monthly budget limit you set.
    BudgetExceeded,
    /// HTTP 402: a card payment has been declined three times.
    PaymentFailed,
    /// HTTP 403: the target blocked every available bypass path. Not billed.
    Blocked,
    /// HTTP 404: something the call asked for does not exist (a plugin lookup,
    /// a missing element).
    NotFound,
    /// HTTP 404 or 410 from the target page itself. Its final answer, not a
    /// block, so it is never retried, and the call is billed. Details are in
    /// [`ApiError::target_not_found`].
    TargetNotFound,
    /// HTTP 408: the API's browser run did not finish in time. Retrying usually helps.
    BrowserTimeout,
    /// HTTP 415: the URL serves a content type the call cannot return (a page
    /// URL passed to `get_image`, an image passed to `get_page_source`).
    UnsupportedContent,
    /// HTTP 422: a parameter is missing or has the wrong type; the JSON body's
    /// `detail` array names each problem field.
    Validation,
    /// HTTP 422: a browser step in `steps` could not run. Details are in
    /// [`ApiError::step_failed`].
    StepFailed,
    /// HTTP 429: too many concurrent requests on your key.
    RateLimit,
    /// HTTP 503: the target site returned a server-side outage page.
    UpstreamOutage,
    /// Any other HTTP 5xx.
    Server,
    /// Any other error status.
    Other,
}

impl ApiErrorKind {
    /// True for the two 401 kinds.
    pub fn is_authentication(self) -> bool {
        matches!(self, Self::Authentication | Self::NoSubscription)
    }

    /// True for every 402 kind. A billing block lifts once the billing state
    /// changes (usually within a minute); retrying before that is pointless.
    pub fn is_payment_required(self) -> bool {
        matches!(
            self,
            Self::PaymentRequired
                | Self::QuotaExceeded
                | Self::CreditLimitExceeded
                | Self::BudgetExceeded
                | Self::PaymentFailed
        )
    }

    /// True for both 404 kinds.
    pub fn is_not_found(self) -> bool {
        matches!(self, Self::NotFound | Self::TargetNotFound)
    }

    /// True for every 422 kind.
    pub fn is_validation(self) -> bool {
        matches!(self, Self::Validation | Self::StepFailed)
    }
}

/// An error response from the API.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ApiError {
    /// What kind of error this is.
    pub kind: ApiErrorKind,
    /// The HTTP status code.
    pub status: u16,
    /// A readable explanation, with the start of the response body.
    pub message: String,
    /// The raw response body, useful for debugging.
    pub body: Option<String>,
    /// Set for [`ApiErrorKind::TargetNotFound`].
    pub target_not_found: Option<TargetNotFound>,
    /// Set for [`ApiErrorKind::StepFailed`].
    pub step_failed: Option<StepFailure>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// Details of a target page that answered 404 or 410 on its own.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TargetNotFound {
    /// The status the target answered with (404 or 410).
    pub origin_status: u16,
    /// The target's own not-found page as served (can be empty). When the call
    /// answers JSON (parsed data, cookies, headers), the `html` field of that
    /// JSON, or `None` when it has none.
    pub html: Option<String>,
    /// The URL the target answered for, when the API sent it.
    pub destination_url: Option<String>,
}

/// Details of a browser step that could not run.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StepFailure {
    /// Zero-based index of the step that failed.
    pub step_index: Option<u64>,
    /// The failed step's action, e.g. `"click"`.
    pub action: Option<String>,
    /// Why it failed.
    pub reason: Option<String>,
    /// The selector the step acted on, if any.
    pub selector: Option<String>,
    /// The page HTML at the moment the step failed.
    pub html: Option<String>,
    /// With [`Client::get_page_with_xhr`](crate::Client::get_page_with_xhr):
    /// the responses the page received before the step failed; otherwise `None`.
    pub xhr: Option<Vec<XhrCall>>,
}

/// Map an error response to an [`ApiError`].
pub(crate) fn api_error(
    status: u16,
    body: Option<String>,
    origin_status: Option<&str>,
    destination_url: Option<String>,
) -> ApiError {
    let text = body.as_deref().unwrap_or("");
    let json: Option<serde_json::Value> = serde_json::from_str(text).ok();
    let field =
        |name: &str| -> Option<String> { json.as_ref()?.get(name)?.as_str().map(str::to_owned) };

    if matches!(status, 404 | 410) {
        if let Some(origin) = origin_status {
            let origin_status = origin.trim().parse().unwrap_or(status);
            // A parsed-data call answers JSON; its `html` field (if any) is the page.
            let html = match &json {
                Some(serde_json::Value::Object(_)) => field("html"),
                _ => body.clone(),
            };
            return ApiError {
                kind: ApiErrorKind::TargetNotFound,
                status,
                message: format!(
                    "Target page does not exist (HTTP {origin_status}). This is the target's \
                     own answer, not a block; the call is billed."
                ),
                body,
                target_not_found: Some(TargetNotFound {
                    origin_status,
                    html,
                    destination_url,
                }),
                step_failed: None,
            };
        }
    }

    let lower = text.to_lowercase();
    let kind = match status {
        400 => ApiErrorKind::InvalidRequest,
        401 if lower.contains("no valid subscription") => ApiErrorKind::NoSubscription,
        401 => ApiErrorKind::Authentication,
        402 if lower.contains("quota exceeded") => ApiErrorKind::QuotaExceeded,
        402 if lower.contains("credit limit exceeded") => ApiErrorKind::CreditLimitExceeded,
        402 if lower.contains("user set budget exceeded") => ApiErrorKind::BudgetExceeded,
        402 if lower.contains("payment failed") => ApiErrorKind::PaymentFailed,
        402 => ApiErrorKind::PaymentRequired,
        403 => ApiErrorKind::Blocked,
        404 => ApiErrorKind::NotFound,
        408 => ApiErrorKind::BrowserTimeout,
        415 => ApiErrorKind::UnsupportedContent,
        422 if field("error").as_deref() == Some("step_failed") => ApiErrorKind::StepFailed,
        422 => ApiErrorKind::Validation,
        429 => ApiErrorKind::RateLimit,
        503 => ApiErrorKind::UpstreamOutage,
        500..=599 => ApiErrorKind::Server,
        _ => ApiErrorKind::Other,
    };

    if kind == ApiErrorKind::StepFailed {
        let num = |name: &str| json.as_ref()?.get(name)?.as_u64();
        let step = StepFailure {
            step_index: num("step_index"),
            action: field("action"),
            reason: field("reason"),
            selector: field("selector"),
            html: field("html"),
            xhr: xhr_calls(json.as_ref().and_then(|j| j.get("xhr"))),
        };
        let message = format!(
            "Browser step {} ({}) failed: {}",
            step.step_index
                .map_or_else(|| "?".to_owned(), |i| i.to_string()),
            step.action.as_deref().unwrap_or("?"),
            step.reason.as_deref().unwrap_or("unknown reason"),
        );
        return ApiError {
            kind,
            status,
            message,
            body,
            target_not_found: None,
            step_failed: Some(step),
        };
    }

    ApiError {
        kind,
        status,
        message: message_for(status, text),
        body,
        target_not_found: None,
        step_failed: None,
    }
}

fn message_for(status: u16, body: &str) -> String {
    let base = match status {
        400 => "Invalid request (bad URL, unsupported scheme, or missing API key header)",
        401 => "Authentication failed - key not recognised, or account has no active plan",
        402 => {
            "Billing block - quota exceeded, credit limit exceeded, monthly budget limit \
             reached, or a failed payment"
        }
        403 => "Target blocked by bot protection on every bypass path",
        404 => "Requested element not found on the page",
        408 => "Browser run timed out before the page was ready",
        415 => "URL serves a content type this call cannot return",
        422 => "Validation error - see the detail array in the response body",
        429 => "Rate limited - too many concurrent requests",
        503 => "Upstream origin returned a server-side outage page",
        504 => "Fetch timed out upstream",
        _ => "",
    };
    let base = if base.is_empty() {
        format!("API returned HTTP {status}")
    } else {
        base.to_owned()
    };
    let snippet: String = body.trim().replace('\n', " ");
    if snippet.is_empty() {
        return base;
    }
    let short: String = snippet.chars().take(200).collect();
    let ellipsis = if snippet.chars().count() > 200 {
        "..."
    } else {
        ""
    };
    format!("{base}: {short}{ellipsis}")
}
