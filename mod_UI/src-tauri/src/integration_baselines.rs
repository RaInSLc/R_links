use crate::models::{SearchResult, Settings};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

// 本地 HTTP 固定数据，不访问外部镜像，也不读取用户缓存。
struct Fixture {
    url: String,
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(width: usize, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let (stop_clone, peak_clone, requests_clone) =
            (stop.clone(), peak.clone(), requests.clone());
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !stop_clone.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let (active, peak, requests) =
                        (active.clone(), peak_clone.clone(), requests_clone.clone());
                    workers.push(std::thread::spawn(move || {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut buffer = [0; 4096];
                        let count = stream.read(&mut buffer).unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..count]);
                        let concurrent = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(concurrent, Ordering::SeqCst);
                        requests.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(delay);
                        let path = request.split_whitespace().nth(1).unwrap_or("");
                        let name = path.trim_end_matches("/DESCRIPTION").rsplit('/').next().unwrap_or("root");
                        let deps = if name == "root" { format!("Imports: {}\n", (0..width).map(|i| format!("dep{i}")).collect::<Vec<_>>().join(", ")) } else { String::new() };
                        let body = format!("Package: {name}\nVersion: 1.0\n{deps}");
                        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        let _ = stream.write_all(response.as_bytes());
                        active.fetch_sub(1, Ordering::SeqCst);
                    }));
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            url,
            stop,
            peak,
            requests,
            thread: Some(thread),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn roots() -> Vec<SearchResult> {
    vec![SearchResult {
        package: "root".into(),
        real_name: "root".into(),
        latest_version: "1.0".into(),
        source: "cran".into(),
        found: true,
        ..Default::default()
    }]
}

#[tokio::test]
#[ignore = "集成性能基准，CI 单独执行"]
async fn baseline_wide_dependencies() {
    let fixture = Fixture::new(120, Duration::from_millis(5));
    let settings = Settings {
        cran_mirror: fixture.url.clone(),
        use_cache: false,
        search_concurrency: 6,
        max_dependency_nodes: 100,
        max_dependency_depth: 2,
        ..Default::default()
    };
    let start = Instant::now();
    let graph = crate::dependency::resolve_dependencies_inner(
        None,
        &reqwest::Client::new(),
        &roots(),
        &settings,
        &AtomicBool::new(false),
        &crate::search::RequestBudget::new(200),
        start + Duration::from_secs(30),
    )
    .await
    .unwrap();
    assert_eq!(graph.nodes.len(), 100);
    assert_eq!(graph.edges.len(), 99);
    assert!(fixture.peak.load(Ordering::SeqCst) <= 6);
    assert_eq!(fixture.requests.load(Ordering::SeqCst), 100);
    println!(
        "BENCH wide nodes=100 requests=100 peak={} elapsed_ms={}",
        fixture.peak.load(Ordering::SeqCst),
        start.elapsed().as_millis()
    );
}

#[tokio::test]
#[ignore = "集成性能基准，CI 单独执行"]
async fn baseline_slow_network_deadline() {
    let fixture = Fixture::new(100, Duration::from_millis(800));
    let settings = Settings {
        cran_mirror: fixture.url.clone(),
        use_cache: false,
        ..Default::default()
    };
    let start = Instant::now();
    let cancelled = AtomicBool::new(false);
    let budget = crate::search::RequestBudget::new(200);
    let client = reqwest::Client::new();
    let roots = roots();
    let deadline = start + Duration::from_millis(150);
    let result = crate::search::await_or_stop(
        crate::dependency::resolve_dependencies_inner(
            None, &client, &roots, &settings, &cancelled, &budget, deadline,
        ),
        &cancelled,
        &budget,
        deadline,
    )
    .await;
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(fixture.requests.load(Ordering::SeqCst) <= 1);
    println!(
        "BENCH slow deadline_ms=150 elapsed_ms={}",
        start.elapsed().as_millis()
    );
}

#[test]
#[ignore = "集成性能基准，CI 单独执行"]
fn baseline_large_cache() {
    let start = Instant::now();
    let mut cache = std::collections::HashMap::new();
    for i in 0..10000 {
        let result = SearchResult {
            package: format!("pkg{i}"),
            real_name: format!("pkg{i}"),
            source: "cran".into(),
            latest_version: "1.0".into(),
            found: true,
            ..Default::default()
        };
        cache.insert(
            result.package.clone(),
            crate::search::cache_entry_from_result(&result, None, "1800000000".into()),
        );
    }
    let encoded = serde_json::to_vec(&cache).unwrap();
    let decoded: std::collections::HashMap<String, crate::models::PackageCacheEntry> =
        serde_json::from_slice(&encoded).unwrap();
    let index = crate::search::index_cache(&decoded);
    for _ in 0..10 {
        for i in 0..10000 {
            assert_eq!(index[&format!("pkg{i}")].len(), 1);
        }
    }
    assert!(start.elapsed() < Duration::from_secs(20));
    println!(
        "BENCH cache entries=10000 lookups=100000 bytes={} elapsed_ms={}",
        encoded.len(),
        start.elapsed().as_millis()
    );
}
