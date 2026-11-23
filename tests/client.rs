use std::time::Duration;

use scrapeunblocker::{ApiErrorKind, Client, Error, HeaderValues, Step};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "test-key";

async fn client(server: &MockServer) -> Client {
    Client::builder()
        .api_key(KEY)
        .base_url(server.uri())
        .max_retries(0)
        .build()
        .unwrap()
}

fn query_values(req: &Request, key: &str) -> Vec<String> {
    req.url
        .query_pairs()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .collect()
}

#[tokio::test]
async fn get_page_source_sends_key_user_agent_and_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/getPageSource"))
        .and(header("x-scrapeunblocker-key", KEY))
        .and(query_param("url", "https://example.com/?a=1&b=2"))
        .and(query_param("proxy_country", "de"))
        .and(query_param("time_sleep", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>ok</html>"))
        .expect(1)
        .mount(&server)
        .await;

    let su = client(&server).await;
    let html = su
        .get_page_source("https://example.com/?a=1&b=2")
        .proxy_country("de")
        .time_sleep(2)
        .await
        .unwrap();
    assert_eq!(html, "<html>ok</html>");

    let req = &server.received_requests().await.unwrap()[0];
    let ua = req.headers.get("user-agent").unwrap().to_str().unwrap();
    assert_eq!(
        ua,
        format!("scrapeunblocker-rust/{}", scrapeunblocker::VERSION)
    );
}

#[tokio::test]
async fn app_token_follows_the_sdk_token_in_user_agent() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x"))
        .mount(&server)
        .await;
    let su = Client::builder()
        .api_key(KEY)
        .base_url(server.uri())
        .user_agent("my-crawler/1.4\n")
        .build()
        .unwrap();
    su.get_page_source("https://example.com").await.unwrap();
    let req = &server.received_requests().await.unwrap()[0];
    let ua = req.headers.get("user-agent").unwrap().to_str().unwrap();
    assert_eq!(
        ua,
        format!(
            "scrapeunblocker-rust/{} my-crawler/1.4",
            scrapeunblocker::VERSION
        )
    );
}

#[tokio::test]
async fn steps_are_sent_as_one_json_array() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_body_string("done"))
        .mount(&server)
        .await;
    let su = client(&server).await;
    su.get_page_source("https://example.com")
        .steps([
            Step::type_text("input[name=q]", "headphones").clear(true),
            Step::press_key("Enter"),
            Step::wait_for(".results").timeout_ms(10_000),
            Step::scroll_to_bottom(),
        ])
        .await
        .unwrap();
    let req = &server.received_requests().await.unwrap()[0];
    let steps: serde_json::Value = serde_json::from_str(&query_values(req, "steps")[0]).unwrap();
    assert_eq!(
        steps,
        json!([
            {"action": "type", "selector": "input[name=q]", "value": "headphones", "clear": true},
            {"action": "press_key", "value": "Enter"},
            {"action": "wait_for", "selector": ".results", "timeout_ms": 10000},
            {"action": "scroll", "value": "bottom"},
        ])
    );
}

#[tokio::test]
async fn markdown_and_list_elements_set_their_flags() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .and(query_param("markdown", "true"))
        .and(query_param("main_content_only", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_string("# Title"))
        .mount(&server)
        .await;
    Mock::given(path("/getPageSource"))
        .and(query_param("list_elements", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"count": 1, "elements": [{"selector": "#a"}]})),
        )
        .mount(&server)
        .await;
    let su = client(&server).await;
    let md = su
        .get_markdown("https://example.com")
        .main_content_only(false)
        .await
        .unwrap();
    assert_eq!(md, "# Title");
    let elements = su.list_elements("https://example.com").await.unwrap();
    assert_eq!(elements["elements"][0]["selector"], "#a");
}

#[tokio::test]
async fn get_parsed_decodes_the_page_and_an_empty_parse() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .and(query_param("parsed_data", "true"))
        .and(query_param("url", "https://shop.example/p/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"page_type": "product", "source": "schema.org", "data": {"title": "Kettle", "price": 19.9}}
        })))
        .mount(&server)
        .await;
    Mock::given(path("/getPageSource"))
        .and(query_param("url", "https://shop.example/empty"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"page_type": "unknown", "data": {}},
            "data_extracted": false,
            "html": "<html></html>",
            "detail": "nothing found"
        })))
        .mount(&server)
        .await;
    let su = client(&server).await;

    let page = su.get_parsed("https://shop.example/p/1").await.unwrap();
    assert_eq!(page.page_type.as_deref(), Some("product"));
    assert_eq!(page.source.as_deref(), Some("schema.org"));
    assert_eq!(page.data["title"], "Kettle");
    assert!(page.data_extracted);
    assert_eq!(page.html, None);

    let empty = su.get_parsed("https://shop.example/empty").await.unwrap();
    assert!(!empty.data_extracted);
    assert_eq!(empty.html.as_deref(), Some("<html></html>"));
    assert_eq!(empty.detail.as_deref(), Some("nothing found"));
}

#[tokio::test]
async fn cookies_serp_image_and_plugin() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .and(query_param("get_cookies", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>hi</p>", "cookies": [{"name": "sid"}], "proxy": "us"
        })))
        .mount(&server)
        .await;
    Mock::given(path("/serpApi"))
        .and(query_param("keyword", "rust sdk"))
        .and(query_param("pages_to_check", "2"))
        .and(query_param("full_urls", "true"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"organic": [{"position": 1}]})),
        )
        .mount(&server)
        .await;
    Mock::given(path("/getImage"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x89, b'P', b'N', b'G']))
        .mount(&server)
        .await;
    Mock::given(path("/marketplace/target-search"))
        .and(query_param("query", "tv"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": []})))
        .mount(&server)
        .await;
    let su = client(&server).await;

    let page = su
        .get_page_with_cookies("https://example.com")
        .await
        .unwrap();
    assert_eq!(page.html.as_deref(), Some("<p>hi</p>"));
    assert_eq!(page.proxy.as_deref(), Some("us"));
    assert_eq!(page.cookies[0]["name"], "sid");

    let serp = su
        .serp("rust sdk")
        .pages_to_check(2)
        .full_urls(true)
        .await
        .unwrap();
    assert_eq!(serp["organic"][0]["position"], 1);

    let image = su.get_image("https://example.com/a.png").await.unwrap();
    assert_eq!(image, vec![0x89, b'P', b'N', b'G']);

    let data = su
        .plugin("marketplace/target-search")
        .param("query", "tv")
        .param("page", 2)
        .send()
        .await
        .unwrap();
    assert_eq!(data, json!({"results": []}));
}

#[tokio::test]
async fn generated_methods_build_the_right_query() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    let su = client(&server).await;

    // A list joined into one value.
    su.google_trends(["coffee", "tea"]).geo("US").await.unwrap();
    // A repeated parameter.
    su.trustpilot_reviews("www.nike.com")
        .stars([4, 5])
        .await
        .unwrap();
    // A fixed parameter the method sets itself.
    su.woolworths_product("888137").await.unwrap();
    // A namespaced method and a parameter named after a Rust keyword.
    su.skyscanner_flights()
        .origin("London")
        .adults(2)
        .await
        .unwrap();
    su.youtube_search("rust").type_("video").await.unwrap();
    // Options left unset are not sent; setting one twice keeps the last value.
    su.amazon_search("tv")
        .marketplace("amazon.com")
        .marketplace("amazon.de")
        .await
        .unwrap();

    let reqs = server.received_requests().await.unwrap();
    let paths: Vec<&str> = reqs.iter().map(|r| r.url.path()).collect();
    assert_eq!(
        paths,
        [
            "/trends/google-trends",
            "/reviews/trustpilot",
            "/groceries/woolworths",
            "/flights/skyscanner-quotes",
            "/content/youtube-search",
            "/marketplace/amazon-search",
        ]
    );
    assert_eq!(query_values(&reqs[0], "keywords"), ["coffee,tea"]);
    assert_eq!(query_values(&reqs[1], "stars"), ["4", "5"]);
    assert_eq!(query_values(&reqs[2], "mode"), ["product"]);
    assert_eq!(query_values(&reqs[3], "origin"), ["London"]);
    assert_eq!(query_values(&reqs[3], "adults"), ["2"]);
    assert_eq!(query_values(&reqs[4], "type"), ["video"]);
    assert_eq!(query_values(&reqs[5], "marketplace"), ["amazon.de"]);
    assert!(query_values(&reqs[5], "page").is_empty());
}

#[tokio::test]
async fn unset_options_are_not_sent() {
    let server = MockServer::start().await;
    Mock::given(path("/serpApi"))
        .and(query_param_is_missing("pages_to_check"))
        .and(query_param_is_missing("proxy_country"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    client(&server).await.serp("q").await.unwrap();
}

async fn error_for(status: u16, body: &str, headers: &[(&str, &str)]) -> Error {
    let server = MockServer::start().await;
    let mut response = ResponseTemplate::new(status).set_body_string(body);
    for (k, v) in headers {
        response = response.insert_header(*k, *v);
    }
    Mock::given(path("/getPageSource"))
        .respond_with(response)
        .mount(&server)
        .await;
    client(&server)
        .await
        .get_page_source("https://example.com")
        .await
        .unwrap_err()
}

async fn kind_for(status: u16, body: &str) -> ApiErrorKind {
    error_for(status, body, &[]).await.api().unwrap().kind
}

#[tokio::test]
async fn error_statuses_map_to_kinds() {
    assert_eq!(
        kind_for(400, "Missing x-scrapeunblocker-key").await,
        ApiErrorKind::InvalidRequest
    );
    assert_eq!(
        kind_for(401, "Unauthorized").await,
        ApiErrorKind::Authentication
    );
    assert_eq!(
        kind_for(401, "No valid subscription").await,
        ApiErrorKind::NoSubscription
    );
    assert_eq!(
        kind_for(402, "Quota exceeded").await,
        ApiErrorKind::QuotaExceeded
    );
    assert_eq!(
        kind_for(402, "Credit limit exceeded").await,
        ApiErrorKind::CreditLimitExceeded
    );
    assert_eq!(
        kind_for(402, "User set budget exceeded").await,
        ApiErrorKind::BudgetExceeded
    );
    assert_eq!(
        kind_for(402, "Payment failed - update payment method").await,
        ApiErrorKind::PaymentFailed
    );
    assert_eq!(
        kind_for(402, "something new").await,
        ApiErrorKind::PaymentRequired
    );
    assert_eq!(kind_for(403, "blocked").await, ApiErrorKind::Blocked);
    assert_eq!(
        kind_for(404, "no such element").await,
        ApiErrorKind::NotFound
    );
    assert_eq!(kind_for(408, "").await, ApiErrorKind::BrowserTimeout);
    assert_eq!(
        kind_for(415, "image/png").await,
        ApiErrorKind::UnsupportedContent
    );
    assert_eq!(
        kind_for(422, r#"{"detail": []}"#).await,
        ApiErrorKind::Validation
    );
    assert_eq!(kind_for(429, "").await, ApiErrorKind::RateLimit);
    assert_eq!(kind_for(503, "").await, ApiErrorKind::UpstreamOutage);
    assert_eq!(kind_for(500, "").await, ApiErrorKind::Server);
    assert_eq!(kind_for(504, "").await, ApiErrorKind::Server);
    assert!(ApiErrorKind::QuotaExceeded.is_payment_required());
    assert!(ApiErrorKind::NoSubscription.is_authentication());
    assert!(ApiErrorKind::TargetNotFound.is_not_found());
    assert!(ApiErrorKind::StepFailed.is_validation());

    let err = error_for(403, "Blocked on every path", &[]).await;
    assert_eq!(err.status(), Some(403));
    assert!(err.to_string().contains("Blocked on every path"));
}

#[tokio::test]
async fn target_not_found_carries_the_targets_page() {
    let err = error_for(
        404,
        "<html>gone</html>",
        &[
            ("x-origin-status", "410"),
            ("x-destination-url", "https://example.com/x"),
        ],
    )
    .await;
    let api = err.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::TargetNotFound);
    let target = api.target_not_found.as_ref().unwrap();
    assert_eq!(target.origin_status, 410);
    assert_eq!(target.html.as_deref(), Some("<html>gone</html>"));
    assert_eq!(
        target.destination_url.as_deref(),
        Some("https://example.com/x")
    );
}

#[tokio::test]
async fn step_failure_carries_the_step() {
    let body = json!({
        "error": "step_failed", "step_index": 2, "action": "click",
        "reason": "selector not found", "selector": "#more", "html": "<html/>"
    })
    .to_string();
    let err = error_for(422, &body, &[]).await;
    let api = err.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::StepFailed);
    let step = api.step_failed.as_ref().unwrap();
    assert_eq!(step.step_index, Some(2));
    assert_eq!(step.action.as_deref(), Some("click"));
    assert_eq!(step.selector.as_deref(), Some("#more"));
    assert_eq!(
        api.message,
        "Browser step 2 (click) failed: selector not found"
    );
}

#[tokio::test]
async fn transient_errors_are_retried() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_body_string("second try"))
        .mount(&server)
        .await;
    let su = Client::builder()
        .api_key(KEY)
        .base_url(server.uri())
        .max_retries(1)
        .build()
        .unwrap();
    let html = su.get_page_source("https://example.com").await.unwrap();
    assert_eq!(html, "second try");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn deterministic_errors_are_not_retried() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(402).set_body_string("Quota exceeded"))
        .expect(1)
        .mount(&server)
        .await;
    let su = Client::builder()
        .api_key(KEY)
        .base_url(server.uri())
        .max_retries(3)
        .build()
        .unwrap();
    let err = su.get_page_source("https://example.com").await.unwrap_err();
    assert_eq!(err.api().unwrap().kind, ApiErrorKind::QuotaExceeded);
}

#[tokio::test]
async fn a_slow_answer_is_a_timeout() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let su = Client::builder()
        .api_key(KEY)
        .base_url(server.uri())
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let err = su.get_page_source("https://example.com").await.unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err:?}");
}

#[test]
fn a_blank_key_is_missing() {
    let err = Client::builder().api_key("   ").build().unwrap_err();
    assert!(matches!(err, Error::MissingApiKey));
}

#[test]
fn the_key_is_not_in_debug_output() {
    let su = Client::new("secret-key-123").unwrap();
    assert!(!format!("{su:?}").contains("secret-key-123"));
}

#[tokio::test]
async fn requests_can_be_spawned() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_body_string("spawned"))
        .mount(&server)
        .await;
    let su = client(&server).await;
    let task = tokio::spawn(su.get_page_source("https://example.com").send());
    assert_eq!(task.await.unwrap().unwrap(), "spawned");
}

#[cfg(feature = "blocking")]
#[test]
fn blocking_client_sends_the_same_request() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/marketplace/amazon-search"))
            .and(query_param("keyword", "tv"))
            .and(query_param("marketplace", "amazon.de"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": [1]})))
            .mount(&server)
            .await;
        Mock::given(path("/getPageSource"))
            .and(query_param("get_headers", "true"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"html": "h", "status_code": 200})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/getPageSource"))
            .respond_with(ResponseTemplate::new(200).set_body_string("blocking html"))
            .mount(&server)
            .await;
        server
    });
    let su = scrapeunblocker::blocking::Client::from_builder(
        Client::builder().api_key(KEY).base_url(server.uri()),
    )
    .unwrap();
    assert_eq!(
        su.get_page_source("https://example.com").send().unwrap(),
        "blocking html"
    );
    let data = su
        .amazon_search("tv")
        .marketplace("amazon.de")
        .send()
        .unwrap();
    assert_eq!(data["results"][0], 1);
    let page = su
        .get_page_with_headers("https://example.com")
        .send()
        .unwrap();
    assert_eq!(page.status_code, Some(200));
}

#[tokio::test]
async fn get_page_with_xhr_returns_the_captured_calls() {
    let server = MockServer::start().await;
    let payload = json!({
        "html": "<html>results</html>",
        "xhr": [
            {"url": "https://api.example.com/v2/items?page=1", "method": "GET", "status": 200,
             "content_type": "application/json", "body": "{\"items\":[1,2]}"},
            {"url": "https://api.example.com/v2/items?page=2", "method": "GET", "status": 403,
             "content_type": null, "body": null, "body_omitted": "binary"}
        ],
        "xhr_truncated": false,
        "proxy": "us"
    });
    Mock::given(method("POST"))
        .and(path("/getPageSource"))
        .and(query_param("capture_xhr", "/v2/items"))
        .and(query_param("proxy_country", "us"))
        .and(query_param_is_missing("get_cookies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
        .expect(1)
        .mount(&server)
        .await;

    let su = client(&server).await;
    let page = su
        .get_page_with_xhr("https://example.com/search", "/v2/items")
        .proxy_country("us")
        .steps([Step::click("#more")])
        .await
        .unwrap();
    assert_eq!(page.html.as_deref(), Some("<html>results</html>"));
    assert_eq!(page.proxy.as_deref(), Some("us"));
    assert!(!page.xhr_truncated);
    assert_eq!(page.xhr.len(), 2);
    assert_eq!(page.xhr[0].status, 200);
    assert_eq!(page.xhr[0].json().unwrap(), json!({"items": [1, 2]}));
    assert_eq!(page.xhr[1].body, None);
    assert_eq!(page.xhr[1].body_omitted.as_deref(), Some("binary"));
    assert!(page.xhr[1].json().is_err());

    let req = &server.received_requests().await.unwrap()[0];
    let steps: serde_json::Value = serde_json::from_str(&query_values(req, "steps")[0]).unwrap();
    assert_eq!(steps, json!([{"action": "click", "selector": "#more"}]));
}

#[tokio::test]
async fn step_failure_with_capture_carries_the_calls() {
    let body = json!({
        "error": "step_failed", "step_index": 0, "action": "click", "reason": "gone",
        "xhr": [{"url": "https://api.example.com/v2/items", "method": "GET", "status": 200,
                 "body": "{}"}]
    })
    .to_string();
    let err = error_for(422, &body, &[]).await;
    let step = err.api().unwrap().step_failed.as_ref().unwrap();
    let calls = step.xhr.as_ref().unwrap();
    assert_eq!(calls[0].url, "https://api.example.com/v2/items");

    let plain = json!({"error": "step_failed", "step_index": 0, "action": "click"}).to_string();
    let err = error_for(422, &plain, &[]).await;
    assert!(err
        .api()
        .unwrap()
        .step_failed
        .as_ref()
        .unwrap()
        .xhr
        .is_none());
}

#[tokio::test]
async fn get_page_with_headers_returns_status_url_and_headers() {
    let server = MockServer::start().await;
    let payload = json!({
        "html": "<!DOCTYPE html><html>ok</html>",
        "status_code": 200,
        "final_url": "https://www.example.com/",
        "headers": {
            "content-type": "text/html",
            "set-cookie": ["a=1; Path=/", "b=2"],
            "vary": "Accept, Cookie"
        },
        "request_headers": {"user-agent": "Mozilla/5.0", "accept": "text/html"},
        "proxy": "us"
    });
    Mock::given(method("POST"))
        .and(path("/getPageSource"))
        .and(query_param("url", "https://example.com/"))
        .and(query_param("get_headers", "true"))
        .and(query_param("proxy_country", "us"))
        .and(query_param("time_sleep", "1"))
        .and(query_param_is_missing("get_cookies"))
        .and(query_param_is_missing("parsed_data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
        .expect(1)
        .mount(&server)
        .await;

    let su = client(&server).await;
    let page = su
        .get_page_with_headers("https://example.com/")
        .proxy_country("us")
        .time_sleep(1)
        .get_cookies(true)
        .get_cookies(false)
        .await
        .unwrap();
    assert_eq!(page.html.as_deref(), Some("<!DOCTYPE html><html>ok</html>"));
    assert_eq!(page.status_code, Some(200));
    assert_eq!(page.final_url.as_deref(), Some("https://www.example.com/"));
    assert_eq!(page.proxy.as_deref(), Some("us"));
    assert_eq!(page.cookies, None);
    assert_eq!(page.raw, payload);

    let content_type = page.header("Content-Type").unwrap();
    assert_eq!(content_type, &HeaderValues::One("text/html".to_owned()));
    assert_eq!(content_type.as_str(), Some("text/html"));
    assert_eq!(page.header("vary").unwrap().values(), ["Accept, Cookie"]);
    let cookies = page.header("set-cookie").unwrap();
    assert_eq!(cookies.as_str(), None);
    assert_eq!(cookies.values(), ["a=1; Path=/", "b=2"]);
    assert!(page.header("x-missing").is_none());

    let sent = page.request_headers.as_ref().unwrap();
    assert_eq!(sent["user-agent"], "Mozilla/5.0");
    assert_eq!(sent.len(), 2);
}

#[tokio::test]
async fn get_page_with_headers_with_cookies_and_null_fields() {
    let server = MockServer::start().await;
    // A target's own 401 page: HTTP 200 from the API, the target's status in the JSON.
    Mock::given(path("/getPageSource"))
        .and(query_param("get_headers", "true"))
        .and(query_param("get_cookies", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-origin-status", "401")
                .set_body_json(json!({
                    "html": "<h1>Unauthorized</h1>",
                    "cookies": [{"name": "sid", "value": "1"}],
                    "status_code": 401,
                    "final_url": null,
                    "headers": null,
                    "request_headers": null,
                    "proxy": null
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let su = client(&server).await;
    let page = su
        .get_page_with_headers("https://example.com/private")
        .get_cookies(true)
        .await
        .unwrap();
    assert_eq!(page.status_code, Some(401));
    assert_eq!(page.html.as_deref(), Some("<h1>Unauthorized</h1>"));
    assert_eq!(page.cookies.as_ref().unwrap()[0]["name"], "sid");
    assert_eq!(page.final_url, None);
    assert_eq!(page.headers, None);
    assert_eq!(page.request_headers, None);
    assert_eq!(page.proxy, None);
    assert!(page.header("content-type").is_none());
}

#[tokio::test]
async fn get_page_with_headers_target_not_found_keeps_the_json() {
    let server = MockServer::start().await;
    let body = json!({
        "html": "<html>gone</html>", "status_code": 410,
        "final_url": "https://example.com/x", "headers": {"server": "nginx"},
        "request_headers": null, "proxy": "de"
    });
    Mock::given(path("/getPageSource"))
        .and(query_param("get_headers", "true"))
        .respond_with(
            ResponseTemplate::new(410)
                .insert_header("x-origin-status", "410")
                .insert_header("x-destination-url", "https://example.com/x")
                .set_body_json(&body),
        )
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .get_page_with_headers("https://example.com/x")
        .await
        .unwrap_err();
    let api = err.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::TargetNotFound);
    let target = api.target_not_found.as_ref().unwrap();
    assert_eq!(target.origin_status, 410);
    assert_eq!(target.html.as_deref(), Some("<html>gone</html>"));
    let json: serde_json::Value = serde_json::from_str(api.body.as_deref().unwrap()).unwrap();
    assert_eq!(json["headers"]["server"], "nginx");
}

/// A body every getPageSource request builder can decode.
fn session_page() -> serde_json::Value {
    json!({
        "html": "<html>ok</html>",
        "data": {"page_type": "product", "source": "schema.org", "data": {}},
        "cookies": [],
        "xhr": [],
        "status_code": 200,
        "proxy": "us"
    })
}

#[tokio::test]
async fn session_is_sent_only_when_given() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_page()))
        .mount(&server)
        .await;
    let su = client(&server).await;
    let url = "https://example.com/";

    for id in [Some("cart-42.a_b"), None] {
        match id {
            Some(id) => {
                su.get_page_source(url).session(id).await.unwrap();
                su.list_elements(url).session(id).await.unwrap();
                su.get_markdown(url).session(id).await.unwrap();
                su.get_parsed(url).session(id).await.unwrap();
                su.get_page_with_cookies(url).session(id).await.unwrap();
                su.get_page_with_headers(url).session(id).await.unwrap();
                su.get_page_with_xhr(url, "/api").session(id).await.unwrap();
            }
            None => {
                su.get_page_source(url).await.unwrap();
                su.list_elements(url).await.unwrap();
                su.get_markdown(url).await.unwrap();
                su.get_parsed(url).await.unwrap();
                su.get_page_with_cookies(url).await.unwrap();
                su.get_page_with_headers(url).await.unwrap();
                su.get_page_with_xhr(url, "/api").await.unwrap();
            }
        }
    }

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 14);
    for req in &requests[..7] {
        assert_eq!(query_values(req, "session"), ["cart-42.a_b"], "{}", req.url);
    }
    for req in &requests[7..] {
        assert!(query_values(req, "session").is_empty(), "{}", req.url);
    }
}

#[tokio::test]
async fn session_combines_with_other_options() {
    let server = MockServer::start().await;
    Mock::given(path("/getPageSource"))
        .and(query_param("session", "s1"))
        .and(query_param("proxy_country", "de"))
        .and(query_param("steps", r#"[{"action":"wait","value":500}]"#))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>ok</html>"))
        .expect(1)
        .mount(&server)
        .await;
    let su = client(&server).await;
    let html = su
        .get_page_source("https://example.com/")
        .session("s1")
        .proxy_country("de")
        .steps([Step::wait(500)])
        .await
        .unwrap();
    assert_eq!(html, "<html>ok</html>");
}

#[tokio::test]
async fn session_changed_comes_from_the_header() {
    let server = MockServer::start().await;
    for (id, value) in [("moved", Some("true")), ("other", Some("false"))] {
        let mut response = ResponseTemplate::new(200).set_body_json(session_page());
        if let Some(value) = value {
            response = response.insert_header("X-Session-Changed", value);
        }
        Mock::given(path("/getPageSource"))
            .and(query_param("session", id))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    Mock::given(path("/getPageSource"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_page()))
        .mount(&server)
        .await;
    let su = client(&server).await;
    let url = "https://example.com/";

    for (id, expected) in [("moved", true), ("kept", false), ("other", false)] {
        let parsed = su.get_parsed(url).session(id).await.unwrap();
        let cookies = su.get_page_with_cookies(url).session(id).await.unwrap();
        let headers = su.get_page_with_headers(url).session(id).await.unwrap();
        let xhr = su.get_page_with_xhr(url, "/api").session(id).await.unwrap();
        assert_eq!(parsed.session_changed, expected, "get_parsed {id}");
        assert_eq!(
            cookies.session_changed, expected,
            "get_page_with_cookies {id}"
        );
        assert_eq!(
            headers.session_changed, expected,
            "get_page_with_headers {id}"
        );
        assert_eq!(xhr.session_changed, expected, "get_page_with_xhr {id}");
    }
}

#[cfg(feature = "blocking")]
#[test]
fn blocking_client_reports_a_moved_session() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/getPageSource"))
            .and(query_param("session", "s1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-session-changed", "true")
                    .set_body_json(session_page()),
            )
            .expect(1)
            .mount(&server)
            .await;
        server
    });
    let su = scrapeunblocker::blocking::Client::from_builder(
        Client::builder().api_key(KEY).base_url(server.uri()),
    )
    .unwrap();
    let page = su
        .get_page_with_headers("https://example.com/")
        .session("s1")
        .send()
        .unwrap();
    assert!(page.session_changed);
}
