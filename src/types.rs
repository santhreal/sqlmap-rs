//! Core type definitions for SQLMap REST API (`sqlmapapi`) payloads.
//!
//! Provides strictly-typed request/response structures, a comprehensive
//! options builder, and multi-format output for scan results.

use serde::{Deserialize, Serialize};
use std::fmt;

// ── Response types ───────────────────────────────────────────────

/// Response when creating a new task.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct NewTaskResponse {
    /// True if task creation succeeded.
    pub success: bool,
    /// The unique execution ID assigned to this task.
    pub taskid: Option<String>,
    /// Error or informational message.
    pub message: Option<String>,
}

/// Generic success response from the API.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct BasicResponse {
    /// True if operation succeeded.
    pub success: bool,
    /// Detailed message.
    pub message: Option<String>,
}

/// Response containing current execution status.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct StatusResponse {
    /// True if request succeeded.
    pub success: bool,
    /// Current engine status ("running", "terminated", etc.).
    pub status: Option<String>,
    /// Underlying process exit code (populated on termination).
    pub returncode: Option<i32>,
}

/// A chunk of extracted data reported by the SQLMap engine.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct SqlmapDataChunk {
    /// Chunk type: 0 (log), 1 (vulnerabilities), 2 (target info), etc.
    pub r#type: i32,
    /// The actual JSON payload chunk.
    pub value: serde_json::Value,
}

/// Final payload block returning all gathered data for a task.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct DataResponse {
    /// True if fetch succeeded.
    pub success: bool,
    /// The aggregated data chunks representing injection results.
    pub data: Option<Vec<SqlmapDataChunk>>,
    /// Array of structured errors from the engine.
    pub error: Option<Vec<String>>,
}

/// A log entry from the sqlmap scan execution.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct LogEntry {
    /// Log message text.
    pub message: String,
    /// Log level string (e.g. "INFO", "WARNING", "ERROR").
    pub level: String,
    /// Timestamp of the log entry.
    pub time: String,
}

/// Response from the log endpoint.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct LogResponse {
    /// True if fetch succeeded.
    pub success: bool,
    /// Array of log entries.
    pub log: Option<Vec<LogEntry>>,
}

// ── Finding types ────────────────────────────────────────────────

/// A parsed finding representing a confirmed SQL injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SqlmapFinding {
    /// Original parameter attacked.
    pub parameter: String,
    /// Classification of injection technique.
    pub vulnerability_type: String,
    /// Raw payload executed against the target.
    pub payload: String,
    /// Arbitrary engine output block with full details.
    pub details: serde_json::Value,
}

impl SqlmapFinding {
    /// Creates a new finding with the given fields.
    pub fn new(
        parameter: impl Into<String>,
        vulnerability_type: impl Into<String>,
        payload: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            parameter: parameter.into(),
            vulnerability_type: vulnerability_type.into(),
            payload: payload.into(),
            details,
        }
    }
}

impl fmt::Display for SqlmapFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[SQLi] {vtype} on param '{param}' — payload: {payload}",
            vtype = self.vulnerability_type,
            param = self.parameter,
            payload = self.payload,
        )
    }
}

impl DataResponse {
    /// Extract structured findings from the raw data chunks.
    ///
    /// Type 1 chunks contain vulnerability data. This parses them into
    /// `SqlmapFinding` structs with parameter, type, payload, and details.
    pub fn findings(&self) -> Vec<SqlmapFinding> {
        let Some(ref chunks) = self.data else {
            return vec![];
        };
        let mut findings = Vec::new();

        for chunk in chunks {
            if chunk.r#type == 1 {
                if let Some(arr) = chunk.value.as_array() {
                    for item in arr {
                        if let Some(obj) = item.as_object() {
                            let parameter = obj
                                .get("parameter")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let vulnerability_type = obj
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let payload = obj
                                .get("payload")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            findings.push(SqlmapFinding {
                                parameter,
                                vulnerability_type,
                                payload,
                                details: item.clone(),
                            });
                        }
                    }
                }
            }
        }

        findings
    }
}

// ── Output formatting ────────────────────────────────────────────

/// Supported output formats for scan results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OutputFormat {
    /// Compact single-line JSON.
    Json,
    /// Pretty-printed JSON.
    JsonPretty,
    /// Comma-separated values with header row.
    Csv,
    /// GitHub-flavored Markdown table.
    Markdown,
    /// Human-readable plain text report.
    Plain,
}

/// Format a slice of findings in the specified output format.
pub fn format_findings(findings: &[SqlmapFinding], format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => match serde_json::to_string(findings) {
            Ok(json) => json,
            Err(err) => format!("{{\"error\": \"serialization failed: {err}\"}}"),
        },
        OutputFormat::JsonPretty => match serde_json::to_string_pretty(findings) {
            Ok(json) => json,
            Err(err) => format!("{{\"error\": \"serialization failed: {err}\"}}"),
        },
        OutputFormat::Csv => {
            let mut buf = String::from("parameter,vulnerability_type,payload\n");
            for f in findings {
                buf.push_str(&format!(
                    "{},{},{}\n",
                    csv_escape(&f.parameter),
                    csv_escape(&f.vulnerability_type),
                    csv_escape(&f.payload),
                ));
            }
            buf
        }
        OutputFormat::Markdown => {
            if findings.is_empty() {
                return "No SQL injection findings.\n".to_string();
            }
            let mut buf = String::from("| Parameter | Type | Payload |\n");
            buf.push_str("|-----------|------|----------|\n");
            for f in findings {
                buf.push_str(&format!(
                    "| `{}` | {} | `{}` |\n",
                    f.parameter,
                    f.vulnerability_type,
                    f.payload.replace('|', "\\|"),
                ));
            }
            buf
        }
        OutputFormat::Plain => {
            if findings.is_empty() {
                return "No SQL injection findings detected.\n".to_string();
            }
            let mut buf = format!("=== {} SQLi Finding(s) ===\n\n", findings.len());
            for (i, f) in findings.iter().enumerate() {
                buf.push_str(&format!(
                    "#{} {} on param '{}'\n  Payload: {}\n\n",
                    i + 1,
                    f.vulnerability_type,
                    f.parameter,
                    f.payload,
                ));
            }
            buf
        }
    }
}

/// Escape a value for CSV output.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

// ── SqlmapOptions ────────────────────────────────────────────────

/// Configuration payload mapped directly to SQLMap CLI arguments.
///
/// All fields are optional and use `skip_serializing_if` so only
/// explicitly set values are sent to the REST API.
///
/// # Examples
///
/// ```rust
/// use sqlmap_rs::SqlmapOptions;
///
/// let opts = SqlmapOptions::builder()
///     .url("http://example.com/api?id=1")
///     .level(3)
///     .risk(2)
///     .batch(true)
///     .threads(4)
///     .build();
/// ```
#[derive(Debug, Clone, Serialize, Default)]
#[non_exhaustive]
pub struct SqlmapOptions {
    // ── Target ──
    /// The target URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Target specific parameter(s), e.g. "id".
    #[serde(rename = "testParameter", skip_serializing_if = "Option::is_none")]
    pub test_parameter: Option<String>,

    // ── Detection ──
    /// Specific DBMS backend, e.g. "MySQL".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dbms: Option<String>,

    /// Payload techniques to test (B=Boolean, T=Time, E=Error, U=UNION, S=Stacked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tech: Option<String>,

    /// Level of tests to perform (1-5, default 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,

    /// Payload risk (1-3, default 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<i32>,

    /// String to match for True on boolean-based blind injection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string: Option<String>,

    /// String to match for False on boolean-based blind injection.
    #[serde(rename = "notString", skip_serializing_if = "Option::is_none")]
    pub not_string: Option<String>,

    /// Regex to match for True on boolean-based blind injection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regexp: Option<String>,

    /// HTTP code to match for True query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,

    /// Compare responses using text only.
    #[serde(rename = "textOnly", skip_serializing_if = "Option::is_none")]
    pub text_only: Option<bool>,

    /// Compare responses using titles only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub titles: Option<bool>,

    // ── Request ──
    /// HTTP Cookie header value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,

    /// HTTP headers string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<String>,

    /// Force specific HTTP method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,

    /// POST data string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,

    /// Use randomly selected User-Agent.
    #[serde(rename = "randomAgent", skip_serializing_if = "Option::is_none")]
    pub random_agent: Option<bool>,

    /// HTTP proxy URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,

    // ── Injection ──
    /// Injection payload prefix string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,

    /// Injection payload suffix string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,

    /// Tamper script(s) for WAF evasion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tamper: Option<String>,

    /// Skip testing specific parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<String>,

    /// Skip testing parameters that appear static.
    #[serde(rename = "skipStatic", skip_serializing_if = "Option::is_none")]
    pub skip_static: Option<bool>,

    // ── Performance ──
    /// Number of concurrent threads (default 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<i32>,

    /// Output verbosity level (1-6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose: Option<i32>,

    /// Do not ask for user input (must be true for automation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<bool>,

    /// Number of retries on connection timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,

    // ── Enumeration ──
    /// Enumerate DBMS databases.
    #[serde(rename = "getDbs", skip_serializing_if = "Option::is_none")]
    pub get_dbs: Option<bool>,

    /// Enumerate DBMS database tables.
    #[serde(rename = "getTables", skip_serializing_if = "Option::is_none")]
    pub get_tables: Option<bool>,

    /// Enumerate DBMS database columns.
    #[serde(rename = "getColumns", skip_serializing_if = "Option::is_none")]
    pub get_columns: Option<bool>,

    /// Enumerate DBMS users.
    #[serde(rename = "getUsers", skip_serializing_if = "Option::is_none")]
    pub get_users: Option<bool>,

    /// Enumerate DBMS users password hashes.
    #[serde(rename = "getPasswordHashes", skip_serializing_if = "Option::is_none")]
    pub get_passwords: Option<bool>,

    /// Enumerate DBMS users privileges.
    #[serde(rename = "getPrivileges", skip_serializing_if = "Option::is_none")]
    pub get_privileges: Option<bool>,

    /// Check if the DBMS user is DBA.
    #[serde(rename = "isDba", skip_serializing_if = "Option::is_none")]
    pub is_dba: Option<bool>,

    /// Retrieve the current DBMS user.
    #[serde(rename = "getCurrentUser", skip_serializing_if = "Option::is_none")]
    pub current_user: Option<bool>,

    /// Retrieve the current DBMS database.
    #[serde(rename = "getCurrentDb", skip_serializing_if = "Option::is_none")]
    pub current_db: Option<bool>,

    /// Dump all DBMS databases tables entries.
    #[serde(rename = "dumpAll", skip_serializing_if = "Option::is_none")]
    pub dump_all: Option<bool>,

    /// Dump DBMS database table entries.
    #[serde(rename = "dumpTable", skip_serializing_if = "Option::is_none")]
    pub dump_table: Option<bool>,

    /// Search for database/table/column names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<bool>,

    // ── OS Access ──
    /// Prompt for an interactive OS shell.
    #[serde(rename = "osShell", skip_serializing_if = "Option::is_none")]
    pub os_shell: Option<bool>,

    /// Prompt for an interactive SQL shell.
    #[serde(rename = "sqlShell", skip_serializing_if = "Option::is_none")]
    pub sql_shell: Option<bool>,

    /// Read a file from the DBMS file system.
    #[serde(rename = "fileRead", skip_serializing_if = "Option::is_none")]
    pub file_read: Option<String>,

    /// Write a file to the DBMS file system.
    #[serde(rename = "fileWrite", skip_serializing_if = "Option::is_none")]
    pub file_write: Option<String>,

    /// Destination path for file write on the DBMS.
    #[serde(rename = "fileDest", skip_serializing_if = "Option::is_none")]
    pub file_dest: Option<String>,

    // ── Networking ──
    /// Use Tor for anonymity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tor: Option<bool>,

    /// Tor proxy port.
    #[serde(rename = "torPort", skip_serializing_if = "Option::is_none")]
    pub tor_port: Option<i32>,

    /// Tor proxy type (HTTP, SOCKS4, SOCKS5).
    #[serde(rename = "torType", skip_serializing_if = "Option::is_none")]
    pub tor_type: Option<String>,

    // ── Crawling ──
    /// Crawl the website from the target URL to given depth.
    #[serde(rename = "crawlDepth", skip_serializing_if = "Option::is_none")]
    pub crawl_depth: Option<i32>,

    /// Regex to filter target URLs during crawling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Parse and test forms on target pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forms: Option<bool>,

    // ── Second-order ──
    /// URL for second-order injection verification.
    #[serde(rename = "secondUrl", skip_serializing_if = "Option::is_none")]
    pub second_url: Option<String>,
}

/// Builder for constructing [`SqlmapOptions`] with a fluent API.
///
/// Every field has a corresponding setter method. Call [`.build()`](SqlmapOptionsBuilder::build)
/// to finalize.
#[derive(Debug, Clone, Default)]
pub struct SqlmapOptionsBuilder {
    inner: SqlmapOptions,
}

impl SqlmapOptions {
    /// Create a new options builder.
    pub fn builder() -> SqlmapOptionsBuilder {
        SqlmapOptionsBuilder::default()
    }
}

/// Macro to generate builder methods for Option<T> fields.
macro_rules! builder_method {
    ($name:ident, $field:ident, String) => {
        /// Sets the `$name` option.
        pub fn $name(mut self, value: impl Into<String>) -> Self {
            self.inner.$field = Some(value.into());
            self
        }
    };
    ($name:ident, $field:ident, bool) => {
        /// Sets the `$name` option.
        pub fn $name(mut self, value: bool) -> Self {
            self.inner.$field = Some(value);
            self
        }
    };
    ($name:ident, $field:ident, i32) => {
        /// Sets the `$name` option.
        pub fn $name(mut self, value: i32) -> Self {
            self.inner.$field = Some(value);
            self
        }
    };
}

impl SqlmapOptionsBuilder {
    // Target
    builder_method!(url, url, String);
    builder_method!(test_parameter, test_parameter, String);

    // Detection
    builder_method!(dbms, dbms, String);
    builder_method!(tech, tech, String);
    builder_method!(level, level, i32);
    builder_method!(risk, risk, i32);
    builder_method!(string, string, String);
    builder_method!(not_string, not_string, String);
    builder_method!(regexp, regexp, String);
    builder_method!(code, code, i32);
    builder_method!(text_only, text_only, bool);
    builder_method!(titles, titles, bool);

    // Request
    builder_method!(cookie, cookie, String);
    builder_method!(headers, headers, String);
    builder_method!(method, method, String);
    builder_method!(data, data, String);
    builder_method!(random_agent, random_agent, bool);
    builder_method!(proxy, proxy, String);

    // Injection
    builder_method!(prefix, prefix, String);
    builder_method!(suffix, suffix, String);
    builder_method!(tamper, tamper, String);
    builder_method!(skip, skip, String);
    builder_method!(skip_static, skip_static, bool);

    // Performance
    builder_method!(threads, threads, i32);
    builder_method!(verbose, verbose, i32);
    builder_method!(batch, batch, bool);
    builder_method!(retries, retries, i32);

    // Enumeration
    builder_method!(get_dbs, get_dbs, bool);
    builder_method!(get_tables, get_tables, bool);
    builder_method!(get_columns, get_columns, bool);
    builder_method!(get_users, get_users, bool);
    builder_method!(get_passwords, get_passwords, bool);
    builder_method!(get_privileges, get_privileges, bool);
    builder_method!(is_dba, is_dba, bool);
    builder_method!(current_user, current_user, bool);
    builder_method!(current_db, current_db, bool);
    builder_method!(dump_all, dump_all, bool);
    builder_method!(dump_table, dump_table, bool);
    builder_method!(search, search, bool);

    // OS Access
    builder_method!(os_shell, os_shell, bool);
    builder_method!(sql_shell, sql_shell, bool);
    builder_method!(file_read, file_read, String);
    builder_method!(file_write, file_write, String);
    builder_method!(file_dest, file_dest, String);

    // Networking
    builder_method!(tor, tor, bool);
    builder_method!(tor_port, tor_port, i32);
    builder_method!(tor_type, tor_type, String);

    // Crawling
    builder_method!(crawl_depth, crawl_depth, i32);
    builder_method!(scope, scope, String);
    builder_method!(forms, forms, bool);

    // Second-order
    builder_method!(second_url, second_url, String);

    /// Finalize and return the configured [`SqlmapOptions`].
    pub fn build(self) -> SqlmapOptions {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data_response_gives_no_findings() {
        let resp = DataResponse {
            success: true,
            data: None,
            error: None,
        };
        assert!(resp.findings().is_empty());
    }

    #[test]
    fn type_0_chunks_ignored() {
        let resp = DataResponse {
            success: true,
            data: Some(vec![SqlmapDataChunk {
                r#type: 0,
                value: serde_json::json!("log message"),
            }]),
            error: None,
        };
        assert!(resp.findings().is_empty());
    }

    #[test]
    fn type_1_chunk_parsed_as_finding() {
        let resp = DataResponse {
            success: true,
            data: Some(vec![SqlmapDataChunk {
                r#type: 1,
                value: serde_json::json!([{
                    "parameter": "id",
                    "type": "boolean-based blind",
                    "payload": "id=1 AND 1=1"
                }]),
            }]),
            error: None,
        };
        let findings = resp.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].parameter, "id");
        assert_eq!(findings[0].vulnerability_type, "boolean-based blind");
    }

    #[test]
    fn builder_pattern_serializes_correctly() {
        let opts = SqlmapOptions::builder()
            .url("http://test.com?id=1")
            .level(3)
            .risk(2)
            .batch(true)
            .threads(4)
            .tamper("space2comment")
            .build();
        let json = serde_json::to_string(&opts).expect("serialize");
        assert!(json.contains("http://test.com"));
        assert!(json.contains("\"level\":3"));
        assert!(json.contains("\"threads\":4"));
        assert!(json.contains("space2comment"));
        // None fields should be skipped.
        assert!(!json.contains("dbms"));
    }

    #[test]
    fn type_1_chunk_edge_cases() {
        let resp = DataResponse {
            success: true,
            data: Some(vec![SqlmapDataChunk {
                r#type: 1,
                value: serde_json::json!([
                    { "parameter": "username" },
                    "string_instead_of_object_should_be_ignored",
                    { "type": "error-based" }
                ]),
            }]),
            error: None,
        };
        let findings = resp.findings();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].parameter, "username");
        assert_eq!(findings[0].vulnerability_type, "unknown");
        assert_eq!(findings[1].parameter, "unknown");
        assert_eq!(findings[1].vulnerability_type, "error-based");
    }

    #[test]
    fn new_options_fields_serialize() {
        let opts = SqlmapOptions::builder()
            .tor(true)
            .tor_port(9050)
            .tor_type("SOCKS5")
            .crawl_depth(3)
            .second_url("http://verify.com")
            .tamper("between,randomcase")
            .retries(5)
            .dump_all(true)
            .file_read("/etc/passwd")
            .build();
        let json = serde_json::to_string(&opts).expect("serialize");
        assert!(json.contains("\"tor\":true"));
        assert!(json.contains("\"torPort\":9050"));
        assert!(json.contains("\"crawlDepth\":3"));
        assert!(json.contains("\"secondUrl\""));
        assert!(json.contains("\"fileRead\""));
        assert!(json.contains("\"dumpAll\":true"));
    }

    #[test]
    fn finding_display() {
        let finding = SqlmapFinding {
            parameter: "id".into(),
            vulnerability_type: "boolean-based blind".into(),
            payload: "id=1 AND 1=1".into(),
            details: serde_json::json!({}),
        };
        let display = format!("{finding}");
        assert!(display.contains("boolean-based blind"));
        assert!(display.contains("id"));
    }

    #[test]
    fn format_csv_output() {
        let findings = vec![SqlmapFinding {
            parameter: "id".into(),
            vulnerability_type: "error-based".into(),
            payload: "' OR 1=1--".into(),
            details: serde_json::json!({}),
        }];
        let csv = format_findings(&findings, OutputFormat::Csv);
        assert!(csv.starts_with("parameter,vulnerability_type,payload\n"));
        assert!(csv.contains("error-based"));
    }

    #[test]
    fn format_plain_empty() {
        let plain = format_findings(&[], OutputFormat::Plain);
        assert_eq!(plain, "No SQL injection findings detected.\n");
    }

    #[test]
    fn format_markdown_output() {
        let findings = vec![SqlmapFinding {
            parameter: "id".into(),
            vulnerability_type: "UNION query".into(),
            payload: "id=1 UNION SELECT 1,2--".into(),
            details: serde_json::json!({}),
        }];
        let md = format_findings(&findings, OutputFormat::Markdown);
        assert!(md.contains("| Parameter |"));
        assert!(md.contains("UNION query"));
    }
}
