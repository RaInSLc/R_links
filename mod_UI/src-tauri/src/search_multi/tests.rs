use super::*;

#[tokio::test]
async fn 已取消的检索不会发送请求() {
    let client = Client::new();
    let cancelled = AtomicBool::new(true);
    let budget = RequestBudget::new(3);
    let mut logs = Vec::new();
    let result = search_one_multi(
        &client,
        &cancelled,
        &budget,
        "pip",
        "numpy",
        ">=1.0",
        "https://pypi.org",
        &[],
        0,
        1,
        &mut logs,
    )
    .await;
    assert!(!result.found);
    assert_eq!(budget.remaining.load(Ordering::SeqCst), 3);
}

#[test]
fn 数字版本约束保留运算符并正确匹配() {
    assert_eq!(
        split_requirement("numpy>=1.26"),
        ("numpy".into(), ">=1.26".into())
    );
    assert!(requirement_matches("1.27.0", ">=1.26,<2"));
    assert!(!requirement_matches("2.0", ">=1.26,<2"));
    assert!(!requirement_matches("1.26.4", "==1.26"));
    assert!(!requirement_matches("1.26", "!=1.26"));
    assert!(requirement_matches("1.26.4", "~=1.26.0"));
    assert!(!requirement_matches("1.27.0", "~=1.26.0"));
}

#[test]
fn 复合约束以第一个运算符分割包名() {
    assert_eq!(
        split_requirement("numpy>1.0,<=2.0"),
        ("numpy".into(), ">1.0,<=2.0".into())
    );
    assert!(requirement_matches("2.0", ">1.0,<=2.0"));
    assert!(!requirement_matches("1.0", ">1.0,<=2.0"));
}

#[test]
fn parses_pip_requirements() {
    assert_eq!(
        split_requirement("numpy==1.26.4"),
        ("numpy".to_string(), "==1.26.4".to_string())
    );
    assert_eq!(
        split_requirement("pandas"),
        ("pandas".to_string(), String::new())
    );
}

#[test]
fn selects_conda_latest_version_from_metadata() {
    let payload = CondaResponse {
        latest_version: Some("1.10.0".to_string()),
        versions: Some(vec!["1.2.0".to_string(), "1.10.0".to_string()]),
    };
    assert_eq!(conda_version(&payload, ""), Some("1.10.0".to_string()));
    assert_eq!(conda_version(&payload, "1.2"), Some("1.2.0".to_string()));
    assert_eq!(conda_version(&payload, "9.0"), None);
}

#[test]
fn empty_conda_metadata_is_not_a_hit() {
    let payload = CondaResponse {
        latest_version: None,
        versions: Some(Vec::new()),
    };
    assert_eq!(conda_version(&payload, ""), None);
}

#[test]
fn request_budget_tracks_remaining() {
    let budget = RequestBudget::new(3);
    assert!(budget.try_acquire());
    assert!(budget.try_acquire());
    assert!(budget.try_acquire());
    assert!(!budget.try_acquire());
    assert!(budget.is_exhausted());
}

#[test]
fn inputs_skip_comments_and_flags() {
    let result = inputs("# comment\n-numpy\npandas==2.0\nscipy");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("pandas".to_string(), "==2.0".to_string()));
    assert_eq!(result[1], ("scipy".to_string(), String::new()));
}

#[test]
fn not_found_result_has_correct_fields() {
    let result = not_found_result("nonexist", "1.0", "pip", "测试消息");
    assert!(!result.found);
    assert_eq!(result.status, "notFound");
    assert_eq!(result.message, "测试消息");
}
