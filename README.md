# sqlmap-rs

[![Crates.io](https://img.shields.io/crates/v/sqlmap-rs.svg)](https://crates.io/crates/sqlmap-rs)
[![Documentation](https://docs.rs/sqlmap-rs/badge.svg)](https://docs.rs/sqlmap-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Available on Crates.io:** [https://crates.io/crates/sqlmap-rs](https://crates.io/crates/sqlmap-rs)

A type-safe, asynchronous Rust orchestrator for the sqlmap SQL injection testing tool.

`sqlmap-rs` spawns sqlmap's native REST server (`sqlmapapi.py`) on **localhost** (`127.0.0.1`) and communicates via a strictly-typed Tokio JSON pipeline. Tasks use RAII-style cleanup on drop (best-effort; task deletion requires an active Tokio runtime).

## Features

- **Core API coverage** - start, stop, kill, log, data, option introspection
- **Builder pattern** - fluent `SqlmapOptions::builder()` with 40+ options
- **Multi-format output** - JSON, CSV, Markdown, and plain text
- **RAII lifecycle** - best-effort task cleanup on drop, daemon killed on engine drop
- **Port conflict detection** - prevents silent connection to wrong daemons
- **Configurable polling** - custom intervals and HTTP timeouts

## Installation

```toml
[dependencies]
sqlmap-rs = "0.3.1"
tokio = { version = "1", features = ["full"] }
```

*Prerequisite: `sqlmapapi` must be in your system `$PATH`.*

## Setup (one-command)

**Option A: Conda** (recommended for isolation)
```bash
conda env create -f environment.yml
conda activate sqlmap-env
```

**Option B: Setup script** (auto-detects or installs conda + sqlmap)
```bash
./setup.sh
# or with custom env name:
./setup.sh my-project-env
```

**Option C: Manual**
```bash
pip install sqlmap
# verify:
sqlmapapi -h
```

## Quick Start

```rust
use sqlmap_rs::{SqlmapEngine, SqlmapOptions, OutputFormat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Boot the daemon - auto-shut-down on drop
    let engine = SqlmapEngine::new(8775, true, None).await?;

    // 2. Configure scan with the builder pattern
    let opts = SqlmapOptions::builder()
        .url("http://example.com/api?id=1")
        .level(3)
        .risk(2)
        .batch(true)
        .threads(4)
        .build();

    // 3. Create and run the task
    let task = engine.create_task(&opts).await?;
    task.start().await?;
    task.wait_for_completion(300).await?;

    // 4. Fetch and format results
    let data = task.fetch_data().await?;
    let findings = data.findings();

    println!("{}", sqlmap_rs::types::format_findings(&findings, OutputFormat::Plain));

    Ok(())
}
```

## Scan Lifecycle Control

```rust
// Gracefully stop a running scan
task.stop().await?;

// Force-kill a scan
task.kill().await?;

// Retrieve execution logs
let logs = task.fetch_log().await?;

// Inspect configured options
let options = task.list_options().await?;
```

## Advanced Options

The builder covers 40+ sqlmap options including tamper scripts, Tor routing, crawling, second-order injection, and file I/O:

```rust
let opts = SqlmapOptions::builder()
    .url("http://target.com/page?id=1")
    .tamper("space2comment,between")
    .tor(true)
    .tor_port(9050)
    .crawl_depth(3)
    .second_url("http://target.com/result")
    .prefix("')")
    .suffix("-- -")
    .get_dbs(true)
    .dump_all(true)
    .build();
```

## Security & Memory

- **Task Drop**: When `SqlmapTask` leaves scope, a background task deletes the execution context from the daemon when a Tokio runtime is active. Without a runtime, cleanup is skipped (best-effort).
- **Engine Drop**: When `SqlmapEngine` is dropped, the daemon subprocess receives a kill signal (best-effort).
- **Port Safety**: The engine detects port conflicts before spawning, preventing accidental connection to unrelated services on the same port.
- **Localhost only**: The API client always targets `127.0.0.1`; there is no remote-daemon mode.

## License

MIT License
