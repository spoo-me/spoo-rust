//! Runtime tests on wasm32, driven by wasm-bindgen-test in a headless
//! browser against the scripted mock in scripts/wasm-mock.py. This is what
//! lets the README claim wasm support at runtime, not just at compile time:
//! it exercises the paths that once compiled but panicked on this target
//! (clock reads in the backoff, `Utc::now()` in Retry-After handling).
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Must match scripts/wasm-mock.py.
const MOCK_BASE: &str = "http://127.0.0.1:18300";

fn client() -> spoo_me::Client {
    spoo_me::Client::builder()
        .api_key("spoo_wasm_test")
        .base_url(MOCK_BASE)
        .build()
        .expect("test client config is valid")
}

#[wasm_bindgen_test]
async fn get_link_decodes() {
    let link = client()
        .links()
        .get("plain")
        .await
        .expect("request round-trips in the browser");
    assert_eq!(link.id, "plain");
}

#[wasm_bindgen_test]
async fn retry_after_429_recovers() {
    // The mock answers 429 with Retry-After: 1 on the first hit and 200 on
    // the second: this proves the retry loop, the header parse (which reads
    // the clock) and the timer-based sleep all work on wasm.
    let link = client()
        .links()
        .get("retry")
        .await
        .expect("429 retry recovers in the browser");
    assert_eq!(link.id, "retry");
}

#[wasm_bindgen_test]
async fn errors_map_on_wasm() {
    let err = client()
        .links()
        .get("missing")
        .await
        .expect_err("404 surfaces");
    assert!(err.is_not_found());
}
