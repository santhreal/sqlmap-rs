# sqlmap-rs

[![Crates.io](https://img.shields.io/crates/v/sqlmap-rs.svg)](https://crates.io/crates/sqlmap-rs)
[![Documentation](https://docs.rs/sqlmap-rs/badge.svg)](https://docs.rs/sqlmap-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Available on Crates.io:** [https://crates.io/crates/sqlmap-rs](https://crates.io/crates/sqlmap-rs)

A type-safe, asynchronous Rust orchestrator for the world's most powerful SQL injection testing tool.

Instead of parsing messy command-line outputs, `sqlmap-rs` spawns Sqlmap's native REST server (`sqlmapapi.py`) in the background and communicates via a strictly typed Tokio JSON pipeline. This allows you to launch thousands of concurrent fuzzing tasks absolutely panic-free, securely mapping memory to RAII drops.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
sqlmap-rs = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

*Prerequisite: `python3` and `sqlmap` (specifically `sqlmapapi`) must be in your system `$PATH`.*

## Quick Start

```rust
use sqlmap_rs::{SqlmapEngine, SqlmapOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Boot the daemon locally on port 8775. It automatically shuts down when `engine` drops.
    let engine = SqlmapEngine::new(8775, true, None).await?;

    // 2. Request an isolated scanning task identifier
    let task = engine.create_task(&SqlmapOptions {
        url: Some("http://example.com/api?id=1".into()),
        level: Some(3),
        risk: Some(2),
        ..Default::default()
    }).await?;

    // 3. Fire the payload testing
    task.start().await?;

    // 4. Poll and wait for completion securely
    task.wait_for_completion(300).await?;

    // 5. Extract results structured in strictly typed JSON structs
    let results = task.fetch_data().await?;
    if let Some(data) = results.data {
        println!("Found {} blocks of injections!", data.len());
    }

    Ok(())
}
```

## Security & Memory

This binding follows modern Rust system safety patterns:
- When the `SqlmapTask` leaves scope, a silent background Tokio thread automatically reclaims the memory on the Python daemon by deleting the specific execution context.
- When the `SqlmapEngine` leaves scope at the end of your program, the `std::process::Child` is sent a kill signal and immediately wiped from the host system, guaranteeing zero orphaned daemon processes.

## License

MIT License
