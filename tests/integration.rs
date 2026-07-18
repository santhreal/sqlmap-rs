//! Integration tests with a mock localhost API (no live sqlmapapi required).
use sqlmap_rs::{SqlmapEngine, SqlmapError, SqlmapOptions};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

enum MockBody {
    Static(&'static str),
    Rotating {
        bodies: Arc<Vec<&'static str>>,
        index: Arc<AtomicUsize>,
    },
    Sequence {
        responses: Arc<Vec<MockResponse>>,
        index: Arc<AtomicUsize>,
    },
}

struct MockResponse {
    body: &'static str,
    status: u16,
}

struct MockRoute {
    body: MockBody,
    status: u16,
}

struct MockApi {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    _handle: tokio::task::JoinHandle<()>,
}

impl MockApi {
    async fn start(routes: HashMap<&'static str, MockRoute>) -> Self {
        Self::start_inner(routes).await
    }

    async fn start_inner(routes: HashMap<&'static str, MockRoute>) -> Self {
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

                        let default_route = MockRoute {
                            body: MockBody::Static(
                                r#"{"success":false,"message":"unhandled route"}"#,
                            ),
                            status: 200,
                        };
                        let route = routes
                            .iter()
                            .find(|(prefix, _)| path.contains(**prefix))
                            .map(|(_, route)| route)
                            .unwrap_or(&default_route);

                        let (body, status) = match &route.body {
                            MockBody::Static(text) => (*text, route.status),
                            MockBody::Rotating { bodies, index } => {
                                let i = index.fetch_add(1, Ordering::SeqCst);
                                (bodies[i.min(bodies.len().saturating_sub(1))], route.status)
                            }
                            MockBody::Sequence { responses, index } => {
                                let i = index.fetch_add(1, Ordering::SeqCst);
                                let r = &responses[i.min(responses.len().saturating_sub(1))];
                                (r.body, r.status)
                            }
                        };

                        let status_line = match status {
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
                            body.len(),
                            body
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
    MockRoute {
        body: MockBody::Static(body),
        status: 200,
    }
}

fn mock_status(body: &'static str, status: u16) -> MockRoute {
    MockRoute {
        body: MockBody::Static(body),
        status,
    }
}

fn mock_rotating(bodies: Vec<&'static str>) -> MockRoute {
    MockRoute {
        body: MockBody::Rotating {
            bodies: Arc::new(bodies),
            index: Arc::new(AtomicUsize::new(0)),
        },
        status: 200,
    }
}

fn mock_sequence(responses: Vec<(&'static str, u16)>) -> MockRoute {
    MockRoute {
        body: MockBody::Sequence {
            responses: Arc::new(
                responses
                    .into_iter()
                    .map(|(body, status)| MockResponse { body, status })
                    .collect(),
            ),
            index: Arc::new(AtomicUsize::new(0)),
        },
        status: 200,
    }
}

fn assert_invalid_task(err: SqlmapError, needle: &str) {
    match err {
        SqlmapError::InvalidTask(msg) => assert!(
            msg.contains(needle),
            "expected InvalidTask containing {needle:?}, got {msg:?}"
        ),
        other => panic!("expected InvalidTask, got {other:?}"),
    }
}

fn assert_malformed_response(err: SqlmapError) {
    match err {
        SqlmapError::MalformedResponse => {}
        other => panic!("expected MalformedResponse, got {other:?}"),
    }
}

fn assert_timeout(err: SqlmapError, secs: u64) {
    match err {
        SqlmapError::Timeout(s) => assert_eq!(s, secs),
        other => panic!("expected Timeout({secs}), got {other:?}"),
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
async fn integration_rejects_port_zero_when_spawn_local() {
    let err = match SqlmapEngine::new(0, true, None).await {
        Err(e) => e,
        Ok(_) => panic!("port 0 with spawn_local must fail"),
    };
    assert_api_error_contains(err, "port 0");
}

#[tokio::test]
async fn integration_rejects_port_zero_when_attach() {
    let err = match SqlmapEngine::new(0, false, None).await {
        Err(e) => e,
        Ok(_) => panic!("port 0 in attach mode must fail"),
    };
    assert_api_error_contains(err, "port 0");
}

#[tokio::test]
async fn integration_rejects_zero_request_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);

    let err = match SqlmapEngine::with_config(
        port,
        false,
        None,
        Duration::ZERO,
        Duration::from_millis(100),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("zero request_timeout must fail"),
    };
    assert_api_error_contains(err, "request_timeout");
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

#[tokio::test]
async fn integration_attach_health_probe_rejects_http_500() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_status(r#"{"success":true,"taskid":"probe-task"}"#, 500),
    );
    let mock = MockApi::start(routes).await;
    let err = match SqlmapEngine::new(mock.port, false, None).await {
        Err(e) => e,
        Ok(_) => panic!("attach health probe must reject HTTP 500"),
    };
    assert_api_error_contains(err, "HTTP 500");
}

#[tokio::test]
async fn integration_create_task_rejects_http_500_on_task_new() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_sequence(vec![
            (r#"{"success":true,"taskid":"probe-task"}"#, 200),
            (r#"{"success":true,"taskid":"mock-task"}"#, 500),
        ]),
    );
    routes.insert("/task/probe-task/delete", mock_ok(r#"{"success":true}"#));
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("health probe must succeed");

    let err = match engine.create_task(&scan_options()).await {
        Err(e) => e,
        Ok(_) => panic!("HTTP 500 on /task/new must fail"),
    };
    assert_api_error_contains(err, "HTTP 500");
}

#[tokio::test]
async fn integration_create_task_rejects_http_500_on_option_set() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert(
        "/option/mock-task/set",
        mock_status(r#"{"success":true}"#, 500),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");

    let err = match engine.create_task(&scan_options()).await {
        Err(e) => e,
        Ok(_) => panic!("HTTP 500 on option set must fail"),
    };
    assert_api_error_contains(err, "HTTP 500");
}

#[tokio::test]
async fn integration_create_task_rejects_empty_taskid() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_sequence(vec![
            (r#"{"success":true,"taskid":"probe-task"}"#, 200),
            (r#"{"success":true,"taskid":""}"#, 200),
        ]),
    );
    routes.insert("/task/probe-task/delete", mock_ok(r#"{"success":true}"#));
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");

    let err = match engine.create_task(&scan_options()).await {
        Err(e) => e,
        Ok(_) => panic!("empty taskid must fail"),
    };
    assert_invalid_task(err, "empty taskid");
}

#[tokio::test]
async fn integration_fetch_data_rejects_http_500() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/data",
        mock_status(r#"{"success":false,"message":"server blew up"}"#, 500),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let err = task.fetch_data().await.expect_err("must reject HTTP 500");
    assert_api_error_contains(err, "HTTP 500");
}

#[tokio::test]
async fn integration_spawn_local_rejects_port_conflict() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let port = listener.local_addr().expect("local addr").port();

    let err = match SqlmapEngine::new(port, true, Some("/nonexistent/sqlmapapi-binary")).await {
        Err(e) => e,
        Ok(_) => panic!("occupied port must fail before spawn"),
    };
    match err {
        SqlmapError::PortConflict {
            port: conflict_port,
        } => assert_eq!(conflict_port, port),
        other => panic!("expected PortConflict, got {other:?}"),
    }
}

#[tokio::test]
async fn integration_wait_after_stop_accepts_non_zero_returncode() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert("/scan/mock-task/stop", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":true,"status":"terminated","returncode":-15}"#),
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

    task.stop().await.expect("stop must succeed");
    task.wait_for_completion(2)
        .await
        .expect("wait after stop must accept signal returncode");
}

#[tokio::test]
async fn integration_wait_after_kill_accepts_non_zero_returncode() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert("/scan/mock-task/kill", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":true,"status":"terminated","returncode":9}"#),
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

    task.kill().await.expect("kill must succeed");
    task.wait_for_completion(2)
        .await
        .expect("wait after kill must accept signal returncode");
}

#[tokio::test]
async fn integration_create_task_rejects_malformed_json() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_sequence(vec![
            (r#"{"success":true,"taskid":"probe-task"}"#, 200),
            ("not-json", 200),
        ]),
    );
    routes.insert("/task/probe-task/delete", mock_ok(r#"{"success":true}"#));
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::new(mock.port, false, None)
        .await
        .expect("engine without local spawn");

    let err = match engine.create_task(&scan_options()).await {
        Err(e) => e,
        Ok(_) => panic!("malformed JSON must fail"),
    };
    assert_malformed_response(err);
}

#[tokio::test]
async fn integration_wait_for_completion_running_to_terminated() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_rotating(vec![
            r#"{"success":true,"status":"running","returncode":null}"#,
            r#"{"success":true,"status":"terminated","returncode":0}"#,
        ]),
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

    task.wait_for_completion(2)
        .await
        .expect("running then terminated must succeed");
}

#[tokio::test]
async fn integration_wait_for_completion_not_running_until_timeout() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":true,"status":"not running","returncode":null}"#),
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
        .expect_err("not running forever must time out");
    assert_timeout(err, 1);
}

#[tokio::test]
async fn integration_wait_for_completion_rejects_non_zero_returncode() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":true,"status":"terminated","returncode":1}"#),
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
        .wait_for_completion(2)
        .await
        .expect_err("non-zero returncode must fail");
    assert_api_error_contains(err, "non-zero exit code 1");
}

#[tokio::test]
async fn integration_wait_for_completion_rejects_null_returncode() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":true,"status":"terminated","returncode":null}"#),
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
        .wait_for_completion(2)
        .await
        .expect_err("terminated with null returncode must fail");
    assert_api_error_contains(err, "without a process exit code");
}

#[tokio::test]
async fn integration_wait_for_completion_zero_timeout_fails_fast() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_ok(r#"{"success":true,"status":"running","returncode":null}"#),
    );
    let mock = MockApi::start(routes).await;
    let engine = SqlmapEngine::with_config(
        mock.port,
        false,
        None,
        Duration::from_secs(10),
        Duration::from_millis(500),
    )
    .await
    .expect("engine without local spawn");
    let task = engine
        .create_task(&scan_options())
        .await
        .expect("create task");

    let start = std::time::Instant::now();
    let err = task
        .wait_for_completion(0)
        .await
        .expect_err("zero timeout must fail immediately");
    assert_timeout(err, 0);
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "zero timeout must not wait for poll_interval; elapsed {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn integration_wait_for_completion_rejects_http_500() {
    let mut routes = HashMap::new();
    routes.insert(
        "/task/new",
        mock_ok(r#"{"success":true,"taskid":"mock-task"}"#),
    );
    routes.insert("/option/mock-task/set", mock_ok(r#"{"success":true}"#));
    routes.insert(
        "/scan/mock-task/status",
        mock_status(r#"{"success":false,"message":"server blew up"}"#, 500),
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
        .expect_err("HTTP 500 on status must fail");
    assert_api_error_contains(err, "HTTP 500");
}
