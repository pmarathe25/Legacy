use game_contracts::causal::{Review, candidates, scope, wake};
#[test]
fn disabled_tracing_never_evaluates_observers() {
    if std::env::var_os("LEGACY_REVIEW_TRACE").is_some() {
        return;
    }
    let review = Review::begin(1, 2, 3, || panic!("disabled observer"));
    review.annotate("unused", || panic!("disabled annotation"));
    let _scope = scope("request", "test.no-output");
    wake(1, 2, false, false, true);
    candidates(1, std::iter::from_fn(|| panic!("disabled candidates")));
}
#[cfg(feature = "causality")]
#[test]
fn trace_child() {
    if std::env::var_os("LEGACY_CAUSAL_TEST_CHILD").is_none() {
        return;
    }
    let _root = scope("request", "test.root");
    {
        let _event = scope("event", "test.completed");
        wake(7, 20, true, false, false);
        wake(7, 20, false, true, true);
    }
    let review = Review::begin(7, 20, 42, || serde_json::json!({"chosen":"old"}));
    candidates(7, [(1, [2, 3, 4, 5, 6], None)].into_iter());
    review.annotate("success", || true.into());
    let _listener = scope("listener", "test.help");
    wake(8, 20, false, false, true);
}
#[cfg(feature = "causality")]
#[test]
fn trace_correlates_nested_requests_wakes_and_reviews() {
    let path =
        std::env::temp_dir().join(format!("legacy-review-test-{}.jsonl", std::process::id()));
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "trace_child", "--nocapture"])
        .env("LEGACY_CAUSAL_TEST_CHILD", "1")
        .env("LEGACY_REVIEW_TRACE", &path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let rows: Vec<serde_json::Value> = source
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len(), 4);
    let review = rows.iter().find(|r| r["type"] == "review").unwrap();
    assert_eq!(review["causes"].as_array().unwrap().len(), 1);
    assert_eq!(review["causes"][0]["hunger_only"], true);
    assert_eq!(review["causes"][0]["stack"][1][1], "test.completed");
    assert_eq!(review["candidates"]["count"], 1);
    assert_eq!(review["success"], true);
    assert_eq!(rows[2]["during_review"], review["id"]);
    assert_eq!(rows[2]["stack"][1][1], "test.help");
}
