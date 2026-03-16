use ama_core::config::AmaConfig;
use ama_core::errors::AmaError;
use ama_core::idempotency::{validate_idempotency_key, IdempotencyCache, IdempotencyStatus};
use ama_core::pipeline::process_action;
use ama_core::schema::ActionRequest;
use ama_core::slime::{P0Authorizer, SlimeAuthorizer};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tower::ServiceBuilder;
use tower::timeout::TimeoutLayer;
use tower_http::limit::RequestBodyLimitLayer;
use axum::error_handling::HandleErrorLayer;
use uuid::Uuid;

/// Convert an AmaError into an axum Response using the http_status_and_body() method.
fn ama_error_response(e: AmaError) -> Response {
    let (status, body) = e.http_status_and_body();
    (StatusCode::from_u16(status).unwrap(), Json(body)).into_response()
}

/// Rate limiter window state — protected by a single mutex to prevent
/// the C3 race condition where counter increment and window reset
/// were not atomic.
pub struct RateLimitState {
    pub window_start: Instant,
    pub count: u64,
}

/// Shared application state wrapped in Arc for thread-safe access.
pub struct AppState {
    pub config: AmaConfig,
    pub authorizer: P0Authorizer,
    pub idempotency_cache: IdempotencyCache,
    pub session_id: Uuid,
    pub start_time: Instant,
    pub domain_counters: HashMap<String, AtomicU64>,
    pub rate_limiter: std::sync::Mutex<RateLimitState>,
}

impl AppState {
    pub fn new(config: AmaConfig) -> Arc<Self> {
        let max_capacity = config.max_capacity;

        let slime_domains: Vec<(ama_core::slime::DomainId, ama_core::config::DomainPolicy)> =
            config.domain_policies.iter().map(|(id, policy)| {
                (id.clone(), policy.clone())
            }).collect();

        let mut domain_counters = HashMap::new();
        for domain_id in config.domain_policies.keys() {
            domain_counters.insert(domain_id.clone(), AtomicU64::new(0));
        }

        Arc::new(Self {
            authorizer: P0Authorizer::new(max_capacity, slime_domains),
            idempotency_cache: IdempotencyCache::new(10_000, std::time::Duration::from_secs(300)),
            session_id: Uuid::new_v4(),
            start_time: Instant::now(),
            domain_counters,
            rate_limiter: std::sync::Mutex::new(RateLimitState {
                window_start: Instant::now(),
                count: 0,
            }),
            config,
        })
    }
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ama/action", post(handle_action))
        .route("/ama/status", get(handle_status))
        .route("/health", get(handle_health))
        .route("/version", get(handle_version))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"status": "error", "error_class": "timeout",
                            "message": "request exceeded 30s global deadline"})),
                    )
                }))
                .layer(RequestBodyLimitLayer::new(1_048_576))
                .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
                .concurrency_limit(8)
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            content_type_middleware,
        ))
        .with_state(state)
}

async fn content_type_middleware(
    State(_state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if req.method() == axum::http::Method::POST {
        let content_type = req.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type.starts_with("application/json") {
            return ama_error_response(AmaError::UnsupportedMediaType);
        }
    }
    next.run(req).await
}

/// P1 fix (C3): window_start and counter are now under the same mutex.
/// No gap between window reset and counter increment.
fn check_rate_limit(state: &AppState) -> bool {
    let mut rl = state.rate_limiter.lock().unwrap();
    let now = Instant::now();
    let elapsed = now.duration_since(rl.window_start);

    if elapsed.as_secs() >= 60 {
        // New window — reset counter atomically with window start
        rl.window_start = now;
        rl.count = 1;
        return true;
    }

    rl.count += 1;
    rl.count <= 60
}

fn increment_domain_counter(state: &AppState, domain_id: &str) {
    if let Some(counter) = state.domain_counters.get(domain_id) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

async fn handle_health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

async fn handle_version() -> impl IntoResponse {
    Json(json!({
        "name": "ama",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": "ama-action-v1"
    }))
}

async fn handle_status(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();
    let capacity_used = state.authorizer.capacity_used();
    let capacity_max = state.authorizer.capacity_max();

    let mut domains = serde_json::Map::new();
    for (domain_id, policy) in &state.config.domain_policies {
        let count = state.domain_counters
            .get(domain_id)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0);
        domains.insert(domain_id.clone(), json!({
            "enabled": policy.enabled,
            "actions_count": count,
        }));
    }

    Json(json!({
        "session_id": state.session_id.to_string(),
        "capacity_used": capacity_used,
        "capacity_max": capacity_max,
        "capacity_remaining": capacity_max.saturating_sub(capacity_used),
        "uptime_seconds": uptime,
        "domains": domains,
    }))
}

async fn handle_action(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. Rate limit
    if !check_rate_limit(&state) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"status": "error", "error_class": "rate_limited", "message": "rate limit exceeded (60/min)"})),
        ).into_response();
    }

    // 2. Extract Idempotency-Key header
    let idem_key_str = match headers.get("idempotency-key") {
        Some(val) => match val.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return ama_error_response(AmaError::BadRequest {
                message: "Idempotency-Key header is not valid ASCII".into(),
            }),
        },
        None => return ama_error_response(AmaError::BadRequest {
            message: "missing Idempotency-Key header".into(),
        }),
    };

    // 3. Validate UUID v4 format
    let idem_key = match validate_idempotency_key(&idem_key_str) {
        Ok(k) => k,
        Err(e) => return ama_error_response(e),
    };

    // 4. Idempotency cache check
    match state.idempotency_cache.check_or_insert(idem_key) {
        IdempotencyStatus::Cached(cached_response) => {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                cached_response,
            ).into_response();
        }
        IdempotencyStatus::InFlight => {
            return ama_error_response(AmaError::Conflict {
                message: "duplicate Idempotency-Key with in-flight request".into(),
            });
        }
        IdempotencyStatus::Full => {
            state.idempotency_cache.remove(&idem_key);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "error", "error_class": "service_unavailable",
                    "message": "idempotency cache full — fail-closed"})),
            ).into_response();
        }
        IdempotencyStatus::New => {
            // Continue processing
        }
    }

    // 5. Deserialize request body
    let request: ActionRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            // P1 Model A: commit error as terminal result, do not remove.
            // This ensures retry with same key replays the error.
            let error_response = json!({
                "status": "error",
                "error_class": "bad_request",
                "message": format!("invalid JSON: {}", e),
            });
            state.idempotency_cache.complete(
                idem_key,
                serde_json::to_string(&error_response).unwrap(),
            );
            return (StatusCode::BAD_REQUEST, Json(error_response)).into_response();
        }
    };

    let action_name = request.action.clone();
    let magnitude = request.magnitude;

    // Generate action_id
    let action_id = Uuid::new_v4().to_string();

    // 6. Process through pipeline
    let result = process_action(
        request,
        &state.config,
        &state.authorizer,
        action_id,
        &state.session_id.to_string(),
    ).await;

    // 7. Build response and cache
    match result {
        Ok(response) => {
            let response_json = serde_json::to_string(&response).unwrap();

            if let Ok(mapping) = ama_core::mapper::map_action(
                &action_name, magnitude, &state.config,
            ) {
                increment_domain_counter(&state, &mapping.domain_id);
            }

            state.idempotency_cache.complete(idem_key, response_json.clone());
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                response_json,
            ).into_response()
        }
        Err(e) => {
            // P1 Model A: commit error as terminal result, do not remove.
            // All terminal outcomes (denial, timeout, failure) go to DONE.
            // Retry with same key will replay the cached error response.
            //
            // Build JSON for caching BEFORE consuming `e` with ama_error_response().
            let cached_json = match &e {
                AmaError::Impossible => json!({"status": "impossible"}),
                AmaError::BadRequest { message } => {
                    json!({"status": "error", "error_class": "bad_request", "message": message})
                }
                AmaError::Validation { error_class, message } => {
                    json!({"status": "error", "error_class": error_class, "message": message})
                }
                AmaError::ServiceUnavailable { message } => {
                    json!({"status": "error", "error_class": "service_unavailable", "message": message})
                }
                other => {
                    json!({"status": "error", "message": other.to_string()})
                }
            };
            state.idempotency_cache.complete(
                idem_key,
                serde_json::to_string(&cached_json).unwrap(),
            );
            ama_error_response(e)
        }
    }
}

/// Graceful shutdown signal handler.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, shutting down gracefully");
            }
            _ = ctrl_c => {
                tracing::info!("Received Ctrl+C, shutting down gracefully");
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C, shutting down gracefully");
    }
}

/// Test helper: build a test server with default capacity (10000).
#[cfg(feature = "test-utils")]
pub async fn test_server() -> axum_test::TestServer {
    test_server_with_capacity(10_000).await
}

/// Test helper: build a test server with custom capacity.
#[cfg(feature = "test-utils")]
pub async fn test_server_with_capacity(max_capacity: u64) -> axum_test::TestServer {
    use ama_core::config::{AmaConfig, DomainPolicy, DomainMapping, BootHashes};

    let workspace = std::env::temp_dir().join(format!("ama-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&workspace).unwrap();

    let mut domain_policies = HashMap::new();
    domain_policies.insert("fs.write.workspace".into(), DomainPolicy {
        enabled: true,
        max_magnitude_per_action: 1000,
    });

    let mut domain_mappings = HashMap::new();
    domain_mappings.insert("file_write".into(), DomainMapping {
        domain_id: "fs.write.workspace".into(),
        max_payload_bytes: Some(1_048_576),
        validator: None,
        requires_intent: false,
    });

    let default_agent = ama_core::config::AgentConfig {
        agent_id: "default".into(),
        max_capacity,
        rate_limit_per_window: 60,
        rate_limit_window_secs: 60,
        domain_policies: domain_policies.clone(),
    };
    let mut agents = HashMap::new();
    agents.insert("default".into(), default_agent);

    let config = AmaConfig {
        workspace_root: workspace,
        bind_host: "127.0.0.1".into(),
        bind_port: 8787,
        log_level: "info".into(),
        log_output: "stderr".into(),
        slime_mode: "embedded".into(),
        max_capacity,
        domain_policies,
        domain_mappings,
        intents: HashMap::new(),
        allowlist: vec![],
        agents,
        default_agent_id: Some("default".into()),
        boot_hashes: BootHashes {
            config_hash: "test".into(),
            domains_hash: "test".into(),
            intents_hash: "test".into(),
            allowlist_hash: "test".into(),
            agents_hash: "test".into(),
        },
    };

    let state = AppState::new(config);
    let app = build_router(state);
    axum_test::TestServer::new(app.into_make_service()).unwrap()
}
