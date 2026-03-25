//! Orchestrator for the `sqlmapapi.py` subprocess and its RESTful interface.

use crate::error::SqlmapError;
use crate::types::{BasicResponse, DataResponse, NewTaskResponse, SqlmapOptions, StatusResponse};
use reqwest::Client;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::sleep;
use tracing::{debug, warn};

/// Manages the `sqlmapapi` lifecycle and provides access to its REST API.
pub struct SqlmapEngine {
    api_url: String,
    http: Client,
    _process: Option<Child>,
}

impl SqlmapEngine {
    /// Launches a local `sqlmapapi` daemon on a specific port, or connects to an existing remote one.
    pub async fn new(
        port: u16,
        spawn_local: bool,
        binary_path: Option<&str>,
    ) -> Result<Self, SqlmapError> {
        let mut _process = None;
        let api_url = format!("http://127.0.0.1:{}", port);
        
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        if spawn_local {
            let binary = binary_path.unwrap_or("sqlmapapi");
            
            let mut cmd = Command::new(binary);
            cmd.arg("-s")
               .arg("-H").arg("127.0.0.1")
               .arg("-p").arg(port.to_string())
               .kill_on_drop(true); // Law 4: Absolute deterministic teardown
               
            cmd.stdout(Stdio::null()).stderr(Stdio::null());

            _process = Some(cmd.spawn()?);

            // Wait for daemon to become responsive instead of blind sleeping.
            let mut ready = false;
            for _ in 0..20 {
                // If we can create and delete a task, the daemon is fully online.
                if let Ok(resp) = http.get(format!("{}/task/new", api_url)).send().await {
                    if let Ok(json) = resp.json::<NewTaskResponse>().await {
                        if let Some(task_id) = json.taskid {
                            let _ = http.get(format!("{}/task/{}/delete", api_url, task_id)).send().await;
                            ready = true;
                            break;
                        }
                    }
                }
                sleep(Duration::from_millis(250)).await;
            }

            if !ready {
                return Err(SqlmapError::ApiError("Daemon failed to boot within 5 seconds".into()));
            }
        }

        Ok(Self { api_url, http, _process })
    }

    /// Creates and configures a new scanning task, returning an RAII wrapper.
    pub async fn create_task(&self, options: &SqlmapOptions) -> Result<SqlmapTask<'_>, SqlmapError> {
        // 1. Create the task ID
        let uri = format!("{}/task/new", self.api_url);
        let resp = self.http.get(uri).send().await?.json::<NewTaskResponse>().await?;
        
        let task_id = if resp.success {
            resp.taskid.unwrap_or_default()
        } else {
            return Err(SqlmapError::ApiError(resp.message.unwrap_or_else(|| "Failed to create task".into())));
        };

        let task = SqlmapTask {
            engine: self,
            task_id,
        };

        // 2. Set the configuration options
        let set_uri = format!("{}/option/{}/set", self.api_url, task.task_id);
        // Sqlmap expects the dictionary directly as JSON body, NOT wrapped in {"options": ...}
        let set_resp = self.http.post(&set_uri).json(&options).send().await?.json::<BasicResponse>().await?;
        
        if !set_resp.success {
            return Err(SqlmapError::ApiError(set_resp.message.unwrap_or_else(|| "Option setup failed".into())));
        }

        Ok(task)
    }
}

/// An RAII tracked execution task. Ensures the Daemon memory is purged cleanly on Drop.
pub struct SqlmapTask<'a> {
    engine: &'a SqlmapEngine,
    task_id: String,
}

impl<'a> SqlmapTask<'a> {
    /// Starts the SQLMap fuzzing on this specific task.
    pub async fn start(&self) -> Result<(), SqlmapError> {
        let uri = format!("{}/scan/{}/start", self.engine.api_url, self.task_id);
        
        // Blank body since the URL is passed via `options`.
        let payload = serde_json::json!({});
        let resp = self.engine.http.post(&uri).json(&payload).send().await?.json::<BasicResponse>().await?;
        
        if !resp.success {
            return Err(SqlmapError::ApiError(resp.message.unwrap_or_else(|| "Failed to start engine".into())));
        }
        Ok(())
    }

    /// Polls the task status until completion. Returns an error on timeout.
    pub async fn wait_for_completion(&self, timeout_secs: u64) -> Result<(), SqlmapError> {
        let uri = format!("{}/scan/{}/status", self.engine.api_url, self.task_id);
        let start = std::time::Instant::now();
        
        loop {
            if start.elapsed().as_secs() > timeout_secs {
                return Err(SqlmapError::Timeout(timeout_secs));
            }

            let resp = self.engine.http.get(&uri).send().await?.json::<StatusResponse>().await?;
            if !resp.success {
                return Err(SqlmapError::ApiError("Failed to fetch task status".into()));
            }

            match resp.status.as_deref() {
                Some("running") => {
                    debug!("Task {} running...", self.task_id);
                }
                Some("terminated") => {
                     return Ok(());
                }
                Some(other) => {
                    warn!("Unknown sqlmap status string: {}", other);
                }
                None => {}
            }

            sleep(Duration::from_millis(3000)).await;
        }
    }

    /// Fetches the compiled data results from the engine.
    pub async fn fetch_data(&self) -> Result<DataResponse, SqlmapError> {
        let uri = format!("{}/scan/{}/data", self.engine.api_url, self.task_id);
        let resp = self.engine.http.get(uri).send().await?;
        
        if resp.status().is_success() {
            Ok(resp.json::<DataResponse>().await?)
        } else {
            Err(SqlmapError::ApiError(format!("Failed pulling data, status: {}", resp.status())))
        }
    }
}

impl<'a> Drop for SqlmapTask<'a> {
    fn drop(&mut self) {
        // Guarantee the server reclaims task memory when this struct goes out of scope.
        // We spawn it as a detached future since Drop cannot be asynchronous.
        let uri = format!("{}/task/{}/delete", self.engine.api_url, self.task_id);
        let client = self.engine.http.clone();
        
        tokio::spawn(async move {
            let _ = client.get(&uri).send().await;
        });
    }
}

impl Drop for SqlmapEngine {
    fn drop(&mut self) {
        if let Some(mut proc) = self._process.take() {
            let _ = proc.start_kill();
        }
    }
}
