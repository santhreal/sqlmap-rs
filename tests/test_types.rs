use sqlmap_rs::types::{DataResponse, NewTaskResponse, StatusResponse};

#[test]
fn test_new_task_parsing() {
    let raw = r#"{"success": true, "taskid": "5f1a3b2k"}"#;
    let resp: NewTaskResponse = serde_json::from_str(raw).unwrap();
    assert!(resp.success);
    assert_eq!(resp.taskid.unwrap(), "5f1a3b2k");
}

#[test]
fn test_status_parsing() {
    let raw = r#"{"status": "running", "returncode": null, "success": true}"#;
    let resp: StatusResponse = serde_json::from_str(raw).unwrap();
    assert!(resp.success);
    assert_eq!(resp.status.unwrap(), "running");
    assert_eq!(resp.returncode, None);
    
    let raw_term = r#"{"status": "terminated", "returncode": 0, "success": true}"#;
    let term: StatusResponse = serde_json::from_str(raw_term).unwrap();
    assert_eq!(term.status.unwrap(), "terminated");
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
                        "title": "AND boolean-based blind - WHERE or HAVING clause",
                        "payload": "id=1 AND 8888=8888"
                    }
                ]
            }
        ],
        "error": [],
        "success": true
    }"#;

    let resp: DataResponse = serde_json::from_str(raw).expect("Could not parse data chunk");
    assert!(resp.success);
    
    let chunk = &resp.data.unwrap()[0];
    assert_eq!(chunk.r#type, 1);
    
    let injection_data = &chunk.value[0];
    assert_eq!(injection_data["parameter"], "id");
    assert_eq!(injection_data["dbms"], "MySQL");
}
