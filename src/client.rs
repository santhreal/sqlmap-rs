//! Orchestrator for the `sqlmapapi.py` subprocess and its RESTful interface.
//!
//! Manages the full daemon lifecycle — boot, health check, task creation,
//! scan execution, log retrieval, graceful stop/kill, and RAII cleanup.

use crate::error::SqlmapError;
use crate::types::{
    BasicResponse, DataResponse, LogResponse, NewTaskResponse, SqlmapOptions, StatusResponse,
};
use reqwest::Client;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, warn};

const SPAWN_STDERR_SNIPPET_MAX: usize = 512;

fn truncate_stderr_snippet(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let snippet = if trimmed.len() > SPAWN_STDERR_SNIPPET_MAX {
        format!("{}…", &trimmed[..SPAWN_STDERR_SNIPPET_MAX])
    } else {
        trimmed.to_string()
    };
    Some(snippet)
}

fn map_json_error(err: reqwest::Error) -> SqlmapError {
    if err.is_decode() {
        SqlmapError::MalformedResponse
    } else {
        SqlmapError::RequestError(err)
    }
}

/// Manages the `sqlmapapi` lifecycle and provides access to its REST API.
///
/// When the engine is dropped, the locally spawned daemon subprocess
/// is killed automatically via RAII (best-effort).
pub struct SqlmapEngine {
    api_url: String,
    http: Client,
    daemon_process: Option<Child>,
    /// Configurable polling interval for `wait_for_completion`.
    poll_interval: Duration,
}

impl SqlmapEngine {
    /// Launches a local `sqlmapapi` daemon bound to `127.0.0.1`.
    ///
    /// # Arguments
    ///
    /// * `port` — TCP port for the daemon. Port `0` is not supported.
    /// * `spawn_local` — If true, spawns a local `sqlmapapi` subprocess.
    /// * `binary_path` — Override the `sqlmapapi` binary location.
    ///
    /// # Errors
    ///
    /// Returns [`SqlmapError::ProcessError`] if the daemon fails to spawn,
    /// or [`SqlmapError::ApiError`] if it doesn't become responsive within 5 seconds.
    pub async fn new(
        port: u16,
        spawn_local: bool,
        binary_path: Option<&str>,
    ) -> Result<Self, SqlmapError> {
        Self::with_config(
            port,
            spawn_local,
            binary_path,
            Duration::from_secs(10),
            Duration::from_millis(1000),
        )
        .await
    }

    /// Launches a daemon with custom HTTP timeout and polling interval.
    ///
    /// # Arguments
    ///
    /// * `request_timeout` — HTTP request timeout for API calls.
    /// * `poll_interval` — Interval between status polls in `wait_for_completion`.
    pub async fn with_config(
        port: u16,
        spawn_local: bool,
        binary_path: Option<&str>,
        request_timeout: Duration,
        poll_interval: Duration,
    ) -> Result<Self, SqlmapError> {
        if poll_interval.is_zero() {
            return Err(SqlmapError::ApiError(
                "poll_interval must be greater than zero".into(),
            ));
        }

        if request_timeout.is_zero() {
            return Err(SqlmapError::ApiError(
                "request_timeout must be greater than zero".into(),
            ));
        }

        if port == 0 {
            return Err(SqlmapError::ApiError("port 0 is not supported".into()));
        }

        let mut daemon_process = None;
        let api_url = format!("http://127.0.0.1:{port}");

        let http = Client::builder().timeout(request_timeout).build()?;

        if spawn_local {
            // Check if port is already in use before spawning.
            if std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
                return Err(SqlmapError::PortConflict { port });
            }

            let binary = binary_path.unwrap_or("sqlmapapi");

            let mut cmd = Command::new(binary);
            cmd.arg("-s")
                .arg("-H")
                .arg("127.0.0.1")
                .arg("-p")
                .arg(port.to_string())
                .kill_on_drop(true);

            cmd.stdout(Stdio::null()).stderr(Stdio::piped());

            let mut child = cmd.spawn().map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    SqlmapError::BinaryNotFound(binary.to_string())
                } else {
                    SqlmapError::ProcessError(e)
                }
            })?;

            let stderr_capture = Arc::new(Mutex::new(Vec::new()));
            if let Some(mut stderr) = child.stderr.take() {
                let capture = Arc::clone(&stderr_capture);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => capture.lock().await.extend_from_slice(&buf[..n]),
                            Err(_) => break,
                        }
                    }
                });
            }

            daemon_process = Some(child);

            // Wait for daemon to become responsive with a health probe.
            let mut ready = false;
            for attempt in 0..20 {
                if Self::probe_task_new(&http, &api_url).await.is_ok() {
                    ready = true;
                    break;
                }
                debug!(attempt, "waiting for sqlmapapi daemon to become ready");
                sleep(Duration::from_millis(250)).await;
            }

            if !ready {
                let stderr_snippet = {
                    let bytes = stderr_capture.lock().await;
                    truncate_stderr_snippet(&bytes)
                };
                let mut msg =
                    "sqlmapapi daemon failed to become responsive within 5 seconds".to_string();
                if let Some(snippet) = stderr_snippet {
                    msg.push_str(": ");
                    msg.push_str(&snippet);
                }
                return Err(SqlmapError::ApiError(msg));
            }
        } else {
            Self::probe_task_new(&http, &api_url).await?;
        }

        Ok(Self {
            api_url,
            http,
            daemon_process,
            poll_interval,
        })
    }

    /// Probes `/task/new` and verifies the peer returns `success` + a non-empty `taskid`.
    async fn probe_task_new(http: &Client, api_url: &str) -> Result<(), SqlmapError> {
        let resp = http.get(format!("{api_url}/task/new")).send().await?;

        if !resp.status().is_success() {
            return Err(SqlmapError::ApiError(format!(
                "health probe returned HTTP {}",
                resp.status()
            )));
        }

        let json = resp
            .json::<NewTaskResponse>()
            .await
            .map_err(map_json_error)?;

        if !json.success {
            return Err(SqlmapError::ApiError(
                json.message
                    .unwrap_or_else(|| "health probe returned success=false".into()),
            ));
        }

        let task_id = json.taskid.filter(|id| !id.is_empty()).ok_or_else(|| {
            SqlmapError::ApiError("health probe: /task/new did not return a taskid".into())
        })?;

        let _ = http
            .get(format!("{api_url}/task/{task_id}/delete"))
            .send()
            .await;

        Ok(())
    }

    /// Creates and configures a new scanning task, returning an RAII wrapper.
    ///
    /// The task is automatically deleted from the daemon when dropped.
    pub async fn create_task(
        &self,
        options: &SqlmapOptions,
    ) -> Result<SqlmapTask<'_>, SqlmapError> {
        let uri = format!("{}/task/new", self.api_url);
        let resp = self.http.get(uri).send().await?;

        if !resp.status().is_success() {
            return Err(SqlmapError::ApiError(format!(
                "task creation returned HTTP {}",
                resp.status()
            )));
        }

        let resp = resp
            .json::<NewTaskResponse>()
            .await
            .map_err(map_json_error)?;

        if !resp.success {
            return Err(SqlmapError::ApiError(
                resp.message
                    .unwrap_or_else(|| "task creation returned success=false".into()),
            ));
        }

        let task_id = match resp.taskid {
            Some(id) if !id.is_empty() => id,
            Some(id) => {
                return Err(SqlmapError::InvalidTask(format!("empty taskid ({id:?})")));
            }
            None => {
                return Err(SqlmapError::InvalidTask("missing taskid".into()));
            }
        };

        let task = SqlmapTask {
            engine: self,
            task_id,
            user_stopped: Arc::new(AtomicBool::new(false)),
        };

        // Set the configuration options on the new task.
        let set_uri = format!("{}/option/{}/set", self.api_url, task.task_id);
        let set_resp = self.http.post(&set_uri).json(options).send().await?;

        if !set_resp.status().is_success() {
            return Err(SqlmapError::ApiError(format!(
                "option configuration returned HTTP {}",
                set_resp.status()
            )));
        }

        let set_resp = set_resp
            .json::<BasicResponse>()
            .await
            .map_err(map_json_error)?;

        if !set_resp.success {
            return Err(SqlmapError::ApiError(
                set_resp
                    .message
                    .unwrap_or_else(|| "option configuration failed".into()),
            ));
        }

        Ok(task)
    }

    /// Check if sqlmapapi is available on this system.
    ///
    /// Tests that the `sqlmapapi` binary exists and is executable.
    /// Does NOT fall back to `python3 -c "import sqlmap"` since that
    /// doesn't guarantee the REST API server is available.
    pub fn is_available() -> bool {
        std::process::Command::new("sqlmapapi")
            .arg("-h")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if sqlmapapi is available, trying the provided binary path first.
    pub fn is_available_at(binary_path: &str) -> bool {
        std::process::Command::new(binary_path)
            .arg("-h")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Returns the base API URL for this engine.
    pub fn api_url(&self) -> &str {
        &self.api_url
    }
}

impl Drop for SqlmapEngine {
    fn drop(&mut self) {
        if let Some(mut proc) = self.daemon_process.take() {
            let _ = proc.start_kill();
        }
    }
}

// ── SqlmapTask ───────────────────────────────────────────────────

/// An RAII-tracked scan execution task.
///
/// Ensures that the daemon reclaims task memory on drop by sending a
/// delete request when a Tokio runtime is available (best-effort).
pub struct SqlmapTask<'a> {
    engine: &'a SqlmapEngine,
    task_id: String,
    /// Set after intentional [`stop`](Self::stop) or [`kill`](Self::kill).
    user_stopped: Arc<AtomicBool>,
}

impl<'a> SqlmapTask<'a> {
    /// Returns the unique task ID assigned by the daemon.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Starts the SQL injection scan on this task.
    ///
    /// The URL and options must have been configured via [`SqlmapEngine::create_task`].
    pub async fn start(&self) -> Result<(), SqlmapError> {
        let uri = format!("{}/scan/{}/start", self.engine.api_url, self.task_id);
        let payload = serde_json::json!({});
        let resp = self.engine.http.post(&uri).json(&payload).send().await?;

        if resp.status().is_success() {
            let body = resp.json::<BasicResponse>().await.map_err(map_json_error)?;
            if !body.success {
                return Err(SqlmapError::ApiError(
                    body.message
                        .unwrap_or_else(|| "scan start returned success=false".into()),
                ));
            }
            Ok(())
        } else {
            Err(SqlmapError::ApiError(format!(
                "scan start returned HTTP {}",
                resp.status()
            )))
        }
    }

    /// Polls the task status until completion or timeout.
    ///
    /// Uses the engine's configured poll interval (default: 1 second).
    pub async fn wait_for_completion(&self, timeout_secs: u64) -> Result<(), SqlmapError> {
        let uri = format!("{}/scan/{}/status", self.engine.api_url, self.task_id);
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);

        loop {
            if std::time::Instant::now() >= deadline {
                return Err(SqlmapError::Timeout(timeout_secs));
            }

            let resp = self.engine.http.get(&uri).send().await?;

            if !resp.status().is_success() {
                return Err(SqlmapError::ApiError(format!(
                    "status check returned HTTP {}",
                    resp.status()
                )));
            }

            let status = resp
                .json::<StatusResponse>()
                .await
                .map_err(map_json_error)?;

            if !status.success {
                return Err(SqlmapError::ApiError(
                    status
                        .message
                        .unwrap_or_else(|| "status check returned success=false".into()),
                ));
            }

            match status.status.as_deref() {
                Some("running") => {
                    debug!(task_id = %self.task_id, "scan running");
                }
                Some("terminated") => {
                    if self.user_stopped.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    match status.returncode {
                        Some(0) => return Ok(()),
                        Some(code) => {
                            return Err(SqlmapError::ApiError(format!(
                                "scan terminated with non-zero exit code {code}"
                            )));
                        }
                        None => {
                            return Err(SqlmapError::ApiError(
                                "scan terminated without a process exit code".into(),
                            ));
                        }
                    }
                }
                Some("not running") => {
                    // sqlmapapi reports "not running" before start attaches a
                    // process AND after some finished states. Keep polling;
                    // only `terminated` is a definitive completion.
                    debug!(task_id = %self.task_id, "scan not running yet");
                }
                Some(other) => {
                    warn!(task_id = %self.task_id, status = %other, "unknown sqlmap status");
                }
                None => {}
            }

            sleep(self.engine.poll_interval).await;
        }
    }

    /// Fetches the compiled data results from the engine.
    pub async fn fetch_data(&self) -> Result<DataResponse, SqlmapError> {
        let uri = format!("{}/scan/{}/data", self.engine.api_url, self.task_id);
        let resp = self.engine.http.get(uri).send().await?;

        if resp.status().is_success() {
            let data = resp.json::<DataResponse>().await.map_err(map_json_error)?;
            if !data.success {
                return Err(SqlmapError::ApiError(
                    data.message
                        .unwrap_or_else(|| "data fetch returned success=false".into()),
                ));
            }
            Ok(data)
        } else {
            Err(SqlmapError::ApiError(format!(
                "data fetch returned HTTP {}",
                resp.status()
            )))
        }
    }

    /// Fetches execution log entries for this task.
    ///
    /// Useful for monitoring what sqlmap is doing during a scan.
    pub async fn fetch_log(&self) -> Result<LogResponse, SqlmapError> {
        let uri = format!("{}/scan/{}/log", self.engine.api_url, self.task_id);
        let resp = self.engine.http.get(uri).send().await?;

        if resp.status().is_success() {
            let log = resp.json::<LogResponse>().await.map_err(map_json_error)?;
            if !log.success {
                return Err(SqlmapError::ApiError(
                    log.message
                        .unwrap_or_else(|| "log fetch returned success=false".into()),
                ));
            }
            Ok(log)
        } else {
            Err(SqlmapError::ApiError(format!(
                "log fetch returned HTTP {}",
                resp.status()
            )))
        }
    }

    /// Gracefully stops a running scan.
    ///
    /// The task can potentially be restarted after stopping.
    pub async fn stop(&self) -> Result<(), SqlmapError> {
        let uri = format!("{}/scan/{}/stop", self.engine.api_url, self.task_id);
        let resp = self.engine.http.get(uri).send().await?;

        if resp.status().is_success() {
            let body = resp.json::<BasicResponse>().await.map_err(map_json_error)?;
            if !body.success {
                return Err(SqlmapError::ApiError(
                    body.message
                        .unwrap_or_else(|| "scan stop returned success=false".into()),
                ));
            }
            self.user_stopped.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            Err(SqlmapError::ApiError(format!(
                "scan stop returned HTTP {}",
                resp.status()
            )))
        }
    }

    /// Forcefully kills a running scan.
    ///
    /// The task is terminated immediately. Data collected up to this point
    /// may still be retrievable via [`fetch_data`](Self::fetch_data).
    pub async fn kill(&self) -> Result<(), SqlmapError> {
        let uri = format!("{}/scan/{}/kill", self.engine.api_url, self.task_id);
        let resp = self.engine.http.get(uri).send().await?;

        if resp.status().is_success() {
            let body = resp.json::<BasicResponse>().await.map_err(map_json_error)?;
            if !body.success {
                return Err(SqlmapError::ApiError(
                    body.message
                        .unwrap_or_else(|| "scan kill returned success=false".into()),
                ));
            }
            self.user_stopped.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            Err(SqlmapError::ApiError(format!(
                "scan kill returned HTTP {}",
                resp.status()
            )))
        }
    }

    /// Retrieves the current option values configured for this task.
    pub async fn list_options(&self) -> Result<serde_json::Value, SqlmapError> {
        let uri = format!("{}/option/{}/list", self.engine.api_url, self.task_id);
        let resp = self.engine.http.get(uri).send().await?;

        if resp.status().is_success() {
            let value = resp
                .json::<serde_json::Value>()
                .await
                .map_err(map_json_error)?;
            if value
                .get("success")
                .and_then(|v| v.as_bool())
                .is_some_and(|success| !success)
            {
                let message = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "option list returned success=false".into());
                return Err(SqlmapError::ApiError(message));
            }
            Ok(value)
        } else {
            Err(SqlmapError::ApiError(format!(
                "option list returned HTTP {}",
                resp.status()
            )))
        }
    }
}

impl<'a> Drop for SqlmapTask<'a> {
    fn drop(&mut self) {
        // Guarantee the server reclaims task memory when this struct goes out of scope.
        // We use Handle::try_current() to avoid panicking if no Tokio runtime is active.
        let uri = format!("{}/task/{}/delete", self.engine.api_url, self.task_id);
        let client = self.engine.http.clone();

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = client.get(&uri).send().await;
            });
        }
        // If no runtime is available, we skip cleanup silently.
        // The daemon will reclaim the task when it shuts down.
    }
}
