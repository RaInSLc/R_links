use super::commands_browser::browser_search_url_for_package;
use super::state::{BrowserOpenLimiter, BROWSER_OPEN_WINDOW, MAX_BROWSER_OPEN_REQUESTS};
use std::time::Instant;
#[test]
fn browser_open_limiter_enforces_window_limit() {
    let l = BrowserOpenLimiter::default();
    let n = Instant::now();
    for _ in 0..MAX_BROWSER_OPEN_REQUESTS {
        assert!(l.try_acquire(n).is_ok())
    }
    assert!(l.try_acquire(n).is_err());
    assert!(l.try_acquire(n + BROWSER_OPEN_WINDOW).is_ok())
}
#[test]
fn browser_search_url_rejects_invalid_package_without_echoing_value() {
    let p = format!("bad/{}", "x".repeat(4096));
    let e = browser_search_url_for_package(&p, None).expect_err("非法包名应被拒绝");
    assert_eq!(e, "无效包名，无法打开浏览器搜索");
    assert!(!e.contains(&p));
}
#[test]
fn browser_search_url_encodes_valid_package() {
    assert_eq!(
        browser_search_url_for_package("GSVA", None).unwrap(),
        "https://www.google.com/search?q=R%20package%20GSVA"
    );
}
