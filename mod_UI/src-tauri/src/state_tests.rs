use super::state::*;
use std::sync::atomic::Ordering;
#[test]
fn search_state_rejects_overlapping_runs() {
    let s = SearchState::default();
    let r = s.try_begin(10).unwrap();
    assert!(s.is_running_for_test());
    assert_eq!(s.run_id_for_test(), 10);
    assert!(!r.cancelled().load(Ordering::SeqCst));
    assert!(s.try_begin(11).is_err());
    drop(r);
    assert!(!s.is_running_for_test());
    assert_eq!(s.run_id_for_test(), 0);
}
#[test]
fn search_state_clears_cancel_flag_after_run_drop() {
    let s = SearchState::default();
    let r = s.try_begin(20).unwrap();
    assert!(s.request_stop(20));
    assert!(r.cancelled().load(Ordering::SeqCst));
    drop(r);
    assert!(!s.is_cancelled_for_test());
}
#[test]
fn search_state_toggles_pause_only_for_active_run() {
    let s = SearchState::default();
    let r = s.try_begin(25).unwrap();
    assert!(!s.is_paused(25));
    assert!(!s.set_paused(24, true));
    assert!(s.set_paused(25, true));
    assert!(s.is_paused(25));
    assert!(s.set_paused(25, false));
    drop(r);
}
#[test]
fn search_state_cancels_only_matching_package_and_run() {
    let s = SearchState::default();
    let r = s.try_begin(26).unwrap();
    assert!(!s.cancel_package(25, "dplyr"));
    assert!(s.cancel_package(26, "DPLYR"));
    assert!(s.is_package_cancelled(26, "dplyr"));
    drop(r);
    assert!(!s.is_package_cancelled(26, "dplyr"));
}
#[test]
fn search_state_ignores_idle_stop_requests() {
    let s = SearchState::default();
    assert!(!s.request_stop(30));
    assert!(!s.is_running_for_test());
}
#[test]
fn search_state_ignores_stale_stop_requests() {
    let s = SearchState::default();
    let r = s.try_begin(40).unwrap();
    assert!(!s.request_stop(39));
    assert!(s.request_stop(40));
    drop(r);
}
#[test]
fn search_state_rejects_non_js_safe_run_ids() {
    let s = SearchState::default();
    assert!(s.try_begin(0).is_err());
    assert!(s.try_begin(MAX_JS_SAFE_INTEGER + 1).is_err());
    let r = s.try_begin(MAX_JS_SAFE_INTEGER).unwrap();
    drop(r);
}
#[test]
fn search_state_releases_slot_when_setup_returns_error() {
    let s = SearchState::default();
    let x = (|| {
        let _r = s.try_begin(50)?;
        Err::<(), String>("模拟初始化失败".into())
    })();
    assert!(x.is_err());
    assert!(s.try_begin(51).is_ok());
}
