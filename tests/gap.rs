//! Gap tests: pin documented limitations so future semantic changes are deliberate.
use sqlmap_rs::{SqlmapEngine, SqlmapOptions};
use std::fs;

#[tokio::test]
async fn gap_api_url_always_localhost_via_accessor() {
    let engine = SqlmapEngine::new(59999, false, None)
        .await
        .expect("engine without local spawn");
    assert!(
        engine.api_url().contains("127.0.0.1"),
        "gap: api_url must always target localhost: {}",
        engine.api_url()
    );
    assert_eq!(engine.api_url(), "http://127.0.0.1:59999");
}

#[test]
fn gap_readme_documents_localhost_only() {
    let readme = fs::read_to_string("README.md").expect("read README");
    assert!(
        readme.contains("127.0.0.1"),
        "gap: README must document localhost-only API binding"
    );
    assert!(
        readme.to_lowercase().contains("localhost"),
        "gap: README must mention localhost-only mode"
    );
}

#[test]
fn gap_raii_cleanup_documented_as_best_effort_in_readme() {
    let readme = fs::read_to_string("README.md").expect("read README");
    assert!(
        readme.to_lowercase().contains("best-effort"),
        "gap: README must state RAII cleanup is best-effort"
    );
}

#[test]
fn gap_sql_shell_serializes_but_may_fail_against_real_api() {
    // Field exists and serializes to sqlmap's REST key; interactive sql shell
    // is not validated against a live daemon in this crate (gap).
    let opts = SqlmapOptions::builder().sql_shell(true).build();
    let json = serde_json::to_string(&opts).expect("serialize");
    assert!(
        json.contains("\"sqlShell\":true"),
        "gap: sql_shell must serialize as sqlShell when set"
    );
    assert!(
        !json.contains("\"sql_shell\""),
        "gap: Rust field name must not leak into JSON"
    );
}
