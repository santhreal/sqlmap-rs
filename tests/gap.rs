//! Gap tests: pin documented limitations so future semantic changes are deliberate.
use sqlmap_rs::{SqlmapEngine, SqlmapOptions};
use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn start_probe_mock() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock listener");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let mut buf = vec![0u8; 8192];
            let _n = stream.read(&mut buf).await.unwrap_or(0);
            let body = r#"{"success":true,"taskid":"gap-probe"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    port
}

#[tokio::test]
async fn gap_api_url_always_localhost_via_accessor() {
    let port = start_probe_mock().await;
    let engine = SqlmapEngine::new(port, false, None)
        .await
        .expect("engine without local spawn");
    assert!(
        engine.api_url().contains("127.0.0.1"),
        "gap: api_url must always target localhost: {}",
        engine.api_url()
    );
    assert_eq!(engine.api_url(), format!("http://127.0.0.1:{port}"));
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

#[test]
fn gap_json_format_ser_failure_collapses_to_empty_array() {
    // Documented limitation: if SqlmapFinding serialization ever fails,
    // format_findings JSON arms return "[]" instead of a typed error.
    use sqlmap_rs::types::{format_findings, OutputFormat, SqlmapFinding};
    let findings = vec![SqlmapFinding::new(
        "id",
        "boolean-based blind",
        "id=1",
        serde_json::json!({}),
    )];
    let json = format_findings(&findings, OutputFormat::Json);
    let parsed: Vec<SqlmapFinding> = serde_json::from_str(&json).expect("parseable array");
    assert_eq!(parsed.len(), 1);
    assert!(
        !json.contains("\"error\""),
        "gap: JSON format must not emit fake error objects"
    );
}

#[test]
fn gap_readme_documents_port_conflict_toctou_and_attach_mode() {
    let readme = fs::read_to_string("README.md").expect("read README");
    assert!(
        readme.to_lowercase().contains("toctou"),
        "gap: README must document port-conflict TOCTOU race"
    );
    assert!(
        readme.contains("spawn_local=false") || readme.contains("spawn_local = false"),
        "gap: README must document attach mode (spawn_local=false)"
    );
    assert!(
        readme.contains("/task/new"),
        "gap: README must document attach-mode /task/new health probe"
    );
}
