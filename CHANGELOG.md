# Changelog

Versions follow the other ScrapeUnblocker SDKs (Python, Node.js, Ruby, PHP), so
the same version number means the same API surface.

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
