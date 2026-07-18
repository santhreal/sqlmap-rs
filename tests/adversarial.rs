//! Adversarial tests for findings parsing and output formatting.
use serde_json::{json, Value};
use sqlmap_rs::types::{format_findings, DataResponse, OutputFormat, SqlmapFinding};

fn data_response_from_json(value: Value) -> DataResponse {
    serde_json::from_value(value).expect("DataResponse fixture")
}

#[test]
fn adversarial_empty_data_array_yields_no_findings() {
    let resp = data_response_from_json(json!({
        "success": true,
        "data": [{ "type": 1, "value": [] }],
        "error": null,
        "message": null
    }));
    assert!(
        resp.findings().is_empty(),
        "empty TECHNIQUES array must not fabricate findings"
    );
}

#[test]
fn adversarial_nullish_chunk_value_shapes() {
    let nullish_values = [
        Value::Null,
        json!("not-an-array"),
        json!(42),
        json!({}),
        json!([null, 1, true, []]),
    ];

    for value in nullish_values {
        let resp = data_response_from_json(json!({
            "success": true,
            "data": [{ "type": 1, "value": value }],
            "error": null,
            "message": null
        }));
        assert!(
            resp.findings().is_empty(),
            "nullish type-1 value must not yield findings"
        );
    }
}

#[test]
fn adversarial_incomplete_parameter_only_object_not_fabricated() {
    let resp = data_response_from_json(json!({
        "success": true,
        "data": [{ "type": 1, "value": [{ "parameter": "x" }] }],
        "error": null,
        "message": null
    }));
    assert!(
        resp.findings().is_empty(),
        "parameter-only objects must not become findings"
    );
}

#[test]
fn adversarial_nested_techniques_empty_data_array() {
    let resp = data_response_from_json(json!({
        "success": true,
        "data": [{
            "type": 1,
            "value": [{
                "parameter": "id",
                "place": "GET",
                "data": []
            }]
        }],
        "error": null,
        "message": null
    }));
    assert!(
        resp.findings().is_empty(),
        "nested TECHNIQUES with empty data[] must yield zero findings"
    );
}

#[test]
fn adversarial_csv_escapes_comma_quote_newline() {
    let findings = vec![SqlmapFinding::new(
        "id",
        "error-based",
        "a,b\n\"quoted\"",
        json!({}),
    )];
    let csv = format_findings(&findings, OutputFormat::Csv);
    assert!(csv.starts_with("parameter,vulnerability_type,payload\n"));
    assert!(
        csv.contains("id,error-based,\""),
        "payload with comma/quote/newline must be quoted: {csv}"
    );
    assert!(
        csv.contains("\"a,b\n\"\"quoted\"\"\""),
        "RFC4180-style escaping expected: {csv}"
    );
}

#[test]
fn adversarial_dict_shaped_techniques_matches_list_fixture() {
    let list_resp = data_response_from_json(json!({
        "success": true,
        "data": [{
            "type": 1,
            "value": [{
                "parameter": "id",
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
            }]
        }],
        "error": null,
        "message": null
    }));
    let dict_resp = data_response_from_json(json!({
        "success": true,
        "data": [{
            "type": 1,
            "value": [{
                "parameter": "id",
                "data": {
                    "2": {
                        "technique": "time-based blind",
                        "payload": "id=1 AND SLEEP(5)"
                    },
                    "1": {
                        "title": "AND boolean-based blind - WHERE or HAVING clause",
                        "payload": "id=1 AND 8888=8888"
                    }
                }
            }]
        }],
        "error": null,
        "message": null
    }));

    let list_findings = list_resp.findings();
    let dict_findings = dict_resp.findings();
    assert_eq!(list_findings.len(), dict_findings.len());
    for (list, dict) in list_findings.iter().zip(dict_findings.iter()) {
        assert_eq!(list.parameter, dict.parameter);
        assert_eq!(list.vulnerability_type, dict.vulnerability_type);
        assert_eq!(list.payload, dict.payload);
    }
}

#[test]
fn adversarial_markdown_escapes_pipe_in_payload() {
    let findings = vec![SqlmapFinding::new(
        "id|param",
        "UNION|query",
        "1|2|3",
        json!({}),
    )];
    let md = format_findings(&findings, OutputFormat::Markdown);
    assert!(
        md.contains("id\\|param"),
        "pipe characters in parameter must be escaped: {md}"
    );
    assert!(
        md.contains("UNION\\|query"),
        "pipe characters in vulnerability_type must be escaped: {md}"
    );
    assert!(
        md.contains("1\\|2\\|3"),
        "pipe characters in payload must be escaped for markdown tables: {md}"
    );
    assert!(
        !md.contains("id|param") && !md.contains("UNION|query"),
        "unescaped pipes must not appear in markdown table cells: {md}"
    );
    assert_eq!(
        md.lines().count(),
        3,
        "markdown table must have header, separator, and one data row: {md}"
    );
}
