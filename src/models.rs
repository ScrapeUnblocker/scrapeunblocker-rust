//! Return types for structured responses and the browser [`Step`] builder.

use serde::Serialize;
use serde_json::{Map, Value};

/// Structured data extracted from a page, from [`Client::get_parsed`](crate::Client::get_parsed).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ParsedPage {
    /// What the API classified the page as, e.g. `"product"`.
    pub page_type: Option<String>,
    /// How the data was extracted (Schema.org, `__NEXT_DATA__`, AI rules).
    pub source: Option<String>,
    /// The extracted fields.
    pub data: Value,
    /// The full JSON payload as returned by the API.
    pub raw: Value,
    /// `false` when the page rendered but no structured data could be
    /// extracted from it (`data` is then empty). Billed like any delivered page.
    pub data_extracted: bool,
    /// The rendered page when `data_extracted` is `false`.
    pub html: Option<String>,
    /// The API's explanation when `data_extracted` is `false`.
    pub detail: Option<String>,
}

impl ParsedPage {
    pub(crate) fn from_value(payload: Value) -> Self {
        let inner = match payload.get("data") {
            Some(v) if !v.is_null() => v.clone(),
            Some(_) => Value::Object(Map::new()),
            None => payload.clone(),
        };
        let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
        ParsedPage {
            page_type: text(&inner, "page_type"),
            source: text(&inner, "source"),
            data: inner.get("data").cloned().unwrap_or(Value::Null),
            data_extracted: payload.get("data_extracted") != Some(&Value::Bool(false)),
            html: text(&payload, "html"),
            detail: text(&payload, "detail"),
            raw: payload,
        }
    }
}

/// A page plus the cookies and proxy that served it, from
/// [`Client::get_page_with_cookies`](crate::Client::get_page_with_cookies).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PageResult {
    /// The rendered HTML.
    pub html: Option<String>,
    /// The cookies set during the visit, as the API returned them.
    pub cookies: Value,
    /// The proxy (country) the page was fetched through.
    pub proxy: Option<String>,
    /// The full JSON payload as returned by the API.
    pub raw: Value,
}

impl PageResult {
    pub(crate) fn from_value(payload: Value) -> Self {
        let first = |keys: &[&str]| {
            keys.iter()
                .filter_map(|k| payload.get(*k).and_then(Value::as_str))
                .find(|s| !s.is_empty())
                .map(str::to_owned)
        };
        PageResult {
            html: first(&["html", "page_source", "content"]),
            cookies: payload.get("cookies").cloned().unwrap_or(Value::Null),
            proxy: first(&["proxy", "proxy_address"]),
            raw: payload,
        }
    }
}

/// One browser action for the `steps` option of
/// [`Client::get_page_source`](crate::Client::get_page_source) and
/// [`Client::get_markdown`](crate::Client::get_markdown).
///
/// Steps run in order in a real browser after the page loads; the API runs a
/// request with steps once. If a step fails the call returns
/// [`ApiErrorKind::StepFailed`](crate::ApiErrorKind::StepFailed) with the step's
/// index, action, reason and the page HTML at that moment.
///
/// ```
/// use scrapeunblocker::Step;
///
/// let steps = [
///     Step::type_text("input[name=q]", "wireless headphones"),
///     Step::press_key("Enter"),
///     Step::wait_for(".search-results").timeout_ms(10_000),
/// ];
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Step(Map<String, Value>);

impl Step {
    fn new(action: &str) -> Self {
        let mut map = Map::new();
        map.insert("action".into(), action.into());
        Step(map)
    }

    fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    /// Wait until an element matching `selector` appears.
    pub fn wait_for(selector: impl Into<String>) -> Self {
        Self::new("wait_for").with("selector", selector.into())
    }

    /// Wait until `text` appears anywhere on the page.
    pub fn wait_for_text(text: impl Into<String>) -> Self {
        Self::new("wait_for_text").with("value", text.into())
    }

    /// Wait a fixed number of milliseconds.
    pub fn wait(ms: u64) -> Self {
        Self::new("wait").with("value", ms)
    }

    /// Click the element matching `selector`.
    pub fn click(selector: impl Into<String>) -> Self {
        Self::new("click").with("selector", selector.into())
    }

    /// Type `value` into the field matching `selector` (the `type` action).
    pub fn type_text(selector: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new("type")
            .with("selector", selector.into())
            .with("value", value.into())
    }

    /// Choose the option `value` in the `<select>` matching `selector`.
    pub fn select(selector: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new("select")
            .with("selector", selector.into())
            .with("value", value.into())
    }

    /// Press a key: `Enter`, `Tab`, `Escape`, `Backspace`, `Delete`, `Space`,
    /// `ArrowUp`/`Down`/`Left`/`Right`, `Home`, `End`, `PageUp` or `PageDown`.
    pub fn press_key(key: impl Into<String>) -> Self {
        Self::new("press_key").with("value", key.into())
    }

    /// Scroll to the bottom of the page.
    pub fn scroll_to_bottom() -> Self {
        Self::new("scroll").with("value", "bottom")
    }

    /// Scroll by a number of pixels (negative scrolls up).
    pub fn scroll_by(pixels: i64) -> Self {
        Self::new("scroll").with("value", pixels)
    }

    /// How `selector` is read: `css` (the default), `xPath`, `className` or `tagName`.
    pub fn selector_type(self, selector_type: impl Into<String>) -> Self {
        self.with("selector_type", selector_type.into())
    }

    /// How long this step may wait, in milliseconds.
    pub fn timeout_ms(self, ms: u64) -> Self {
        self.with("timeout_ms", ms)
    }

    /// For [`Step::type_text`]: clear the field before typing.
    pub fn clear(self, clear: bool) -> Self {
        self.with("clear", clear)
    }

    /// A step given as raw JSON, for an action this version has no helper for.
    /// Returns `None` when `value` is not a JSON object.
    pub fn from_json(value: Value) -> Option<Self> {
        match value {
            Value::Object(map) => Some(Step(map)),
            _ => None,
        }
    }
}
