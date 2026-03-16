# AMA P2 — Multi-Agent Capacity System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform AMA from a single global capacity budget into a multi-agent system where each agent gets its own capacity configuration, rate limits, and budget counters — while preserving the monotonic thermodynamic model.

**Architecture:** Split into `ama-core` (pure Rust library, zero HTTP dependency) and `ama-daemon` (axum HTTP wrapper). Agent configs loaded from `config/agents/*.toml` at boot. `X-Agent-Id` header selects the agent context (NOT authentication). Each agent gets its own `P0Authorizer` instance and rate limiter. Idempotency cache stays global.

**Tech Stack:** Rust, axum 0.8, tokio, DashMap, Tower middleware, TOML config, SHA-256 boot integrity

---

## Pre-Implementation Notes

### What Already Exists (DO NOT recreate)
- `CanonicalAction` enum (FileWrite, FileRead, ShellExec, HttpRequest) — `src/canonical.rs`
- Newtypes with private constructors (WorkspacePath, SafeArg, AllowlistedUrl, BoundedBytes, HttpMethod, IntentId) — `src/newtypes.rs`
- Domain mapping (`map_action()`) — `src/mapper.rs`
- `SlimeAuthorizer` trait with `try_reserve()` / `check_only()` — `src/slime.rs`
- `P0Authorizer` with AtomicU64 CAS loop (monotonic, correct) — `src/slime.rs`
- Idempotency cache with DashMap entry() API (P1 fix) — `src/idempotency.rs`
- Full pipeline: validate → map → authorize → actuate — `src/pipeline.rs`
- 94 passing tests across 17 test files (verified with `cargo test --features test-utils`)

### Key Design Decisions
1. **Monotonic capacity is sacred** — capacity never releases. Per-agent budgets are independent monotonic counters (same CAS loop as P0).
2. **No "auth" vocabulary** — `X-Agent-Id` is a context selector. If the header is missing or unknown, reject with 400, not 401/403.
3. **No "session" terminology** for rate limits — use "window" (rate limit windows).
4. **Idempotency cache stays GLOBAL** — not per-agent. Same UUID can't be reused across agents.
5. **Global rate limiter → per-agent rate limiters** — each agent gets its own 60/window counter.
6. **Boot integrity** — SHA-256 hashes now include all agent config files.
7. **DomainPolicy duplicate (I6)** — resolved: `config::DomainPolicy` is the canonical type, `slime::DomainPolicy` becomes a re-export or gets merged.
8. **Default agent** — if `config/agents/` has exactly one file, that agent_id is the default when `X-Agent-Id` is absent (backward compat with P1 single-agent mode).

### Config Migration Path
```
P1 (single agent):                    P2 (multi-agent):
config/config.toml                    config/config.toml (global only, no [slime.domains])
  [slime]                             config/agents/openclaw.toml
    max_capacity = 10000                [agent]
    [slime.domains.fs_write_...]        agent_id = "openclaw"
                                        max_capacity = 10000
                                        rate_limit_per_window = 60
                                        rate_limit_window_secs = 60
                                        [agent.domains.fs_write_workspace]
                                        enabled = true
                                        max_magnitude_per_action = 100
```

---

## Chunk 1: Workspace Crate Split

Split the single `ama` crate into a Cargo workspace with two members: `ama-core` (library) and `ama-daemon` (binary). All 94 existing tests must pass after the split.

### Task 1: Create workspace Cargo.toml

**Files:**
- Create: `Cargo.toml` (workspace root — replaces current)
- Create: `ama-core/Cargo.toml`
- Create: `ama-daemon/Cargo.toml`

- [ ] **Step 1: Back up current Cargo.toml**

```bash
cp Cargo.toml Cargo.toml.p1-backup
```

- [ ] **Step 2: Create workspace root Cargo.toml**

```toml
[workspace]
members = ["ama-core", "ama-daemon"]
resolver = "2"
```

- [ ] **Step 3: Create ama-core/Cargo.toml**

```toml
[package]
name = "ama-core"
version = "0.2.0-p2-dev"
edition = "2021"
description = "AMA core library — deterministic security membrane for AI agents (no HTTP)"

[features]
test-utils = []

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
uuid = { version = "1", features = ["v4"] }
sha2 = "0.10"
reqwest = { version = "0.12", features = ["rustls-tls"], default-features = false }
thiserror = "2"
tracing = "0.1"
dashmap = "6"
tokio = { version = "1", features = ["full"] }

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["full", "test-util"] }
```

- [ ] **Step 4: Create ama-daemon/Cargo.toml**

```toml
[package]
name = "ama-daemon"
version = "0.2.0-p2-dev"
edition = "2021"
description = "AMA HTTP daemon — axum wrapper for ama-core"

[features]
test-utils = ["dep:axum-test", "ama-core/test-utils"]

[dependencies]
ama-core = { path = "../ama-core" }
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tower = { version = "0.5", features = ["limit"] }
tower-http = { version = "0.6", features = ["limit"] }
bytes = "1"
axum-test = { version = "17", optional = true }

[dev-dependencies]
axum-test = "17"
tokio = { version = "1", features = ["full", "test-util"] }
tempfile = "3"

[[test]]
name = "test_integration"
required-features = ["test-utils"]

[[test]]
name = "p1_timeouts"
required-features = ["test-utils"]

[[test]]
name = "p1_rate_limit"
required-features = ["test-utils"]

[[test]]
name = "p1_queue"
required-features = ["test-utils"]

[[test]]
name = "p1_adversarial"
required-features = ["test-utils"]

[[test]]
name = "p2_rate_limit"
required-features = ["test-utils"]

[[test]]
name = "p2_agent_routing"
required-features = ["test-utils"]

[[test]]
name = "p2_adversarial"
required-features = ["test-utils"]
```

- [ ] **Step 5: Commit workspace scaffolding**

```bash
git add Cargo.toml Cargo.toml.p1-backup ama-core/Cargo.toml ama-daemon/Cargo.toml
git commit -m "feat(p2): scaffold workspace with ama-core and ama-daemon crates"
```

### Task 2: Move source files to ama-core

**Files:**
- Move: `src/errors.rs` → `ama-core/src/errors.rs` (**strip axum IntoResponse impl** — see below)
- Move: `src/newtypes.rs` → `ama-core/src/newtypes.rs`
- Move: `src/canonical.rs` → `ama-core/src/canonical.rs`
- Move: `src/schema.rs` → `ama-core/src/schema.rs`
- Move: `src/config.rs` → `ama-core/src/config.rs`
- Move: `src/slime.rs` → `ama-core/src/slime.rs`
- Move: `src/mapper.rs` → `ama-core/src/mapper.rs`
- Move: `src/actuator/` → `ama-core/src/actuator/`
- Move: `src/idempotency.rs` → `ama-core/src/idempotency.rs`
- Move: `src/audit.rs` → `ama-core/src/audit.rs`
- Move: `src/pipeline.rs` → `ama-core/src/pipeline.rs`
- Create: `ama-core/src/lib.rs`

- [ ] **Step 1: Create ama-core directory structure**

```bash
mkdir -p ama-core/src/actuator
```

- [ ] **Step 2: Move core modules**

**Note:** Use `cp` for now (git will detect renames via `git diff -M`). Cannot use `git mv` because we need to modify files during the move (e.g., stripping axum from errors.rs). History is preserved via rename detection.

```bash
# Move all non-HTTP modules to ama-core
cp src/errors.rs ama-core/src/errors.rs
cp src/newtypes.rs ama-core/src/newtypes.rs
cp src/canonical.rs ama-core/src/canonical.rs
cp src/schema.rs ama-core/src/schema.rs
cp src/config.rs ama-core/src/config.rs
cp src/slime.rs ama-core/src/slime.rs
cp src/mapper.rs ama-core/src/mapper.rs
cp src/idempotency.rs ama-core/src/idempotency.rs
cp src/audit.rs ama-core/src/audit.rs
cp src/pipeline.rs ama-core/src/pipeline.rs
cp -r src/actuator/* ama-core/src/actuator/
```

- [ ] **Step 3: Create ama-core/src/lib.rs**

```rust
pub mod errors;
pub mod newtypes;
pub mod canonical;
pub mod schema;
pub mod config;
pub mod slime;
pub mod mapper;
pub mod actuator;
pub mod idempotency;
pub mod audit;
pub mod pipeline;
```

- [ ] **Step 4: Split errors.rs — remove axum dependency from ama-core**

**CRITICAL:** `src/errors.rs` imports `axum::http::StatusCode` and implements `IntoResponse`. Since ama-core must have zero HTTP dependencies, split it:

In `ama-core/src/errors.rs`, keep only the pure error enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AmaError {
    #[error("malformed request: {message}")]
    BadRequest { message: String },

    #[error("impossible")]
    Impossible,

    #[error("validation error: {message}")]
    Validation { error_class: String, message: String },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("unsupported media type")]
    UnsupportedMediaType,

    #[error("rate limit exceeded")]
    RateLimited,

    #[error("service unavailable: {message}")]
    ServiceUnavailable { message: String },
}
```

Create `ama-daemon/src/error_response.rs` with the `IntoResponse` impl:

```rust
use ama_core::errors::AmaError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

impl IntoResponse for AmaError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AmaError::Impossible => (StatusCode::FORBIDDEN, json!({"status": "impossible"})),
            AmaError::BadRequest { message } => (StatusCode::BAD_REQUEST,
                json!({"status": "error", "error_class": "bad_request", "message": message})),
            AmaError::Validation { error_class, message } => (StatusCode::UNPROCESSABLE_ENTITY,
                json!({"status": "error", "error_class": error_class, "message": message})),
            AmaError::Conflict { message } => (StatusCode::CONFLICT,
                json!({"status": "error", "error_class": "conflict", "message": message})),
            AmaError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE,
                json!({"status": "error", "error_class": "payload_too_large", "message": "payload exceeds limit"})),
            AmaError::UnsupportedMediaType => (StatusCode::UNSUPPORTED_MEDIA_TYPE,
                json!({"status": "error", "error_class": "unsupported_media_type", "message": "expected application/json"})),
            AmaError::RateLimited => (StatusCode::TOO_MANY_REQUESTS,
                json!({"status": "error", "error_class": "rate_limited", "message": "rate limit exceeded"})),
            AmaError::ServiceUnavailable { message } => (StatusCode::SERVICE_UNAVAILABLE,
                json!({"status": "error", "error_class": "service_unavailable", "message": message})),
        };
        (status, axum::Json(body)).into_response()
    }
}
```

**Note:** Rust orphan rules require that `IntoResponse` impl lives in the crate that owns `AmaError` OR the crate that owns `IntoResponse`. Since ama-core owns `AmaError`, we need a newtype wrapper in ama-daemon:

```rust
// ama-daemon/src/error_response.rs
pub struct AmaErrorResponse(pub AmaError);

impl IntoResponse for AmaErrorResponse {
    fn into_response(self) -> Response {
        // ... same match on self.0 ...
    }
}
```

Or alternatively, add a `to_response(&self) -> (StatusCode, Json<Value>)` method on AmaError in ama-core that returns the status code as u16 + JSON body, and let ama-daemon call it. This avoids orphan rule issues entirely:

```rust
// ama-core/src/errors.rs
impl AmaError {
    /// Returns (HTTP status code, JSON body) for HTTP serialization.
    /// Does not depend on axum — status is a raw u16.
    pub fn http_status_and_body(&self) -> (u16, serde_json::Value) {
        match self {
            AmaError::Impossible => (403, json!({"status": "impossible"})),
            AmaError::BadRequest { message } => (400,
                json!({"status": "error", "error_class": "bad_request", "message": message})),
            // ... etc for all variants ...
        }
    }
}
```

```rust
// ama-daemon/src/server.rs — wherever AmaError needs to become a Response:
fn ama_error_response(e: AmaError) -> Response {
    let (status, body) = e.http_status_and_body();
    (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response()
}
```

**Recommended approach:** The `http_status_and_body()` method — cleanest, no orphan issues, no axum in ama-core.

- [ ] **Step 5: Update ama-core internal imports**

Replace all `use crate::` references — they stay as `crate::` within ama-core (no change needed since they're now inside the ama-core crate). Remove `axum` from ama-core `Cargo.toml`.

Verify: `cd ama-core && cargo check`

- [ ] **Step 6: Commit core module move**

```bash
git add ama-core/src/
git commit -m "feat(p2): move core modules to ama-core crate"
```

### Task 3: Move HTTP layer to ama-daemon

**Files:**
- Move: `src/server.rs` → `ama-daemon/src/server.rs`
- Move: `src/main.rs` → `ama-daemon/src/main.rs`
- Create: `ama-daemon/src/lib.rs`

- [ ] **Step 1: Create ama-daemon directory structure**

```bash
mkdir -p ama-daemon/src
```

- [ ] **Step 2: Move daemon modules**

```bash
cp src/server.rs ama-daemon/src/server.rs
cp src/main.rs ama-daemon/src/main.rs
```

- [ ] **Step 3: Create ama-daemon/src/lib.rs**

```rust
pub mod server;
pub mod error_response;
```

- [ ] **Step 4: Update ama-daemon imports to use ama-core**

In `ama-daemon/src/server.rs`, replace all `crate::` references with `ama_core::`:

```rust
use ama_core::config::AmaConfig;
use ama_core::errors::AmaError;
use ama_core::idempotency::{validate_idempotency_key, IdempotencyCache, IdempotencyStatus};
use ama_core::pipeline::process_action;
use ama_core::schema::ActionRequest;
use ama_core::slime::{P0Authorizer, SlimeAuthorizer};
```

In `ama-daemon/src/main.rs`:

```rust
use ama_core::config::AmaConfig;
use ama_core::actuator::file::cleanup_orphan_temps;
use ama_daemon::server::{AppState, build_router, shutdown_signal};
```

Also update the `cleanup_orphan_temps` call and the `AmaError` response conversion to use `ama_core::` paths.

- [ ] **Step 5: Verify ama-daemon compiles**

```bash
cd ama-daemon && cargo check
```

- [ ] **Step 6: Commit daemon module move**

```bash
git add ama-daemon/src/
git commit -m "feat(p2): move HTTP layer to ama-daemon crate"
```

### Task 4: Move tests and verify green

**Files:**
- Move: `tests/test_newtypes.rs` → `ama-core/tests/test_newtypes.rs`
- Move: `tests/test_schema.rs` → `ama-core/tests/test_schema.rs`
- Move: `tests/test_config.rs` → `ama-core/tests/test_config.rs`
- Move: `tests/test_slime.rs` → `ama-core/tests/test_slime.rs`
- Move: `tests/test_mapper.rs` → `ama-core/tests/test_mapper.rs`
- Move: `tests/test_actuator_file.rs` → `ama-core/tests/test_actuator_file.rs`
- Move: `tests/test_actuator_shell.rs` → `ama-core/tests/test_actuator_shell.rs`
- Move: `tests/test_actuator_http.rs` → `ama-core/tests/test_actuator_http.rs`
- Move: `tests/test_idempotency.rs` → `ama-core/tests/test_idempotency.rs`
- Move: `tests/test_audit.rs` → `ama-core/tests/test_audit.rs`
- Move: `tests/test_pipeline.rs` → `ama-core/tests/test_pipeline.rs`
- Move: `tests/test_integration.rs` → `ama-daemon/tests/test_integration.rs`
- Move: `tests/p1_*.rs` → `ama-daemon/tests/p1_*.rs`

- [ ] **Step 1: Move unit tests to ama-core**

```bash
mkdir -p ama-core/tests ama-daemon/tests
# Unit tests → ama-core
cp tests/test_newtypes.rs ama-core/tests/
cp tests/test_schema.rs ama-core/tests/
cp tests/test_config.rs ama-core/tests/
cp tests/test_slime.rs ama-core/tests/
cp tests/test_mapper.rs ama-core/tests/
cp tests/test_actuator_file.rs ama-core/tests/
cp tests/test_actuator_shell.rs ama-core/tests/
cp tests/test_actuator_http.rs ama-core/tests/
cp tests/test_idempotency.rs ama-core/tests/
cp tests/test_audit.rs ama-core/tests/
cp tests/test_pipeline.rs ama-core/tests/
# Integration tests → ama-daemon
cp tests/test_integration.rs ama-daemon/tests/
cp tests/p1_idempotency.rs ama-daemon/tests/
cp tests/p1_timeouts.rs ama-daemon/tests/
cp tests/p1_rate_limit.rs ama-daemon/tests/
cp tests/p1_queue.rs ama-daemon/tests/
cp tests/p1_adversarial.rs ama-daemon/tests/
```

- [ ] **Step 2: Update test imports**

In all `ama-core/tests/*.rs` files, replace `use ama::` with `use ama_core::`.

In all `ama-daemon/tests/*.rs` files, replace `use ama::` with the appropriate mix of `use ama_core::` (for types) and `use ama_daemon::` (for server helpers).

- [ ] **Step 3: Move test config fixtures**

If tests reference `config/` directory, ensure test helpers use `tempdir` or that the config dir path is relative to workspace root, not crate root. The `test_server()` helper in `ama-daemon/src/server.rs` builds config programmatically so no file fixtures needed for integration tests.

For `test_config.rs` which tests `AmaConfig::load()`, ensure the test config fixtures are accessible. Copy them or update paths:

```bash
# Config fixtures stay at workspace root
# test_config.rs should reference "../config" or use a test fixture path
```

- [ ] **Step 4: Run all tests**

```bash
# From workspace root
cargo test --workspace --features test-utils
```

Expected: All 94 tests pass. Zero failures.

- [ ] **Step 5: Remove old src/ and tests/ directories**

Only after all tests pass:

```bash
rm -rf src/ tests/ Cargo.toml.p1-backup
```

- [ ] **Step 6: Verify clean build**

```bash
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace --features test-utils
```

- [ ] **Step 7: Commit clean split**

```bash
git add -A
git commit -m "feat(p2): complete crate split — ama-core + ama-daemon, 94 tests green"
```

---

## Chunk 2: Agent Configuration System

Add per-agent configuration files loaded from `config/agents/*.toml`. Each agent defines its own capacity budget, domain policies, and rate limit parameters.

### Task 5: Define AgentConfig type

**Files:**
- Modify: `ama-core/src/config.rs`
- Test: `ama-core/tests/test_config.rs`

- [ ] **Step 1: Write the failing test**

In `ama-core/tests/test_config.rs`:

```rust
#[test]
fn test_agent_config_loads_from_toml() {
    let toml_str = r#"
[agent]
agent_id = "openclaw"
max_capacity = 5000
rate_limit_per_window = 30
rate_limit_window_secs = 60

[agent.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100

[agent.domains.fs_read_workspace]
enabled = true
max_magnitude_per_action = 500
"#;
    let agent_config = ama_core::config::AgentConfig::from_toml_str(toml_str).unwrap();
    assert_eq!(agent_config.agent_id, "openclaw");
    assert_eq!(agent_config.max_capacity, 5000);
    assert_eq!(agent_config.rate_limit_per_window, 30);
    assert_eq!(agent_config.rate_limit_window_secs, 60);
    assert_eq!(agent_config.domain_policies.len(), 2);
    assert!(agent_config.domain_policies.contains_key("fs.write.workspace"));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p ama-core test_agent_config_loads_from_toml
```

Expected: FAIL — `AgentConfig` does not exist.

- [ ] **Step 3: Implement AgentConfig**

In `ama-core/src/config.rs`, add:

```rust
// ── Agent Config (P2) ─────────────────────────────────────────

#[derive(Deserialize)]
struct RawAgentConfig {
    agent: RawAgent,
}

#[derive(Deserialize)]
struct RawAgent {
    agent_id: String,
    max_capacity: u64,
    #[serde(default = "default_rate_limit_per_window")]
    rate_limit_per_window: u64,
    #[serde(default = "default_rate_limit_window_secs")]
    rate_limit_window_secs: u64,
    domains: HashMap<String, RawDomainPolicy>,
}

fn default_rate_limit_per_window() -> u64 { 60 }
fn default_rate_limit_window_secs() -> u64 { 60 }

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub agent_id: String,
    pub max_capacity: u64,
    pub rate_limit_per_window: u64,
    pub rate_limit_window_secs: u64,
    pub domain_policies: HashMap<String, DomainPolicy>,
}

impl AgentConfig {
    pub fn from_toml_str(toml_str: &str) -> Result<Self, AmaError> {
        let raw: RawAgentConfig = toml::from_str(toml_str)
            .map_err(|e| AmaError::ServiceUnavailable {
                message: format!("agent config parse error: {e}"),
            })?;

        if raw.agent.agent_id.is_empty() {
            return Err(AmaError::ServiceUnavailable {
                message: "agent_id must not be empty".into(),
            });
        }
        if !raw.agent.agent_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(AmaError::ServiceUnavailable {
                message: format!("agent_id '{}' contains invalid characters", raw.agent.agent_id),
            });
        }
        if raw.agent.max_capacity == 0 {
            return Err(AmaError::ServiceUnavailable {
                message: "max_capacity must be > 0".into(),
            });
        }
        if raw.agent.rate_limit_per_window == 0 {
            return Err(AmaError::ServiceUnavailable {
                message: "rate_limit_per_window must be > 0".into(),
            });
        }
        if raw.agent.rate_limit_window_secs == 0 {
            return Err(AmaError::ServiceUnavailable {
                message: "rate_limit_window_secs must be > 0".into(),
            });
        }

        let mut domain_policies = HashMap::new();
        for (key, raw_policy) in &raw.agent.domains {
            let domain_id = key.replace('_', ".");
            if raw_policy.max_magnitude_per_action == 0 {
                return Err(AmaError::ServiceUnavailable {
                    message: format!("agent '{}' domain '{}': max_magnitude_per_action must be > 0",
                        raw.agent.agent_id, domain_id),
                });
            }
            if raw_policy.max_magnitude_per_action > raw.agent.max_capacity {
                return Err(AmaError::ServiceUnavailable {
                    message: format!("agent '{}' domain '{}': max_magnitude_per_action ({}) > max_capacity ({})",
                        raw.agent.agent_id, domain_id,
                        raw_policy.max_magnitude_per_action, raw.agent.max_capacity),
                });
            }
            domain_policies.insert(domain_id, DomainPolicy {
                enabled: raw_policy.enabled,
                max_magnitude_per_action: raw_policy.max_magnitude_per_action,
            });
        }

        Ok(Self {
            agent_id: raw.agent.agent_id,
            max_capacity: raw.agent.max_capacity,
            rate_limit_per_window: raw.agent.rate_limit_per_window,
            rate_limit_window_secs: raw.agent.rate_limit_window_secs,
            domain_policies,
        })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p ama-core test_agent_config_loads_from_toml
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add ama-core/src/config.rs ama-core/tests/test_config.rs
git commit -m "feat(p2): add AgentConfig type with TOML parsing and validation"
```

### Task 6: Load agent configs from config/agents/ directory

**Files:**
- Modify: `ama-core/src/config.rs`
- Test: `ama-core/tests/test_config.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_load_agent_configs_from_directory() {
    let dir = tempfile::tempdir().unwrap();
    let agents_dir = dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(agents_dir.join("openclaw.toml"), r#"
[agent]
agent_id = "openclaw"
max_capacity = 5000
[agent.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100
"#).unwrap();

    std::fs::write(agents_dir.join("claude.toml"), r#"
[agent]
agent_id = "claude"
max_capacity = 8000
[agent.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 200
"#).unwrap();

    let agents = ama_core::config::load_agent_configs(&agents_dir).unwrap();
    assert_eq!(agents.len(), 2);
    assert!(agents.contains_key("openclaw"));
    assert!(agents.contains_key("claude"));
    assert_eq!(agents["openclaw"].max_capacity, 5000);
    assert_eq!(agents["claude"].max_capacity, 8000);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p ama-core test_load_agent_configs_from_directory
```

- [ ] **Step 3: Implement load_agent_configs**

```rust
/// Load all agent configs from a directory of TOML files.
/// Returns a map of agent_id -> AgentConfig.
/// Fails if any file is invalid or if two files define the same agent_id.
pub fn load_agent_configs(agents_dir: &Path) -> Result<HashMap<String, AgentConfig>, AmaError> {
    let mut agents = HashMap::new();
    let mut hashes = Vec::new();

    let entries = fs::read_dir(agents_dir).map_err(|e| AmaError::ServiceUnavailable {
        message: format!("cannot read agents directory: {e}"),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| AmaError::ServiceUnavailable {
            message: format!("error reading agents directory entry: {e}"),
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        let bytes = fs::read(&path).map_err(|e| AmaError::ServiceUnavailable {
            message: format!("cannot read {}: {e}", path.display()),
        })?;
        let toml_str = std::str::from_utf8(&bytes).map_err(|e| AmaError::ServiceUnavailable {
            message: format!("{} not UTF-8: {e}", path.display()),
        })?;

        let agent = AgentConfig::from_toml_str(toml_str)?;
        let hash = sha256_hex(&bytes);
        hashes.push((agent.agent_id.clone(), hash));

        if agents.contains_key(&agent.agent_id) {
            return Err(AmaError::ServiceUnavailable {
                message: format!("duplicate agent_id '{}' in agents directory", agent.agent_id),
            });
        }
        agents.insert(agent.agent_id.clone(), agent);
    }

    if agents.is_empty() {
        return Err(AmaError::ServiceUnavailable {
            message: "no agent configs found in agents directory".into(),
        });
    }

    Ok(agents)
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p ama-core test_load_agent_configs_from_directory
```

- [ ] **Step 5: Write validation edge case tests**

```rust
#[test]
fn test_load_agent_configs_rejects_duplicate_agent_id() {
    let dir = tempfile::tempdir().unwrap();
    let agents_dir = dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    // Two files with same agent_id
    let toml = r#"
[agent]
agent_id = "same"
max_capacity = 1000
[agent.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100
"#;
    std::fs::write(agents_dir.join("a.toml"), toml).unwrap();
    std::fs::write(agents_dir.join("b.toml"), toml).unwrap();

    let result = ama_core::config::load_agent_configs(&agents_dir);
    assert!(result.is_err());
}

#[test]
fn test_load_agent_configs_rejects_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let agents_dir = dir.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    let result = ama_core::config::load_agent_configs(&agents_dir);
    assert!(result.is_err());
}
```

- [ ] **Step 6: Run all tests**

```bash
cargo test -p ama-core
```

- [ ] **Step 7: Commit**

```bash
git add ama-core/src/config.rs ama-core/tests/test_config.rs
git commit -m "feat(p2): load agent configs from config/agents/ directory"
```

### Task 7: Integrate agent configs into AmaConfig and boot

**Files:**
- Modify: `ama-core/src/config.rs` (AmaConfig gets `agents` field)
- Modify: `ama-core/src/config.rs` (BootHashes gets agent hashes)
- Modify: `ama-daemon/src/main.rs` (boot loads agents)

- [ ] **Step 1: Add agents field to AmaConfig**

Add to `AmaConfig` struct:

```rust
pub agents: HashMap<String, AgentConfig>,
pub default_agent_id: Option<String>,
```

- [ ] **Step 2: Update RawSlime to make domains/max_capacity optional (P1 backward compat)**

In `ama-core/src/config.rs`, update `RawSlime`:

```rust
#[derive(Deserialize)]
struct RawSlime {
    mode: String,
    #[serde(default)]
    max_capacity: Option<u64>,  // Now optional — required only if no agents/ dir
    #[serde(default)]
    domains: HashMap<String, RawDomainPolicy>,  // Now optional — moved to agents/
}
```

Update the validation in `load()` — only enforce `max_capacity > 0` when no agents/ dir exists:

```rust
// Remove this existing check:
// if raw_config.slime.max_capacity == 0 {
//     return Err(Self::boot_err("slime.max_capacity must be > 0".into()));
// }
// Replaced by conditional check below after agent loading.
```

- [ ] **Step 3: Update AmaConfig::load() to load agents**

In the `load()` method, after loading existing config files:

```rust
// ── Load agent configs ─────────────────────────────
let agents_dir = config_dir.join("agents");
let agents = if agents_dir.is_dir() {
    load_agent_configs(&agents_dir)?
} else {
    // P1 backward compat: synthesize agent from global [slime] config
    let max_cap = raw_config.slime.max_capacity.ok_or_else(|| {
        Self::boot_err("slime.max_capacity required when no config/agents/ directory exists".into())
    })?;
    if max_cap == 0 {
        return Err(Self::boot_err("slime.max_capacity must be > 0".into()));
    }
    if raw_config.slime.domains.is_empty() {
        return Err(Self::boot_err(
            "slime.domains required when no config/agents/ directory exists".into()));
    }
    let mut agents = HashMap::new();
    agents.insert("default".into(), AgentConfig {
        agent_id: "default".into(),
        max_capacity: max_cap,
        rate_limit_per_window: 60,
        rate_limit_window_secs: 60,
        domain_policies: domain_policies.clone(),
    });
    agents
};

let default_agent_id = if agents.len() == 1 {
    Some(agents.keys().next().unwrap().clone())
} else {
    None
};
```

Cross-validate: every domain_id referenced in agent configs must exist in `domains.toml` mappings:

```rust
for (agent_id, agent) in &agents {
    for domain_id in agent.domain_policies.keys() {
        let referenced = domain_mappings.values().any(|m| m.domain_id == *domain_id);
        if !referenced {
            return Err(Self::boot_err(format!(
                "agent '{}' references domain '{}' not mapped in domains.toml",
                agent_id, domain_id
            )));
        }
    }
}
```

- [ ] **Step 3: Update BootHashes to include agent config hashes**

```rust
#[derive(Debug, Clone)]
pub struct BootHashes {
    pub config_hash: String,
    pub domains_hash: String,
    pub intents_hash: String,
    pub allowlist_hash: String,
    pub agents_hash: String,  // SHA-256 of concatenated sorted agent file hashes
}
```

- [ ] **Step 4: Update main.rs boot logging**

```rust
tracing::info!(
    hashes = ?config.boot_hashes,
    agents = config.agents.len(),
    "Boot integrity verified"
);
```

- [ ] **Step 5: Run all tests**

```bash
cargo test --workspace --features test-utils
```

- [ ] **Step 6: Commit**

```bash
git add ama-core/src/config.rs ama-daemon/src/main.rs
git commit -m "feat(p2): integrate agent configs into AmaConfig boot with integrity hashes"
```

### Task 8: Create example agent config files

**Files:**
- Create: `config/agents/default.toml`

- [ ] **Step 1: Create agents directory and default config**

```bash
mkdir -p config/agents
```

```toml
# config/agents/default.toml
# Default agent — backward-compatible with P1 single-agent mode

[agent]
agent_id = "default"
max_capacity = 10000
rate_limit_per_window = 60
rate_limit_window_secs = 60

[agent.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100

[agent.domains.fs_read_workspace]
enabled = true
max_magnitude_per_action = 500

[agent.domains.proc_exec_bounded]
enabled = true
max_magnitude_per_action = 50

[agent.domains.net_out_http]
enabled = true
max_magnitude_per_action = 200
```

- [ ] **Step 2: Update config.toml — remove [slime.domains] section**

The `[slime]` section in `config.toml` keeps only global settings. Domain policies move to agent configs:

```toml
[ama]
workspace_root = "F:\\SYF PROJECT\\AMA\\workspace"
bind_host = "127.0.0.1"
bind_port = 8787

[slime]
mode = "embedded"
# max_capacity is now per-agent in config/agents/*.toml
# domains are now per-agent in config/agents/*.toml
```

**Note:** The `AmaConfig::load()` backward compat path handles missing agents/ dir by synthesizing from [slime.domains], so existing tests with inline configs still work.

- [ ] **Step 3: Commit**

```bash
git add config/agents/default.toml config/config.toml
git commit -m "feat(p2): add default agent config, move domain policies to per-agent"
```

---

## Chunk 3: Multi-Agent Authorizer

Replace the single `P0Authorizer` with a registry of per-agent authorizers. Each agent gets its own monotonic capacity counter.

### Task 9: Resolve DomainPolicy duplicate (I6)

**Files:**
- Modify: `ama-core/src/slime.rs`
- Modify: `ama-core/src/config.rs`

- [ ] **Step 1: Make slime.rs use config::DomainPolicy**

In `ama-core/src/slime.rs`, remove the local `DomainPolicy` struct and import from config:

```rust
use crate::config::DomainPolicy;
```

Remove from slime.rs:
```rust
// DELETE this:
// #[derive(Debug, Clone)]
// pub struct DomainPolicy {
//     pub enabled: bool,
//     pub max_magnitude_per_action: u64,
// }
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p ama-core
```

- [ ] **Step 3: Commit**

```bash
git add ama-core/src/slime.rs ama-core/src/config.rs
git commit -m "fix(p2): resolve DomainPolicy duplicate (I6) — single canonical type in config"
```

### Task 10: Create AgentAuthorizer (per-agent P0Authorizer wrapper)

**Files:**
- Modify: `ama-core/src/slime.rs`
- Test: `ama-core/tests/test_slime.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_agent_registry_independent_capacity() {
    use ama_core::slime::{AgentRegistry, SlimeAuthorizer};
    use ama_core::config::{AgentConfig, DomainPolicy};
    use std::collections::HashMap;

    let mut domain_policies = HashMap::new();
    domain_policies.insert("fs.write.workspace".into(), DomainPolicy {
        enabled: true,
        max_magnitude_per_action: 100,
    });

    let agents = vec![
        AgentConfig {
            agent_id: "agent_a".into(),
            max_capacity: 500,
            rate_limit_per_window: 60,
            rate_limit_window_secs: 60,
            domain_policies: domain_policies.clone(),
        },
        AgentConfig {
            agent_id: "agent_b".into(),
            max_capacity: 300,
            rate_limit_per_window: 60,
            rate_limit_window_secs: 60,
            domain_policies: domain_policies.clone(),
        },
    ];

    let registry = AgentRegistry::new(agents);

    // Agent A can reserve independently
    let auth_a = registry.get("agent_a").unwrap();
    assert_eq!(auth_a.try_reserve(&"fs.write.workspace".into(), 100),
        ama_core::slime::SlimeVerdict::Authorized);
    assert_eq!(auth_a.capacity_used(), 100);

    // Agent B is unaffected
    let auth_b = registry.get("agent_b").unwrap();
    assert_eq!(auth_b.capacity_used(), 0);
    assert_eq!(auth_b.try_reserve(&"fs.write.workspace".into(), 100),
        ama_core::slime::SlimeVerdict::Authorized);

    // Agent A can exhaust its own budget
    for _ in 0..4 {
        auth_a.try_reserve(&"fs.write.workspace".into(), 100);
    }
    assert_eq!(auth_a.capacity_used(), 500);
    assert_eq!(auth_a.try_reserve(&"fs.write.workspace".into(), 1),
        ama_core::slime::SlimeVerdict::Impossible);

    // Agent B still has budget
    assert_eq!(auth_b.try_reserve(&"fs.write.workspace".into(), 100),
        ama_core::slime::SlimeVerdict::Authorized);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p ama-core test_agent_registry_independent_capacity
```

- [ ] **Step 3: Implement AgentRegistry**

In `ama-core/src/slime.rs`:

```rust
use crate::config::{AgentConfig, DomainPolicy};

/// Registry of per-agent authorizers.
/// Each agent gets its own P0Authorizer with independent capacity.
pub struct AgentRegistry {
    agents: HashMap<String, P0Authorizer>,
}

impl AgentRegistry {
    pub fn new(configs: Vec<AgentConfig>) -> Self {
        let mut agents = HashMap::new();
        for config in configs {
            let domains: Vec<(DomainId, DomainPolicy)> = config.domain_policies
                .into_iter()
                .collect();
            let authorizer = P0Authorizer::new(config.max_capacity, domains);
            agents.insert(config.agent_id, authorizer);
        }
        Self { agents }
    }

    /// Get the authorizer for a specific agent.
    pub fn get(&self, agent_id: &str) -> Option<&P0Authorizer> {
        self.agents.get(agent_id)
    }

    /// List all registered agent IDs.
    pub fn agent_ids(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p ama-core test_agent_registry_independent_capacity
```

- [ ] **Step 5: Commit**

```bash
git add ama-core/src/slime.rs ama-core/tests/test_slime.rs
git commit -m "feat(p2): add AgentRegistry with per-agent P0Authorizer instances"
```

### Task 11: Update pipeline to accept trait object

**Files:**
- Modify: `ama-core/src/pipeline.rs`
- Test: `ama-core/tests/test_pipeline.rs`

- [ ] **Step 1: Change process_action signature**

Change `authorizer: &P0Authorizer` to `authorizer: &dyn SlimeAuthorizer`:

```rust
pub async fn process_action(
    request: ActionRequest,
    config: &AmaConfig,
    authorizer: &dyn SlimeAuthorizer,
    action_id: String,
    session_id: &str,
) -> Result<ActionResponse, AmaError> {
```

- [ ] **Step 2: Run all tests**

```bash
cargo test --workspace --features test-utils
```

This should pass immediately since `P0Authorizer` implements `SlimeAuthorizer`.

- [ ] **Step 3: Commit**

```bash
git add ama-core/src/pipeline.rs
git commit -m "refactor(p2): pipeline accepts &dyn SlimeAuthorizer for agent polymorphism"
```

---

## Chunk 4: X-Agent-Id Header and Per-Agent Routing

Add `X-Agent-Id` header extraction in the HTTP layer. Route each request to the correct agent's authorizer and rate limiter.

### Task 12: Add per-agent rate limiter

**Files:**
- Modify: `ama-daemon/src/server.rs`
- Test: `ama-daemon/tests/p2_rate_limit.rs`

- [ ] **Step 1: Write the failing test**

Create `ama-daemon/tests/p2_rate_limit.rs`:

```rust
//! P2 per-agent rate limiting tests.
use axum_test::TestServer;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_per_agent_rate_limits_are_independent() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_a", 10000, 5),  // agent_a: 5 req/window
        ("agent_b", 10000, 5),  // agent_b: 5 req/window
    ]).await;

    // Exhaust agent_a's rate limit
    for _ in 0..5 {
        let resp = server.post("/ama/action")
            .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
            .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
            .add_header("x-agent-id".parse().unwrap(), "agent_a".parse().unwrap())
            .json(&json!({
                "adapter": "test", "action": "file_write",
                "target": "test.txt", "magnitude": 1,
                "payload": "hello"
            }))
            .await;
        assert_eq!(resp.status_code(), 200);
    }

    // agent_a should be rate limited
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "agent_a".parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 429);

    // agent_b should still work
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "agent_b".parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 200);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p ama-daemon test_per_agent_rate_limits_are_independent --features test-utils
```

- [ ] **Step 3: Implement per-agent rate limiter in AppState**

Replace the single `Mutex<RateLimitState>` with a `HashMap<String, Mutex<RateLimitState>>`:

```rust
pub struct AppState {
    pub config: AmaConfig,
    pub agent_registry: AgentRegistry,
    pub idempotency_cache: IdempotencyCache,
    pub session_id: Uuid,
    pub start_time: Instant,
    pub domain_counters: HashMap<String, AtomicU64>,
    pub agent_rate_limiters: HashMap<String, std::sync::Mutex<RateLimitState>>,
}
```

Initialize per-agent rate limiters from agent configs:

```rust
impl AppState {
    pub fn new(config: AmaConfig) -> Arc<Self> {
        let agents: Vec<AgentConfig> = config.agents.values().cloned().collect();

        let mut agent_rate_limiters = HashMap::new();
        for agent in &config.agents {
            agent_rate_limiters.insert(agent.0.clone(), std::sync::Mutex::new(RateLimitState {
                window_start: Instant::now(),
                count: 0,
                max_per_window: agent.1.rate_limit_per_window,
                window_secs: agent.1.rate_limit_window_secs,
            }));
        }

        // Build domain_counters: union of all agents' domain policy keys
        let mut domain_counters = HashMap::new();
        for agent in config.agents.values() {
            for domain_id in agent.domain_policies.keys() {
                domain_counters.entry(domain_id.clone())
                    .or_insert_with(|| AtomicU64::new(0));
            }
        }

        Arc::new(Self {
            agent_registry: AgentRegistry::new(agents),
            idempotency_cache: IdempotencyCache::new(10_000, std::time::Duration::from_secs(300)),
            session_id: Uuid::new_v4(),
            start_time: Instant::now(),
            domain_counters,
            agent_rate_limiters,
            config,
        })
    }
}
```

Update `RateLimitState` to carry its own limits:

```rust
pub struct RateLimitState {
    pub window_start: Instant,
    pub count: u64,
    pub max_per_window: u64,
    pub window_secs: u64,
}
```

Update `check_rate_limit` to take agent_id:

```rust
fn check_rate_limit(state: &AppState, agent_id: &str) -> bool {
    let limiter = match state.agent_rate_limiters.get(agent_id) {
        Some(l) => l,
        None => return false,  // Unknown agent → deny
    };
    let mut rl = limiter.lock().unwrap();
    let now = Instant::now();
    let elapsed = now.duration_since(rl.window_start);

    if elapsed.as_secs() >= rl.window_secs {
        rl.window_start = now;
        rl.count = 1;
        return true;
    }

    rl.count += 1;
    rl.count <= rl.max_per_window
}
```

- [ ] **Step 4: Implement test_server_multiagent helper**

```rust
#[cfg(feature = "test-utils")]
pub async fn test_server_multiagent(
    agent_specs: Vec<(&str, u64, u64)>,  // (agent_id, capacity, rate_limit)
) -> axum_test::TestServer {
    use ama_core::config::{AmaConfig, AgentConfig, DomainPolicy, DomainMapping, BootHashes};

    let workspace = std::env::temp_dir().join(format!("ama-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&workspace).unwrap();

    let mut domain_policies = HashMap::new();
    domain_policies.insert("fs.write.workspace".into(), DomainPolicy {
        enabled: true,
        max_magnitude_per_action: 1000,
    });

    let mut agents = HashMap::new();
    for (agent_id, capacity, rate_limit) in agent_specs {
        agents.insert(agent_id.to_string(), AgentConfig {
            agent_id: agent_id.to_string(),
            max_capacity: capacity,
            rate_limit_per_window: rate_limit,
            rate_limit_window_secs: 60,
            domain_policies: domain_policies.clone(),
        });
    }

    let mut domain_mappings = HashMap::new();
    domain_mappings.insert("file_write".into(), DomainMapping {
        domain_id: "fs.write.workspace".into(),
        max_payload_bytes: Some(1_048_576),
        validator: None,
        requires_intent: false,
    });

    let config = AmaConfig {
        workspace_root: workspace,
        bind_host: "127.0.0.1".into(),
        bind_port: 8787,
        log_level: "info".into(),
        log_output: "stderr".into(),
        slime_mode: "embedded".into(),
        max_capacity: 0,  // Ignored in P2 — per-agent
        domain_policies: HashMap::new(),
        domain_mappings,
        intents: HashMap::new(),
        allowlist: vec![],
        boot_hashes: BootHashes {
            config_hash: "test".into(),
            domains_hash: "test".into(),
            intents_hash: "test".into(),
            allowlist_hash: "test".into(),
            agents_hash: "test".into(),
        },
        default_agent_id: if agents.len() == 1 {
            Some(agents.keys().next().unwrap().clone())
        } else {
            None
        },
        agents,
    };

    let state = AppState::new(config);
    let app = build_router(state);
    axum_test::TestServer::new(app.into_make_service()).unwrap()
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test -p ama-daemon test_per_agent_rate_limits_are_independent --features test-utils
```

- [ ] **Step 6: Commit**

```bash
git add ama-daemon/src/server.rs ama-daemon/tests/p2_rate_limit.rs
git commit -m "feat(p2): per-agent rate limiters with independent windows"
```

### Task 13: Extract X-Agent-Id header and route to agent

**Files:**
- Modify: `ama-daemon/src/server.rs`
- Test: `ama-daemon/tests/p2_agent_routing.rs`

- [ ] **Step 1: Write the failing tests**

Create `ama-daemon/tests/p2_agent_routing.rs`:

```rust
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_missing_agent_id_uses_default_when_single_agent() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("default", 10000, 60),
    ]).await;
    // No X-Agent-Id header — should use default
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 200);
}

#[tokio::test]
async fn test_missing_agent_id_rejected_when_multi_agent() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_a", 10000, 60),
        ("agent_b", 10000, 60),
    ]).await;
    // No X-Agent-Id header with multiple agents → 400
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 400);
}

#[tokio::test]
async fn test_unknown_agent_id_rejected() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_a", 10000, 60),
    ]).await;
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "unknown_agent".parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "test.txt", "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    assert_eq!(resp.status_code(), 400);
}

#[tokio::test]
async fn test_valid_agent_id_routes_to_correct_budget() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("small", 10, 60),   // tiny budget
        ("large", 10000, 60),
    ]).await;

    // Small agent: exhaust capacity
    for i in 0..10 {
        let resp = server.post("/ama/action")
            .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
            .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
            .add_header("x-agent-id".parse().unwrap(), "small".parse().unwrap())
            .json(&json!({
                "adapter": "test", "action": "file_write",
                "target": format!("test{i}.txt"), "magnitude": 1,
                "payload": "x"
            }))
            .await;
        assert_eq!(resp.status_code(), 200);
    }

    // Small agent should now get 403 (impossible — capacity exhausted)
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "small".parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "overflow.txt", "magnitude": 1,
            "payload": "x"
        }))
        .await;
    assert_eq!(resp.status_code(), 403);

    // Large agent should still work fine
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "large".parse().unwrap())
        .json(&json!({
            "adapter": "test", "action": "file_write",
            "target": "large.txt", "magnitude": 1,
            "payload": "x"
        }))
        .await;
    assert_eq!(resp.status_code(), 200);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ama-daemon p2_agent_routing --features test-utils
```

- [ ] **Step 3: Implement X-Agent-Id extraction in handle_action**

Update `handle_action` in `ama-daemon/src/server.rs`:

```rust
async fn handle_action(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 0. Resolve agent_id from X-Agent-Id header
    let agent_id = match resolve_agent_id(&headers, &state) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // 1. Per-agent rate limit
    if !check_rate_limit(&state, &agent_id) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"status": "error", "error_class": "rate_limited",
                "message": format!("rate limit exceeded for agent '{}'", agent_id)})),
        ).into_response();
    }

    // ... rest of handler unchanged, but use agent's authorizer:

    // 6. Get agent's authorizer
    let authorizer = match state.agent_registry.get(&agent_id) {
        Some(a) => a,
        None => return AmaError::BadRequest {
            message: format!("unknown agent: {}", agent_id),
        }.into_response(),
    };

    // 7. Process through pipeline (now with agent's authorizer)
    let result = process_action(
        request,
        &state.config,
        authorizer,
        action_id,
        &state.session_id.to_string(),
    ).await;

    // ... response building unchanged ...
}

/// Resolve agent_id from X-Agent-Id header.
/// - Present and valid → use it
/// - Absent + single agent → use default
/// - Absent + multi agent → 400
/// - Present but unknown → 400
fn resolve_agent_id(
    headers: &axum::http::HeaderMap,
    state: &AppState,
) -> Result<String, Response> {
    match headers.get("x-agent-id") {
        Some(val) => {
            let agent_id = val.to_str().map_err(|_| {
                AmaError::BadRequest {
                    message: "X-Agent-Id header is not valid ASCII".into(),
                }.into_response()
            })?;
            if state.agent_registry.get(agent_id).is_none() {
                return Err(AmaError::BadRequest {
                    message: format!("unknown agent: {}", agent_id),
                }.into_response());
            }
            Ok(agent_id.to_string())
        }
        None => {
            match &state.config.default_agent_id {
                Some(default) => Ok(default.clone()),
                None => Err(AmaError::BadRequest {
                    message: "X-Agent-Id header required (multiple agents configured)".into(),
                }.into_response()),
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p ama-daemon p2_agent_routing --features test-utils
```

- [ ] **Step 5: Run all existing tests to verify no regressions**

```bash
cargo test --workspace --features test-utils
```

- [ ] **Step 6: Commit**

```bash
git add ama-daemon/src/server.rs ama-daemon/tests/p2_agent_routing.rs
git commit -m "feat(p2): X-Agent-Id header routing with per-agent capacity isolation"
```

### Task 14: Update /ama/status endpoint for multi-agent

**Files:**
- Modify: `ama-daemon/src/server.rs`

- [ ] **Step 1: Update handle_status to show per-agent capacity**

```rust
async fn handle_status(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();

    let mut agents_status = serde_json::Map::new();
    for agent_id in state.agent_registry.agent_ids() {
        if let Some(auth) = state.agent_registry.get(agent_id) {
            agents_status.insert(agent_id.to_string(), json!({
                "capacity_used": auth.capacity_used(),
                "capacity_max": auth.capacity_max(),
                "capacity_remaining": auth.capacity_max().saturating_sub(auth.capacity_used()),
            }));
        }
    }

    let mut domains = serde_json::Map::new();
    for (domain_id, counter) in &state.domain_counters {
        let count = counter.load(Ordering::Relaxed);
        domains.insert(domain_id.clone(), json!({
            "actions_count": count,
        }));
    }

    Json(json!({
        "session_id": state.session_id.to_string(),
        "uptime_seconds": uptime,
        "agents": agents_status,
        "domains": domains,
    }))
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --workspace --features test-utils
```

- [ ] **Step 3: Commit**

```bash
git add ama-daemon/src/server.rs
git commit -m "feat(p2): /ama/status shows per-agent capacity breakdown"
```

### Task 15: Update existing test_server helper for backward compat

**Files:**
- Modify: `ama-daemon/src/server.rs`

- [ ] **Step 1: Update test_server() to use single-agent mode**

```rust
#[cfg(feature = "test-utils")]
pub async fn test_server() -> axum_test::TestServer {
    test_server_multiagent(vec![("default", 10_000, 60)]).await
}

#[cfg(feature = "test-utils")]
pub async fn test_server_with_capacity(max_capacity: u64) -> axum_test::TestServer {
    test_server_multiagent(vec![("default", max_capacity, 60)]).await
}
```

- [ ] **Step 2: Run all tests — verify backward compat**

```bash
cargo test --workspace --features test-utils
```

All existing P1 tests should pass without modifications because they don't send `X-Agent-Id` and there's exactly one agent ("default"), so the default agent path kicks in.

- [ ] **Step 3: Commit**

```bash
git add ama-daemon/src/server.rs
git commit -m "refactor(p2): update test helpers for backward compat with single-agent default"
```

---

## Chunk 5: P2 Adversarial and Integration Tests

Comprehensive tests for multi-agent edge cases and cross-agent isolation.

### Task 16: Cross-agent isolation adversarial tests

**Files:**
- Create: `ama-daemon/tests/p2_adversarial.rs`

- [ ] **Step 1: Write adversarial tests**

```rust
//! P2 adversarial tests — cross-agent isolation.

use serde_json::json;
use uuid::Uuid;

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
    ]).await;

    // Exhaust agent_a
    for i in 0..5 {
        let resp = server.post("/ama/action")
            .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
            .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
            .add_header("x-agent-id".parse().unwrap(), "isolated_a".parse().unwrap())
            .json(&action_request(&format!("a{i}.txt")))
            .await;
        assert_eq!(resp.status_code(), 200, "agent_a request {i} should succeed");
    }

    // agent_a is exhausted
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "isolated_a".parse().unwrap())
        .json(&action_request("overflow.txt"))
        .await;
    assert_eq!(resp.status_code(), 403);

    // agent_b is unaffected
    for i in 0..5 {
        let resp = server.post("/ama/action")
            .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
            .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
            .add_header("x-agent-id".parse().unwrap(), "isolated_b".parse().unwrap())
            .json(&action_request(&format!("b{i}.txt")))
            .await;
        assert_eq!(resp.status_code(), 200, "agent_b request {i} should succeed");
    }
}

/// Same idempotency key across different agents returns cached result
/// (global idempotency cache). This means agent_y gets agent_x's cached
/// result without capacity charge — accepted tradeoff in P2. If this
/// becomes a concern, P3 can key the cache on (agent_id, uuid) instead.
#[tokio::test]
async fn test_idempotency_key_global_across_agents() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("agent_x", 10000, 60),
        ("agent_y", 10000, 60),
    ]).await;

    let shared_key = Uuid::new_v4().to_string();

    // First request with agent_x
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), shared_key.parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "agent_x".parse().unwrap())
        .json(&action_request("shared.txt"))
        .await;
    assert_eq!(resp.status_code(), 200);

    // Same key with agent_y → should return cached result (200, not re-execute)
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), shared_key.parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "agent_y".parse().unwrap())
        .json(&action_request("shared.txt"))
        .await;
    assert_eq!(resp.status_code(), 200);  // Cached replay
}

/// X-Agent-Id header injection attempts.
#[tokio::test]
async fn test_agent_id_injection_rejected() {
    let server = ama_daemon::server::test_server_multiagent(vec![
        ("legit", 10000, 60),
    ]).await;

    // Newline injection
    let resp = server.post("/ama/action")
        .add_header("content-type".parse().unwrap(), "application/json".parse().unwrap())
        .add_header("idempotency-key".parse().unwrap(), Uuid::new_v4().to_string().parse().unwrap())
        .add_header("x-agent-id".parse().unwrap(), "legit\r\nX-Injected: true".parse::<axum::http::HeaderValue>().unwrap_or_else(|_| "bad".parse().unwrap()))
        .json(&action_request("inject.txt"))
        .await;
    // Should be 400 (unknown agent) since the injected value won't match
    assert_ne!(resp.status_code(), 200);
}
```

- [ ] **Step 2: Run adversarial tests**

```bash
cargo test -p ama-daemon p2_adversarial --features test-utils
```

- [ ] **Step 3: Commit**

```bash
git add ama-daemon/tests/p2_adversarial.rs
git commit -m "test(p2): adversarial tests for cross-agent isolation and global idempotency"
```

### Task 17: Final integration pass

- [ ] **Step 1: Run full test suite**

```bash
cargo test --workspace --features test-utils
```

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace --features test-utils -- -D warnings
```

- [ ] **Step 3: Verify binary runs**

```bash
cd ama-daemon && cargo run -- --help 2>&1 || true
cargo build --workspace --release
```

- [ ] **Step 4: Update version**

In both `ama-core/Cargo.toml` and `ama-daemon/Cargo.toml`:

```toml
version = "0.2.0-p2-held"
```

- [ ] **Step 5: Commit final P2**

```bash
git add -A
git commit -m "feat(p2): AMA P2 complete — multi-agent capacity system with per-agent budgets

- Workspace split: ama-core (pure library) + ama-daemon (HTTP wrapper)
- Per-agent capacity configs from config/agents/*.toml
- X-Agent-Id header for agent context selection
- Per-agent rate limiters (independent windows)
- AgentRegistry with independent P0Authorizer per agent
- Global idempotency cache (cross-agent)
- Boot integrity includes agent config SHA-256 hashes
- Backward-compatible single-agent default mode
- DomainPolicy I6 duplicate resolved"
```

---

## Summary of Files Changed

### New Files
| File | Purpose |
|------|---------|
| `Cargo.toml` (workspace root) | Workspace manifest |
| `ama-core/Cargo.toml` | Core library crate manifest |
| `ama-core/src/lib.rs` | Core library module declarations |
| `ama-daemon/Cargo.toml` | HTTP daemon crate manifest |
| `ama-daemon/src/error_response.rs` | AmaError → HTTP response conversion (orphan-safe) |
| `ama-daemon/src/lib.rs` | Daemon library module declarations |
| `config/agents/default.toml` | Default agent capacity config |
| `ama-daemon/tests/p2_rate_limit.rs` | Per-agent rate limit tests |
| `ama-daemon/tests/p2_agent_routing.rs` | X-Agent-Id routing tests |
| `ama-daemon/tests/p2_adversarial.rs` | Cross-agent adversarial tests |

### Modified Files
| File | Changes |
|------|---------|
| `ama-core/src/config.rs` | +AgentConfig, +load_agent_configs(), agents field in AmaConfig, agents_hash in BootHashes |
| `ama-core/src/slime.rs` | Remove DomainPolicy (use config::), +AgentRegistry |
| `ama-core/src/pipeline.rs` | process_action takes `&dyn SlimeAuthorizer` |
| `ama-daemon/src/server.rs` | AppState gets AgentRegistry, per-agent rate limiters, X-Agent-Id routing, updated /ama/status |
| `ama-daemon/src/main.rs` | Updated imports for ama-core |
| `config/config.toml` | Removed [slime.domains] (moved to agents/) |

### Moved Files (src/ → ama-core/src/ or ama-daemon/src/)
All 12 source modules + actuator directory moved to ama-core. server.rs + main.rs moved to ama-daemon. All 17 test files split between ama-core/tests/ (11 unit) and ama-daemon/tests/ (6 integration).

### Deleted Files
| File | Reason |
|------|--------|
| `src/` (old root) | Replaced by ama-core/src/ and ama-daemon/src/ |
| `tests/` (old root) | Split into ama-core/tests/ and ama-daemon/tests/ |

---

## What P2 Does NOT Include (Deferred to P2.5+)

1. **Hot-reload of agent configs** — restart required to add/remove agents
2. **Per-agent workspace isolation** — all agents share the same workspace_root (P3)
3. **Agent authentication/identity binding** — X-Agent-Id is trust-based context selection
4. **Multi-adapter capability routing** — still 4 fixed adapters (file_write, file_read, shell_exec, http_request)
5. **Capability manifest / admission laws** — P3 feature (GPT's original prompt item)
6. **Windowed capacity reset** — capacity stays monotonic (thermodynamic), no time-based reset
7. **C1 (WorkspacePath TOCTOU/symlink)** — deferred from P1, still deferred
8. **I5 (test helper multi-adapter)** — partially addressed by test_server_multiagent but still only file_write
