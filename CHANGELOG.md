# Changelog

Versions follow the other ScrapeUnblocker SDKs (Python, Node.js, Ruby, PHP), so
the same version number means the same API surface.

## 0.22.0 (2026-11-23)

- Sticky sessions: `GetPageSource`, `GetMarkdown`, `ListElements`, `GetParsed`,
  `GetPageWithCookies`, `GetPageWithHeaders` and `GetPageWithXhr` (async and blocking)
  have a new `.session(id)` setter for the API's `session` parameter. Calls with the
  same id (1-64 letters, digits, `_`, `.`, `-`) leave through the same proxy IP while
  the session is in use (it expires after 10 minutes without calls). The parameter is
  only sent when set.
- `ParsedPage`, `PageResult`, `PageHeaders` and `CapturedPage` have a new
  `session_changed` field: `true` when the API answered with `X-Session-Changed: true`,
  i.e. the call was answered through another IP than the session's. `get_page_source`,
  `get_markdown` and `list_elements` still return a `String` / `Value`; use another
  builder when you need the flag.

No breaking changes.

## 0.21.0 (2026-11-23)

- New `get_page_with_headers(url)` (async and blocking) for the API's `get_headers`
  parameter: returns a `PageHeaders` with `html`, the target's own `status_code`,
  `final_url` after redirects, the target's response `headers` (lowercase names,
  `HeaderValues::One` or `HeaderValues::Many` - `set-cookie` is always a list),
  the `request_headers` actually sent, `proxy`, `cookies` (with `get_cookies(true)`)
  and `raw`. `PageHeaders::header(name)` looks a header up in any letter case. The
  builder takes `get_cookies`, `proxy_country` and `time_sleep`.

No breaking changes.

## 0.20.0 (2026-11-18)

- New `get_page_with_xhr(url, capture_xhr)` (async and blocking) for the API's `capture_xhr`
  parameter: the page is rendered in a real browser and every XHR/fetch response whose URL
  contains the fragment comes back as a `CapturedPage` (`html`, `xhr` - `XhrCall` with `url`,
  `method`, `status`, `content_type`, `body`, `body_omitted` and a `json()` helper -,
  `xhr_truncated`, `proxy`). The builder takes `proxy_country` and `steps`.
- `StepFailure::xhr` holds the responses the page received before a step failed, when the call
  used `capture_xhr`.

No breaking changes.

## 0.19.0

First release of the Rust client.

- Async `Client` (Tokio, reqwest) and a blocking client behind the `blocking` feature.
- `get_page_source`, `get_markdown`, `get_parsed`, `get_page_with_cookies`, `list_elements`,
  `serp` (with `full_urls`), `get_image`, browser `steps` via `Step`.
- Every public plugin as a method that returns a request builder (185 methods),
  plus `plugin(path)` for any endpoint by path.
- Typed errors: `Error::Api` with an `ApiErrorKind` for every status the API
  sends, target 404/410 details (`target_not_found`) and failed-step details
  (`step_failed`).
- Retries HTTP 429/502/503/504 and network errors with exponential backoff.
- `User-Agent: scrapeunblocker-rust/<version>`, plus your app's token via
  `ClientBuilder::user_agent`.
