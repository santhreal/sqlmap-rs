//! Complete example demonstrating all sqlmap-rs capabilities.
//!
//! Run with: cargo run --example full_scan -- <target-url>
//!
//! Prerequisites:
//!   conda env create -f environment.yml
//!   conda activate sqlmap-env
//!   # OR: ./setup.sh

use sqlmap_rs::{OutputFormat, SqlmapEngine, SqlmapOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Check availability ────────────────────────────
    if !SqlmapEngine::is_available() {
        eprintln!("ERROR: sqlmapapi not found in PATH");
        eprintln!("Quick fix:");
        eprintln!("  conda env create -f environment.yml");
        eprintln!("  conda activate sqlmap-env");
        eprintln!("  # OR: ./setup.sh");
        std::process::exit(1);
    }

    // ── 2. Boot the daemon ───────────────────────────────
    println!("Booting sqlmapapi daemon on port 8775...");
    let engine = SqlmapEngine::new(8775, true, None).await?;
    println!("Daemon ready at {}", engine.api_url());

    // ── 3. Configure scan with builder ───────────────────
    let target = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cargo run --example full_scan -- <target-url>");
        eprintln!("Example: cargo run --example full_scan -- http://example.com/page?id=1");
        std::process::exit(1);
    });

    println!("Target: {target}");

    let opts = SqlmapOptions::builder()
        .url(&target)
        .level(3)
        .risk(2)
        .batch(true)
        .threads(4)
        .random_agent(true)
        .build();

    // ── 4. Create and run task ───────────────────────────
    let task = engine.create_task(&opts).await?;
    println!("Task created: {}", task.task_id());

    task.start().await?;
    println!("Scan started, polling for completion...");

    // ── 5. Monitor execution ─────────────────────────────
    task.wait_for_completion(300).await?;
    println!("Scan complete!");

    // ── 6. Fetch and display logs ────────────────────────
    match task.fetch_log().await {
        Ok(log_resp) => {
            if let Some(logs) = &log_resp.log {
                println!("\n=== Scan Log ({} entries) ===", logs.len());
                for entry in logs.iter().rev().take(10) {
                    println!("  [{}] {}: {}", entry.time, entry.level, entry.message);
                }
                if logs.len() > 10 {
                    println!("  ... and {} more entries", logs.len() - 10);
                }
            }
        }
        Err(err) => eprintln!("Could not fetch log: {err}"),
    }

    // ── 7. Fetch results ─────────────────────────────────
    let data = task.fetch_data().await?;
    let findings = data.findings();

    println!("\n=== Results ===");
    println!("  Findings: {}", findings.len());

    if findings.is_empty() {
        println!("  No SQL injection vulnerabilities detected.");
    } else {
        // ── 8. Multi-format output ───────────────────────
        println!(
            "\n{}",
            sqlmap_rs::types::format_findings(&findings, OutputFormat::Plain)
        );

        println!("=== JSON ===");
        println!(
            "{}",
            sqlmap_rs::types::format_findings(&findings, OutputFormat::JsonPretty)
        );

        println!("=== CSV ===");
        println!(
            "{}",
            sqlmap_rs::types::format_findings(&findings, OutputFormat::Csv)
        );

        println!("=== Markdown ===");
        println!(
            "{}",
            sqlmap_rs::types::format_findings(&findings, OutputFormat::Markdown)
        );
    }

    // ── 9. Inspect configured options ────────────────────
    match task.list_options().await {
        Ok(options) => {
            println!("\n=== Active Options ===");
            println!("{}", serde_json::to_string_pretty(&options)?);
        }
        Err(err) => eprintln!("Could not fetch options: {err}"),
    }

    // Task is auto-deleted from daemon on drop.
    // Engine daemon is auto-killed on drop.
    println!("\nDone. Task and daemon will be cleaned up automatically.");

    Ok(())
}
