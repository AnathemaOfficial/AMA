use axum::http::header::{HeaderName, HeaderValue};
use serde_json::json;
use uuid::Uuid;

fn hval(s: &str) -> HeaderValue {
    HeaderValue::from_str(s).unwrap()
}

const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
const X_AGENT_ID: HeaderName = HeaderName::from_static("x-agent-id");

#[tokio::test]
async fn test_per_agent_rate_limits_are_independent() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_a", 10000, 5),
        ("agent_b", 10000, 5),
    ])
    .await;

    // Exhaust agent_a's rate limit (5 requests)
    for _ in 0..5 {
        let resp = server
            .post("/ama/action")
            .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
            .add_header(X_AGENT_ID, hval("agent_a"))
            .json(&json!({
                "adapter": "test", "action": "file_write",
                "target": "test.txt", "magnitude": 1,
                "payload": "hello"
            }))
            .await;
        assert_eq!(resp.status_code(), 200);
    }

    // agent_a rate limited (429)
    let resp = server
        .post("/ama/action")
        .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
        .add_header(X_AGENT_ID, hval("agent_a"))
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 429);

    // agent_b still works
    let resp = server
        .post("/ama/action")
        .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
        .add_header(X_AGENT_ID, hval("agent_b"))
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 200);
}
