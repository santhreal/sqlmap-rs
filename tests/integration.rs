//! Integration tests with a mock localhost API (no live sqlmapapi required).
use sqlmap_rs::{SqlmapEngine, SqlmapError, SqlmapOptions};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

struct MockApi {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl MockApi {
    async fn start(routes: HashMap<&'static str, &'static str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock listener");
        let port = listener.local_addr().expect("local addr").port();
        let routes = Arc::new(routes);
        let (shutdown, mut stop) = oneshot::channel();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stop => break,
                    accepted = listener.accept() => {
                        let Ok((mut stream, _)) = accepted else { continue };
                        let mut buf = vec![0u8; 8192];
                        let n = stream.read(&mut buf).await.unwrap_or(0);
                        let request = String::from_utf8_lossy(&buf[..n]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("");

                        let body = routes
                            .iter()
                            .find(|(prefix, _)| path.contains(**prefix))
                            .map(|(_, body)| *body)
                            .unwrap_or(r#"{"success":false,"message":"unhandled route"}"#);

                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                }
            }
        });

        Self {
            port,
            shutdown: Some(shutdown),
            _handle: handle,
        }
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn assert_api_error_contains(err: SqlmapError, needle: &str) {
    match err {
        SqlmapError::ApiError(msg) => assert!(
            msg.contains(needle),
            "expected ApiError containing {needle:?}, got {msg:?}"
        ),
        other => panic!("expected ApiError, got {other:?}"),
    }
}

fn scan_options() -> SqlmapOptions {
    SqlmapOptions::builder()
        .url("http://example.com/?id=1")
        .batch(true)
        .build()
}

#[test]
fn integration_is_available_at_nonexistent_binary_is_false() {
    assert!(
        !SqlmapEngine::is_available_at("/nonexistent/sqlmapapi-binary"),
        "missing binary path must report unavailable"
    );
}

#[tokio::test]
async fn integration_fetch_data_rejects_success_false() {
    let mut routes = HashMap::new();
    routes.insert("/task/new", r#"{"success":true,"taskid":"mock-task"}"#);
    routes.insert("/option/mock-task/set", r#"{"success":true}"#);
    routes.insert(
        "/scan/mock-task/data",
        r#"{"success":false,"message":"data unavailable"}"#,
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task.fetch_data().await.expect_err("must reject success=false");
    assert_api_error_contains(err, "data unavailable");
}

#[tokio::test]
async fn integration_fetch_log_rejects_success_false() {
    let mut routes = HashMap::new();
    routes.insert("/task/new", r#"{"success":true,"taskid":"mock-task"}"#);
    routes.insert("/option/mock-task/set", r#"{"success":true}"#);
    routes.insert(
        "/scan/mock-task/log",
        r#"{"success":false,"message":"log unavailable"}"#,
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task.fetch_log().await.expect_err("must reject success=false");
    assert_api_error_contains(err, "log unavailable");
}

#[tokio::test]
async fn integration_list_options_rejects_success_false() {
    let mut routes = HashMap::new();
    routes.insert("/task/new", r#"{"success":true,"taskid":"mock-task"}"#);
    routes.insert("/option/mock-task/set", r#"{"success":true}"#);
    routes.insert(
        "/option/mock-task/list",
        r#"{"success":false,"message":"options unavailable"}"#,
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task
        .list_options()
        .await
        .expect_err("must reject success=false");
    assert_api_error_contains(err, "options unavailable");
}
