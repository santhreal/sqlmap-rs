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
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::sleep;
use tracing::{debug, warn};

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
    /// * `port` — TCP port for the daemon. If `0` is passed with `spawn_local`,
    ///   the OS assigns an ephemeral port (not yet supported by sqlmapapi).
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

        if spawn_local && port == 0 {
            return Err(SqlmapError::ApiError(
                "port 0 is not supported when spawning local sqlmapapi".into(),
            ));
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

            cmd.stdout(Stdio::null()).stderr(Stdio::null());

            daemon_process = Some(cmd.spawn().map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    SqlmapError::BinaryNotFound(binary.to_string())
                } else {
                    SqlmapError::ProcessError(e)
                }
            })?);

            // Wait for daemon to become responsive with a health probe.
            let mut ready = false;
            for attempt in 0..20 {
                if let Ok(resp) = http.get(format!("{api_url}/task/new")).send().await {
                    if let Ok(json) = resp.json::<NewTaskResponse>().await.map_err(map_json_error) {
                        if json.success {
                            if let Some(task_id) = json.taskid {
                                // Clean up the probe task.
                                let _ = http
                                    .get(format!("{api_url}/task/{task_id}/delete"))
                                    .send()
                                    .await;
                                ready = true;
                                break;
                            }
                        }
                    }
                }
                debug!(attempt, "waiting for sqlmapapi daemon to become ready");
                sleep(Duration::from_millis(250)).await;
            }

            if !ready {
                return Err(SqlmapError::ApiError(
                    "sqlmapapi daemon failed to become responsive within 5 seconds".into(),
                ));
            }
        }

        Ok(Self {
            api_url,
            http,
            daemon_process,
            poll_interval,
        })
    }

    /// Creates and configures a new scanning task, returning an RAII wrapper.
    ///
    /// The task is automatically deleted from the daemon when dropped.
    pub async fn create_task(
        &self,
        options: &SqlmapOptions,
    ) -> Result<SqlmapTask<'_>, SqlmapError> {
        let uri = format!("{}/task/new", self.api_url);
        let resp = self
            .http
            .get(uri)
            .send()
            .await?
            .json::<NewTaskResponse>()
            .await
            .map_err(map_json_error)?;

        if !resp.success {
            return Err(SqlmapError::ApiError(
                resp.message
                    .unwrap_or_else(|| "task creation returned success=false".into()),
            ));
        }

        let task_id = resp
            .taskid
            .filter(|id| !id.is_empty())
            .ok_or_else(|| SqlmapError::InvalidTask(String::new()))?;

        let task = SqlmapTask {
            engine: self,
            task_id,
        };

        // Set the configuration options on the new task.
        let set_uri = format!("{}/option/{}/set", self.api_url, task.task_id);
        let set_resp = self
            .http
            .post(&set_uri)
            .json(options)
            .send()
            .await?
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
        let start = std::time::Instant::now();

        loop {
            if start.elapsed().as_secs() > timeout_secs {
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
                    if let Some(code) = status.returncode {
                        if code != 0 {
                            return Err(SqlmapError::ApiError(format!(
                                "scan terminated with non-zero exit code {code}"
                            )));
                        }
                    }
                    return Ok(());
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
