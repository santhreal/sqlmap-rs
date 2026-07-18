//! Integration tests with a mock localhost API (no live sqlmapapi required).
use sqlmap_rs::{SqlmapEngine, SqlmapError, SqlmapOptions};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

struct MockRoute {
    body: &'static str,
    status: u16,
}

struct MockApi {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl MockApi {
    async fn start(routes: HashMap<&'static str, MockRoute>) -> Self {
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

                        let route = routes
                            .iter()
                            .find(|(prefix, _)| path.contains(**prefix))
                            .map(|(_, route)| route)
                            .unwrap_or(&MockRoute {
                                body: r#"{"success":false,"message":"unhandled route"}"#,
                                status: 200,
                            });

                        let status_line = match route.status {
                            200 => "200 OK",
                            404 => "404 Not Found",
                            500 => "500 Internal Server Error",
                            code => {
                                let _ = code;
                                "500 Internal Server Error"
                            }
                        };
                        let response = format!(
                            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            route.body.len(),
                            route.body
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

fn mock_ok(body: &'static str) -> MockRoute {
    MockRoute { body, status: 200 }
}

fn mock_status(body: &'static str, status: u16) -> MockRoute {
    MockRoute { body, status }
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
async fn integration_rejects_port_zero_when_spawn_local() {
    let err = match SqlmapEngine::new(0, true, None).await {
        Err(e) => e,
        Ok(_) => panic!("port 0 with spawn_local must fail"),
    };
    assert_api_error_contains(err, "port 0");
}

#[tokio::test]
async fn integration_rejects_zero_poll_interval() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);

    let err =
        match SqlmapEngine::with_config(port, false, None, Duration::from_secs(10), Duration::ZERO)
            .await
        {
            Err(e) => e,
            Ok(_) => panic!("zero poll_interval must fail"),
        };
    assert_api_error_contains(err, "poll_interval");
}

#[tokio::test]
async fn integration_fetch_data_rejects_success_false() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/data",
        mock_ok(r#"{"success":false,"message":"data unavailable"}"#),
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
        .fetch_data()
        .await
        .expect_err("must reject success=false");
    assert_api_error_contains(err, "data unavailable");
}

#[tokio::test]
async fn integration_fetch_log_rejects_success_false() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/log",
        mock_ok(r#"{"success":false,"message":"log unavailable"}"#),
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
        .fetch_log()
        .await
        .expect_err("must reject success=false");
    assert_api_error_contains(err, "log unavailable");
}

#[tokio::test]
async fn integration_list_options_rejects_success_false() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/option/mock-task/list",
        mock_ok(r#"{"success":false,"message":"options unavailable"}"#),
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

#[tokio::test]
async fn integration_wait_for_completion_propagates_status_message() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":false,"message":"status probe failed"}"#),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::with_config(
        mock.port,
        false,
        None,
        Duration::from_secs(10),
        Duration::from_millis(10),
    )
    .await
    .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task
        .wait_for_completion(1)
        .await
        .expect_err("must reject success=false status");
    assert_api_error_contains(err, "status probe failed");
}

#[tokio::test]
async fn integration_start_rejects_non_success_http() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/start",
        mock_status(r#"{"success":true}"#, 500),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task.start().await.expect_err("must reject HTTP 500");
    assert_api_error_contains(err, "HTTP 500");
}

#[tokio::test]
async fn integration_stop_rejects_non_success_http() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/stop",
        mock_status(r#"{"success":true}"#, 404),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task.stop().await.expect_err("must reject HTTP 404");
    assert_api_error_contains(err, "HTTP 404");
}

#[tokio::test]
async fn integration_kill_rejects_non_success_http() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/kill",
        mock_status(r#"{"success":true}"#, 404),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task.kill().await.expect_err("must reject HTTP 404");
    assert_api_error_contains(err, "HTTP 404");
}
