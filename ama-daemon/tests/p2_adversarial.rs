use axum::http::header::{HeaderName, HeaderValue};
use serde_json::json;
use uuid::Uuid;

fn hval(s: &str) -> HeaderValue {
    HeaderValue::from_str(s).unwrap()
}

const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
const X_AGENT_ID: HeaderName = HeaderName::from_static("x-agent-id");

fn action_request(target: &str) -> serde_json::Value {
    json!({
        "adapter": "test", "action": "file_write",
        "target": target, "magnitude": 1,
        "payload": "data"
    })
}

/// Agent A exhausting capacity must NOT affect Agent B.
#[tokio::test]
async fn test_capacity_isolation_under_exhaustion() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("isolated_a", 5, 60),
        ("isolated_b", 5, 60),
    ])
    .await;

    // Exhaust agent_a
    for i in 0..5 {
        let resp = server
            .post("/ama/action")
            .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
            .add_header(X_AGENT_ID, hval("isolated_a"))
            .json(&action_request(&format!("a{i}.txt")))
            .await;
        assert_eq!(
            resp.status_code(),
            200,
            "agent_a request {i} should succeed"
        );
    }

    // agent_a is exhausted
    let resp = server
        .post("/ama/action")
        .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
        .add_header(X_AGENT_ID, hval("isolated_a"))
        .json(&action_request("overflow.txt"))
        .await;
    assert_eq!(resp.status_code(), 403);

    // agent_b is unaffected — can still make all its requests
    for i in 0..5 {
        let resp = server
            .post("/ama/action")
            .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
            .add_header(X_AGENT_ID, hval("isolated_b"))
            .json(&action_request(&format!("b{i}.txt")))
            .await;
        assert_eq!(
            resp.status_code(),
            200,
            "agent_b request {i} should succeed"
        );
    }
}

/// Same idempotency key across different agents returns cached result
/// (global idempotency cache). This means agent_y gets agent_x's cached
/// result without capacity charge — accepted tradeoff in P2.
#[tokio::test]
async fn test_idempotency_key_global_across_agents() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_x", 10000, 60),
        ("agent_y", 10000, 60),
    ])
    .await;

    let shared_key = Uuid::new_v4().to_string();

    // First request with agent_x
    let resp = server
        .post("/ama/action")
        .add_header(IDEMPOTENCY_KEY, hval(&shared_key))
        .add_header(X_AGENT_ID, hval("agent_x"))
        .json(&action_request("shared.txt"))
        .await;
    assert_eq!(resp.status_code(), 200);

    // Same key with agent_y -> should return cached result (200)
    let resp = server
        .post("/ama/action")
        .add_header(IDEMPOTENCY_KEY, hval(&shared_key))
        .add_header(X_AGENT_ID, hval("agent_y"))
        .json(&action_request("shared.txt"))
        .await;
    assert_eq!(resp.status_code(), 200);
}

/// Empty X-Agent-Id header should be treated as missing
#[tokio::test]
async fn test_empty_agent_id_header() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_a", 10000, 60),
        ("agent_b", 10000, 60),
    ])
    .await;

    let resp = server
        .post("/ama/action")
        .add_header(IDEMPOTENCY_KEY, hval(&Uuid::new_v4().to_string()))
        .add_header(X_AGENT_ID, hval(""))
        .json(&action_request("empty.txt"))
        .await;
    // Empty string is not a valid agent_id, should be rejected
    assert_eq!(resp.status_code(), 400);
}
