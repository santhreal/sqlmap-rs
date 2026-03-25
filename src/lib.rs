//! # Sqlmap-RS
//!
//! An asynchronous, strictly-typed Rust wrapper for the `sqlmapapi` REST Server.
//!
//! Provides a panic-free API for orchestrating SQL injection scans using the
//! world's most powerful detection engine. Instead of parsing messy CLI outputs,
//! this library communicates via sqlmap's native REST API.
//!
//! ## Features
//!
//! - **Full API coverage**: start, stop, kill, log, data, option introspection.
//! - **Builder pattern**: Fluent `SqlmapOptions::builder()` with 40+ sqlmap options.
//! - **Multi-format output**: JSON, CSV, Markdown, and plain text.
//! - **RAII cleanup**: Tasks and daemon processes are cleaned up on drop.
//! - **Port conflict detection**: Prevents silent connection to wrong daemons.
//! - **Configurable polling**: Custom intervals and HTTP timeouts.
//!
//! ## Architecture
//!
//! The library spawns `sqlmapapi.py` under a Tokio background thread and uses
//! HTTP polling to track scan lifecycles. When the engine drops, the daemon
//! is killed. When a task drops, it is deleted from the daemon.

#![warn(missing_docs)]

/// REST API Client Orchestration.
pub mod client;
/// Typed Error Variants.
pub mod error;
/// Deserialization payloads, options builder, and output formatting.
pub mod types;

pub use client::SqlmapEngine;
pub use error::SqlmapError;
pub use types::{
    DataResponse, LogEntry, LogResponse, OutputFormat, SqlmapDataChunk, SqlmapFinding,
    SqlmapOptions, SqlmapOptionsBuilder,
};
