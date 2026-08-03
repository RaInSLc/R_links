use std::collections::{HashSet, VecDeque};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub(crate) const MAX_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub(crate) const MAX_RSCRIPT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_BROWSER_OPEN_REQUESTS: usize = 30;
pub(crate) const BROWSER_OPEN_WINDOW: Duration = Duration::from_secs(60);

pub struct SearchState { pub(crate) inner: Mutex<SearchStateInner> }
pub(crate) struct SearchStateInner {
    pub(crate) running: bool,
    pub(crate) paused: bool,
    pub(crate) run_id: u64,
    pub(crate) cancelled_packages: HashSet<String>,
    pub(crate) cancellation: Arc<AtomicBool>,
}
pub(crate) struct SearchRunGuard<'a> { state: &'a SearchState, run_id: u64, cancellation: Arc<AtomicBool> }

impl Default for SearchState {
    fn default() -> Self { Self { inner: Mutex::new(SearchStateInner { running: false, paused: false, run_id: 0, cancelled_packages: HashSet::new(), cancellation: Arc::new(AtomicBool::new(false)) }) } }
}
impl SearchState {
    pub(crate) fn try_begin(&self, run_id: u64) -> Result<SearchRunGuard<'_>, String> {
        if run_id == 0 || run_id > MAX_JS_SAFE_INTEGER { return Err("检索任务 ID 无效".to_string()); }
        let mut inner = self.lock_inner();
        if inner.running { return Err("已有检索任务正在运行".to_string()); }
        let cancellation = Arc::new(AtomicBool::new(false));
        inner.running = true; inner.paused = false; inner.run_id = run_id;
        inner.cancelled_packages.clear(); inner.cancellation = Arc::clone(&cancellation);
        Ok(SearchRunGuard { state: self, run_id, cancellation })
    }
    pub(crate) fn request_stop(&self, run_id: u64) -> bool { let inner = self.lock_inner(); if !inner.running || inner.run_id != run_id { return false; } inner.cancellation.store(true, Ordering::SeqCst); true }
    pub(crate) fn set_paused(&self, run_id: u64, paused: bool) -> bool { let mut inner = self.lock_inner(); if !inner.running || inner.run_id != run_id { return false; } inner.paused = paused; true }
    pub(crate) fn is_paused(&self, run_id: u64) -> bool { let inner = self.lock_inner(); inner.running && inner.run_id == run_id && inner.paused }
    pub(crate) fn cancel_package(&self, run_id: u64, package: &str) -> bool { let mut inner = self.lock_inner(); if !inner.running || inner.run_id != run_id { return false; } inner.cancelled_packages.insert(package.to_ascii_lowercase()); true }
    pub(crate) fn is_package_cancelled(&self, run_id: u64, package: &str) -> bool { let inner = self.lock_inner(); inner.running && inner.run_id == run_id && inner.cancelled_packages.contains(&package.to_ascii_lowercase()) }
    fn lock_inner(&self) -> MutexGuard<'_, SearchStateInner> { self.inner.lock().unwrap_or_else(|error| error.into_inner()) }
    #[cfg(test)] pub(crate) fn is_running_for_test(&self) -> bool { self.lock_inner().running }
    #[cfg(test)] pub(crate) fn is_cancelled_for_test(&self) -> bool { self.lock_inner().cancellation.load(Ordering::SeqCst) }
    #[cfg(test)] pub(crate) fn run_id_for_test(&self) -> u64 { self.lock_inner().run_id }
}
impl SearchRunGuard<'_> { pub(crate) fn cancelled(&self) -> &AtomicBool { self.cancellation.as_ref() } }
impl Drop for SearchRunGuard<'_> {
    fn drop(&mut self) { self.cancellation.store(false, Ordering::SeqCst); let mut inner = self.state.lock_inner(); if inner.run_id == self.run_id && Arc::ptr_eq(&inner.cancellation, &self.cancellation) { inner.running = false; inner.paused = false; inner.run_id = 0; inner.cancelled_packages.clear(); inner.cancellation = Arc::new(AtomicBool::new(false)); } }
}

pub struct BrowserOpenLimiter { opened_at: Mutex<VecDeque<Instant>> }
impl Default for BrowserOpenLimiter { fn default() -> Self { Self { opened_at: Mutex::new(VecDeque::new()) } } }
impl BrowserOpenLimiter {
    pub(crate) fn try_acquire(&self, now: Instant) -> Result<(), String> {
        let mut opened_at = self.opened_at.lock().map_err(|_| "浏览器打开限流状态已损坏".to_string())?;
        while opened_at.front().is_some_and(|opened| now.checked_duration_since(*opened).is_some_and(|elapsed| elapsed >= BROWSER_OPEN_WINDOW)) { opened_at.pop_front(); }
        if opened_at.len() >= MAX_BROWSER_OPEN_REQUESTS { return Err(format!("浏览器搜索打开过于频繁，请稍后再试；每 {} 秒最多允许 {} 次", BROWSER_OPEN_WINDOW.as_secs(), MAX_BROWSER_OPEN_REQUESTS)); }
        opened_at.push_back(now); Ok(())
    }
}
