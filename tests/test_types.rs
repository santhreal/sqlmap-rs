use sqlmap_rs::types::{DataResponse, NewTaskResponse, StatusResponse};
use sqlmap_rs::{OutputFormat, SqlmapFinding, SqlmapOptions};

#[test]
fn test_new_task_parsing() {
    let raw = r#"{"success": true, "taskid": "5f1a3b2k"}"#;
    let resp: NewTaskResponse = serde_json::from_str(raw).expect("parse");
    assert!(resp.success);
    assert_eq!(resp.taskid.as_deref(), Some("5f1a3b2k"));
}

#[test]
fn test_status_parsing() {
    let raw = r#"{"status": "running", "returncode": null, "success": true}"#;
    let resp: StatusResponse = serde_json::from_str(raw).expect("parse");
    assert!(resp.success);
    assert_eq!(resp.status.as_deref(), Some("running"));
    assert_eq!(resp.returncode, None);

    let raw_term = r#"{"status": "terminated", "returncode": 0, "success": true}"#;
    let term: StatusResponse = serde_json::from_str(raw_term).expect("parse");
    assert_eq!(term.status.as_deref(), Some("terminated"));
    assert_eq!(term.returncode, Some(0));
}

#[test]
fn test_data_extraction() {
    let raw = r#"{
        "data": [
            {
                "status": 1,
                "type": 1,
                "value": [
                    {
                        "dbms": "MySQL",
                        "dbms_version": [">= 5.0.0"],
                        "place": "GET",
                        "parameter": "id",
                        "ptype": 1,
                        "data": [
                            {
                                "title": "AND boolean-based blind - WHERE or HAVING clause",
                                "payload": "id=1 AND 8888=8888"
                            }
                        ]
                    }
                ]
            }
        ],
        "error": [],
        "success": true
    }"#;

    let resp: DataResponse = serde_json::from_str(raw).expect("parse");
    assert!(resp.success);

    let chunk = &resp.data.as_ref().expect("data")[0];
    assert_eq!(chunk.r#type, 1);

    let findings = resp.findings();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].parameter, "id");
    assert_eq!(
        findings[0].vulnerability_type,
        "AND boolean-based blind - WHERE or HAVING clause"
    );
    assert_eq!(findings[0].payload, "id=1 AND 8888=8888");
}

#[test]
fn test_nested_techniques_status_fixture_two_findings() {
    let raw = r#"{
        "data": [
            {
                "status": 1,
                "type": 1,
                "value": [
                    {
                        "place": "GET",
                        "parameter": "id",
                        "ptype": 1,
                        "data": [
                            {
                                "title": "AND boolean-based blind - WHERE or HAVING clause",
                                "payload": "id=1 AND 8888=8888"
                            },
                            {
                                "technique": "time-based blind",
                                "payload": "id=1 AND SLEEP(5)"
                            }
                        ]
                    }
                ]
            }
        ],
        "error": [],
        "success": true
    }"#;

    let resp: DataResponse = serde_json::from_str(raw).expect("parse");
    assert!(resp.success);

    let findings = resp.findings();
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].parameter, "id");
    assert_eq!(
        findings[0].vulnerability_type,
        "AND boolean-based blind - WHERE or HAVING clause"
    );
    assert_eq!(findings[0].payload, "id=1 AND 8888=8888");
    assert_eq!(findings[1].vulnerability_type, "time-based blind");
    assert_eq!(findings[1].payload, "id=1 AND SLEEP(5)");
}

#[test]
fn test_builder_api() {
    let opts = SqlmapOptions::builder()
        .url("http://test.com?id=1")
        .level(5)
        .risk(3)
        .batch(true)
        .threads(8)
        .tamper("space2comment,between")
        .tor(true)
        .tor_port(9050)
        .crawl_depth(3)
        .dump_all(true)
        .random_agent(true)
        .build();

    let json = serde_json::to_string(&opts).expect("serialize");
    assert!(json.contains("\"level\":5"));
    assert!(json.contains("\"risk\":3"));
    assert!(json.contains("\"tor\":true"));
    assert!(json.contains("\"torPort\":9050"));
    assert!(json.contains("\"crawlDepth\":3"));
    assert!(json.contains("\"dumpAll\":true"));
    assert!(json.contains("\"randomAgent\":true"));
}

#[test]
fn test_output_formats() {
    let findings = vec![SqlmapFinding::new(
        "id",
        "error-based",
        "' OR 1=1--",
        serde_json::json!({}),
    )];

    let json = sqlmap_rs::types::format_findings(&findings, OutputFormat::Json);
    assert!(json.contains("error-based"));

    let csv = sqlmap_rs::types::format_findings(&findings, OutputFormat::Csv);
    assert!(csv.starts_with("parameter,"));

    let md = sqlmap_rs::types::format_findings(&findings, OutputFormat::Markdown);
    assert!(md.contains("| Parameter |"));

    let plain = sqlmap_rs::types::format_findings(&findings, OutputFormat::Plain);
    assert!(plain.contains("error-based"));
}

#[test]
fn test_is_available_at_nonexistent_binary_is_false() {
    assert!(
        !sqlmap_rs::SqlmapEngine::is_available_at("/nonexistent-sqlmapapi-xyz"),
        "missing binary path must report unavailable"
    );
}
