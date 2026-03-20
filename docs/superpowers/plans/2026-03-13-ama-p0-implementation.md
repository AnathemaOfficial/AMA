# SAFA P0 Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build SAFA P0 — a deterministic security membrane between AI agents and real-world actuation (filesystem, shell, HTTP).

**Architecture:** HTTP server on 127.0.0.1:8787 receives JSON actions, validates via newtypes, maps to SLIME domains, authorizes via embedded AB-S (atomic CAS capacity), actuates if authorized. Fail-closed everywhere.

**Tech Stack:** Rust 1.93, axum, tokio, serde, toml, uuid, sha2, reqwest, thiserror, tracing

**Spec:** `docs/superpowers/specs/2026-03-13-ama-p0-design.md`

**Platform:** Linux primary (POSIX: execv, setpgid, lstat). Compiles on Windows for dev, but shell/process actuators are Linux-only.

---

## File Structure

```
SAFA/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point: load config, build state, start server
│   ├── config.rs             # TOML loading, validation, SHA-256 hashing
│   ├── errors.rs             # AmaError enum, HTTP status mapping
│   ├── newtypes.rs           # WorkspacePath, IntentId, AllowlistedUrl, BoundedBytes, SafeArg, HttpMethod
│   ├── canonical.rs          # CanonicalAction enum, ActionResult enum
│   ├── schema.rs             # JSON request/response structs, deserialization into CanonicalAction
│   ├── mapper.rs             # Action -> (domain_id, magnitude) mapping
│   ├── slime.rs              # SlimeAuthorizer trait, P0Authorizer (AtomicU64 CAS)
│   ├── pipeline.rs           # Full request pipeline: validate -> map -> authorize -> actuate
│   ├── idempotency.rs        # Idempotency-Key cache (DashMap, 5-min TTL, 10K cap)
│   ├── server.rs             # Axum router, endpoints, rate limiter, middleware
│   ├── audit.rs              # Structured audit log, request_hash (SHA-256)
│   └── actuator/
│       ├── mod.rs            # Actuator dispatcher (match on CanonicalAction)
│       ├── file.rs           # FileWrite + FileRead (atomic rename, lstat, bounded read)
│       ├── shell.rs          # ShellExec (execv, setpgid, kill sequence) — Linux only
│       └── http.rs           # HTTP actuator (reqwest, DNS/IP safety, redirect validation)
├── config/
│   ├── config.toml
│   ├── domains.toml
│   ├── intents.toml
│   └── allowlist.toml
└── tests/
    ├── test_newtypes.rs      # Adversarial newtype construction tests
    ├── test_schema.rs        # JSON parsing tests (valid + invalid)
    ├── test_slime.rs         # CAS capacity tests, concurrent stress
    ├── test_pipeline.rs      # Full pipeline unit tests (dry_run, impossible, authorized)
    ├── test_actuator_file.rs # File read/write with symlinks, traversal, atomicity
    └── test_integration.rs   # HTTP integration tests (axum test client)
```

---

## Chunk 1: Foundation

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/errors.rs`
- Create: `config/config.toml`, `config/domains.toml`, `config/intents.toml`, `config/allowlist.toml`

- [ ] **Step 1: Initialize Cargo project**

```bash
cd "F:/SYF PROJECT/SAFA"
cargo init --name ama
```

- [ ] **Step 2: Write Cargo.toml with dependencies**

```toml
[package]
name = "ama"
version = "0.1.0"
edition = "2021"
description = "SLIME Adapter for Agents — Deterministic security membrane for AI agents"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
uuid = { version = "1", features = ["v4"] }
sha2 = "0.10"
reqwest = { version = "0.12", features = ["rustls-tls"], default-features = false }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
dashmap = "6"
tower = { version = "0.5", features = ["limit"] }
tower-http = { version = "0.6", features = ["limit"] }
bytes = "1"

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[dev-dependencies]
axum-test = "16"
tempfile = "3"
tokio = { version = "1", features = ["full", "test-util"] }
```

- [ ] **Step 3: Write src/errors.rs**

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

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

impl IntoResponse for AmaError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AmaError::Impossible => (
                StatusCode::FORBIDDEN,
                json!({"status": "impossible"}),
            ),
            AmaError::BadRequest { message } => (
                StatusCode::BAD_REQUEST,
                json!({"status": "error", "error_class": "bad_request", "message": message}),
            ),
            AmaError::Validation { error_class, message } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({"status": "error", "error_class": error_class, "message": message}),
            ),
            AmaError::Conflict { message } => (
                StatusCode::CONFLICT,
                json!({"status": "error", "error_class": "conflict", "message": message}),
            ),
            AmaError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"status": "error", "error_class": "payload_too_large", "message": "payload exceeds limit"}),
            ),
            AmaError::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                json!({"status": "error", "error_class": "unsupported_media_type", "message": "expected application/json"}),
            ),
            AmaError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                json!({"status": "error", "error_class": "rate_limited", "message": "rate limit exceeded"}),
            ),
            AmaError::ServiceUnavailable { message } => (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"status": "error", "error_class": "service_unavailable", "message": message}),
            ),
        };
        (status, axum::Json(body)).into_response()
    }
}
```

- [ ] **Step 4: Write minimal main.rs that compiles**

```rust
mod errors;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("SAFA P0 starting...");
    // Config loading, server setup will be added in later tasks
}
```

- [ ] **Step 5: Write all 4 config files from the spec**

Copy the exact TOML from spec sections 4.1-4.3 and 6 into:
- `config/config.toml` (use `workspace_root = "/tmp/ama-workspace"` for dev)
- `config/domains.toml`
- `config/intents.toml`
- `config/allowlist.toml`

- [ ] **Step 6: Verify it compiles**

```bash
cargo build
```
Expected: Compiles with warnings about unused imports.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: project scaffold with dependencies and error types"
```

---

### Task 2: Newtypes (Security Core)

**Files:**
- Create: `src/newtypes.rs`
- Create: `tests/test_newtypes.rs`

- [ ] **Step 1: Write adversarial tests FIRST (TDD)**

```rust
// tests/test_newtypes.rs
use ama::newtypes::*;
use std::path::PathBuf;

#[test]
fn workspace_path_rejects_traversal() {
    let root = PathBuf::from("/tmp/ama-workspace");
    assert!(WorkspacePath::new("../../etc/passwd", &root).is_err());
    assert!(WorkspacePath::new("../outside", &root).is_err());
    assert!(WorkspacePath::new("foo/../../bar", &root).is_err());
}

#[test]
fn workspace_path_rejects_absolute() {
    let root = PathBuf::from("/tmp/ama-workspace");
    assert!(WorkspacePath::new("/etc/passwd", &root).is_err());
}

#[test]
fn workspace_path_accepts_valid() {
    let root = PathBuf::from("/tmp/ama-workspace");
    // This test needs a real directory — use tempdir in real test
    // For now, test the lexical validation only
    assert!(WorkspacePath::new("", &root).is_err()); // empty
}

#[test]
fn bounded_bytes_rejects_oversized() {
    let data = "x".repeat(1_048_577); // 1 MiB + 1
    assert!(BoundedBytes::new(data, 1_048_576).is_err());
}

#[test]
fn bounded_bytes_accepts_within_limit() {
    let data = "hello world".to_string();
    assert!(BoundedBytes::new(data, 1_048_576).is_ok());
}

#[test]
fn safe_arg_rejects_null_bytes() {
    assert!(SafeArg::new("hello\0world").is_err());
}

#[test]
fn safe_arg_rejects_empty() {
    assert!(SafeArg::new("").is_err());
}

#[test]
fn safe_arg_accepts_valid() {
    assert!(SafeArg::new("src/main.rs").is_ok());
}

#[test]
fn intent_id_rejects_traversal() {
    assert!(IntentId::new("../bad").is_err());
    assert!(IntentId::new("good_intent").is_ok());
}

#[test]
fn allowlisted_url_rejects_http() {
    assert!(AllowlistedUrl::new("http://example.com", &[]).is_err());
}

#[test]
fn allowlisted_url_rejects_not_in_list() {
    let patterns = vec![AllowlistEntry {
        pattern: "https://api.github.com/*".to_string(),
        methods: vec!["GET".to_string()],
        max_body_bytes: None,
    }];
    assert!(AllowlistedUrl::new("https://evil.com/steal", &patterns).is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --test test_newtypes
```
Expected: FAIL — module not found.

- [ ] **Step 3: Implement newtypes.rs**

```rust
// src/newtypes.rs
use crate::errors::AmaError;
use std::path::{Path, PathBuf};

/// A path guaranteed to be inside workspace_root, with no traversal or symlinks.
#[derive(Debug, Clone)]
pub struct WorkspacePath {
    canonical: PathBuf,
    relative: String,
}

impl WorkspacePath {
    pub fn new(relative: &str, workspace_root: &Path) -> Result<Self, AmaError> {
        if relative.is_empty() {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "empty path".into(),
            });
        }
        // Reject absolute paths
        if relative.starts_with('/') || relative.starts_with('\\') || relative.contains(':') {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "absolute paths forbidden".into(),
            });
        }
        // Reject .. segments
        for segment in relative.split(['/', '\\']) {
            if segment == ".." {
                return Err(AmaError::Validation {
                    error_class: "invalid_target".into(),
                    message: "path traversal forbidden".into(),
                });
            }
            if segment.is_empty() && relative.contains("//") {
                return Err(AmaError::Validation {
                    error_class: "invalid_target".into(),
                    message: "ambiguous path segment".into(),
                });
            }
        }
        let joined = workspace_root.join(relative);
        // Lexical canonicalization (full symlink check done at actuation time on Linux)
        let canonical = joined;
        Ok(Self {
            canonical,
            relative: relative.to_string(),
        })
    }

    pub fn canonical(&self) -> &Path { &self.canonical }
    pub fn relative(&self) -> &str { &self.relative }
}

/// Bytes guaranteed to be valid UTF-8 and within size limit.
#[derive(Debug, Clone)]
pub struct BoundedBytes(String);

impl BoundedBytes {
    pub fn new(data: String, max_bytes: usize) -> Result<Self, AmaError> {
        if data.len() > max_bytes {
            return Err(AmaError::Validation {
                error_class: "payload_too_large".into(),
                message: format!("payload {} bytes exceeds limit {}", data.len(), max_bytes),
            });
        }
        Ok(Self(data))
    }

    pub fn as_str(&self) -> &str { &self.0 }
    pub fn len(&self) -> usize { self.0.len() }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
}

/// A shell argument guaranteed to have no null bytes and be non-empty.
#[derive(Debug, Clone)]
pub struct SafeArg(String);

impl SafeArg {
    pub fn new(arg: &str) -> Result<Self, AmaError> {
        if arg.is_empty() {
            return Err(AmaError::Validation {
                error_class: "invalid_args".into(),
                message: "empty argument".into(),
            });
        }
        if arg.contains('\0') {
            return Err(AmaError::Validation {
                error_class: "invalid_args".into(),
                message: "null byte in argument".into(),
            });
        }
        Ok(Self(arg.to_string()))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

/// An intent ID that exists in intents.toml. Alphanumeric + underscore only.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntentId(String);

impl IntentId {
    pub fn new(id: &str) -> Result<Self, AmaError> {
        if id.is_empty() {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "empty intent id".into(),
            });
        }
        if !id.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "intent id must be alphanumeric/underscore".into(),
            });
        }
        Ok(Self(id.to_string()))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

/// Allowlist entry for URL matching.
#[derive(Debug, Clone)]
pub struct AllowlistEntry {
    pub pattern: String,
    pub methods: Vec<String>,
    pub max_body_bytes: Option<usize>,
}

/// A URL guaranteed to be HTTPS and matched against the allowlist.
#[derive(Debug, Clone)]
pub struct AllowlistedUrl {
    url: String,
}

impl AllowlistedUrl {
    pub fn new(url: &str, allowlist: &[AllowlistEntry]) -> Result<Self, AmaError> {
        // Must be https
        if !url.starts_with("https://") {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "only https URLs allowed".into(),
            });
        }
        // Reject userinfo
        if let Some(authority) = url.strip_prefix("https://") {
            if authority.contains('@') {
                return Err(AmaError::Validation {
                    error_class: "invalid_target".into(),
                    message: "userinfo in URL forbidden".into(),
                });
            }
        }
        // Reject fragments
        if url.contains('#') {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "fragments in URL forbidden".into(),
            });
        }
        // Match against allowlist (glob)
        let matched = allowlist.iter().any(|entry| {
            glob_match(&entry.pattern, url)
        });
        if !matched {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "URL not in allowlist".into(),
            });
        }
        Ok(Self { url: url.to_string() })
    }

    pub fn as_str(&self) -> &str { &self.url }
}

/// Simple glob matching: `*` matches any suffix.
fn glob_match(pattern: &str, url: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        url == pattern
    }
}

/// HTTP method — GET or POST only in P0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub fn parse(s: &str) -> Result<Self, AmaError> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            _ => Err(AmaError::Validation {
                error_class: "invalid_method".into(),
                message: format!("unsupported method: {}", s),
            }),
        }
    }
}
```

- [ ] **Step 4: Add `pub mod newtypes;` to main.rs, add `lib.rs`**

Create `src/lib.rs`:
```rust
pub mod errors;
pub mod newtypes;
```

Update `src/main.rs` to use lib:
```rust
use ama::errors;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("SAFA P0 starting...");
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test --test test_newtypes
```
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: security newtypes with adversarial tests (TDD)"
```

---

### Task 3: CanonicalAction Enum & JSON Schema

**Files:**
- Create: `src/canonical.rs`
- Create: `src/schema.rs`
- Create: `tests/test_schema.rs`

- [ ] **Step 1: Write failing tests for JSON deserialization**

```rust
// tests/test_schema.rs
use ama::schema::*;

#[test]
fn parses_valid_file_write() {
    let json = r#"{
        "adapter": "generic",
        "action": "file_write",
        "target": "test.txt",
        "magnitude": 1,
        "payload": "hello"
    }"#;
    let req: ActionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.action, "file_write");
    assert_eq!(req.magnitude, 1);
}

#[test]
fn rejects_missing_action() {
    let json = r#"{"adapter": "x", "target": "t", "magnitude": 1}"#;
    let result: Result<ActionRequest, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn rejects_magnitude_zero() {
    let json = r#"{
        "adapter": "x", "action": "file_write",
        "target": "t", "magnitude": 0, "payload": "x"
    }"#;
    let req: ActionRequest = serde_json::from_str(json).unwrap();
    assert!(validate_magnitude(req.magnitude).is_err());
}

#[test]
fn rejects_magnitude_over_1000() {
    let json = r#"{
        "adapter": "x", "action": "file_write",
        "target": "t", "magnitude": 1001, "payload": "x"
    }"#;
    let req: ActionRequest = serde_json::from_str(json).unwrap();
    assert!(validate_magnitude(req.magnitude).is_err());
}

#[test]
fn parses_shell_exec_with_args() {
    let json = r#"{
        "adapter": "generic",
        "action": "shell_exec",
        "target": "list_dir",
        "magnitude": 1,
        "args": ["src"]
    }"#;
    let req: ActionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.args, Some(vec!["src".to_string()]));
}

#[test]
fn parses_http_request_with_method() {
    let json = r#"{
        "adapter": "generic",
        "action": "http_request",
        "method": "GET",
        "target": "https://api.github.com/repos",
        "magnitude": 1
    }"#;
    let req: ActionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, Some("GET".to_string()));
}
```

- [ ] **Step 2: Run tests, verify fail**

```bash
cargo test --test test_schema
```

- [ ] **Step 3: Implement schema.rs (JSON request/response structs)**

```rust
// src/schema.rs
use crate::errors::AmaError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ActionRequest {
    pub adapter: String,
    pub action: String,
    pub target: String,
    pub magnitude: u64,
    #[serde(default)]
    pub dry_run: bool,
    pub method: Option<String>,
    pub payload: Option<String>,
    pub args: Option<Vec<String>>,
}

pub fn validate_magnitude(magnitude: u64) -> Result<(), AmaError> {
    if magnitude < 1 || magnitude > 1000 {
        return Err(AmaError::Validation {
            error_class: "invalid_magnitude".into(),
            message: format!("magnitude must be 1-1000, got {}", magnitude),
        });
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub status: String,
    pub action_id: String,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: String,
    pub error_class: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub name: String,
    pub version: String,
    pub schema_version: String,
}
```

- [ ] **Step 4: Implement canonical.rs**

```rust
// src/canonical.rs
use crate::newtypes::*;

/// Type-safe canonical action. If this exists, it is structurally valid.
#[derive(Debug)]
pub enum CanonicalAction {
    FileWrite {
        path: WorkspacePath,
        content: BoundedBytes,
    },
    FileRead {
        path: WorkspacePath,
    },
    ShellExec {
        intent: IntentId,
        args: Vec<SafeArg>,
    },
    HttpRequest {
        method: HttpMethod,
        url: AllowlistedUrl,
        body: Option<BoundedBytes>,
    },
}

/// Typed result from actuation.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type")]
pub enum ActionResult {
    #[serde(rename = "file_write")]
    FileWrite { bytes_written: u64 },
    #[serde(rename = "file_read")]
    FileRead {
        content: String,
        bytes_returned: u64,
        total_bytes: u64,
        truncated: bool,
    },
    #[serde(rename = "shell_exec")]
    ShellExec {
        stdout: String,
        stderr: String,
        exit_code: i32,
        truncated: bool,
    },
    #[serde(rename = "http_response")]
    HttpResponse {
        status_code: u16,
        body: String,
        truncated: bool,
    },
}
```

- [ ] **Step 5: Add modules to lib.rs**

```rust
pub mod errors;
pub mod newtypes;
pub mod canonical;
pub mod schema;
```

- [ ] **Step 6: Run tests**

```bash
cargo test --test test_schema
```
Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: canonical action model and JSON schema (TDD)"
```

---

### Task 4: Config Loading & Boot Validation

**Files:**
- Create: `src/config.rs`
- Create: `tests/test_config.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/test_config.rs
use ama::config::*;
use tempfile::TempDir;
use std::fs;

#[test]
fn loads_valid_config() {
    let dir = TempDir::new().unwrap();
    let ws = dir.path().join("workspace");
    fs::create_dir(&ws).unwrap();

    write_test_configs(dir.path(), ws.to_str().unwrap());

    let result = AmaConfig::load(dir.path());
    assert!(result.is_ok());
}

#[test]
fn rejects_missing_config_file() {
    let dir = TempDir::new().unwrap();
    let result = AmaConfig::load(dir.path());
    assert!(result.is_err());
}

#[test]
fn rejects_relative_workspace_root() {
    let dir = TempDir::new().unwrap();
    write_test_configs(dir.path(), "./relative");
    let result = AmaConfig::load(dir.path());
    assert!(result.is_err());
}

#[test]
fn computes_sha256_hashes() {
    let dir = TempDir::new().unwrap();
    let ws = dir.path().join("workspace");
    fs::create_dir(&ws).unwrap();
    write_test_configs(dir.path(), ws.to_str().unwrap());

    let config = AmaConfig::load(dir.path()).unwrap();
    assert!(!config.boot_hashes.config_hash.is_empty());
    assert!(!config.boot_hashes.domains_hash.is_empty());
}

/// Helper: write minimal valid config files into a directory.
fn write_test_configs(dir: &std::path::Path, workspace_root: &str) {
    fs::write(dir.join("config.toml"), format!(r#"
[ama]
workspace_root = "{workspace_root}"
bind_host = "127.0.0.1"
bind_port = 8787

[slime]
mode = "embedded"
max_capacity = 10000

[slime.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100

[slime.domains.fs_read_workspace]
enabled = true
max_magnitude_per_action = 500

[slime.domains.proc_exec_bounded]
enabled = true
max_magnitude_per_action = 50

[slime.domains.net_out_http]
enabled = true
max_magnitude_per_action = 200
"#)).unwrap();

    fs::write(dir.join("domains.toml"), r#"
[meta]
schema_version = "ama-domains-v1"
max_magnitude_claim = 1000

[domains.file_write]
domain_id = "fs.write.workspace"
max_payload_bytes = 1048576
validator = "relative_workspace_path"

[domains.file_read]
domain_id = "fs.read.workspace"
validator = "relative_workspace_path"

[domains.shell_exec]
domain_id = "proc.exec.bounded"
requires_intent = true

[domains.http_request]
domain_id = "net.out.http"
max_payload_bytes = 262144
validator = "allowlisted_url"
"#).unwrap();

    fs::write(dir.join("intents.toml"), r#"
[meta]
schema_version = "ama-intents-v1"
"#).unwrap();

    fs::write(dir.join("allowlist.toml"), r#"
[meta]
schema_version = "ama-allowlist-v1"
"#).unwrap();
}
```

- [ ] **Step 2: Run tests, verify fail**

```bash
cargo test --test test_config
```

- [ ] **Step 3: Implement config.rs**

```rust
// src/config.rs
use crate::errors::AmaError;
use crate::newtypes::AllowlistEntry;
use sha2::{Sha256, Digest};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ── Boot Hashes ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BootHashes {
    pub config_hash: String,
    pub domains_hash: String,
    pub intents_hash: String,
    pub allowlist_hash: String,
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

// ── TOML raw structs (serde) ─────────────────────────────────

#[derive(Deserialize)]
struct RawConfig {
    ama: RawAma,
    slime: RawSlime,
}

#[derive(Deserialize)]
struct RawAma {
    workspace_root: String,
    #[serde(default = "default_host")]
    bind_host: String,
    #[serde(default = "default_port")]
    bind_port: u16,
    #[serde(default = "default_log_level")]
    log_level: String,
    #[serde(default = "default_log_output")]
    log_output: String,
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 8787 }
fn default_log_level() -> String { "info".into() }
fn default_log_output() -> String { "stderr".into() }

#[derive(Deserialize)]
struct RawSlime {
    mode: String,
    max_capacity: u64,
    domains: HashMap<String, RawDomainPolicy>,
}

#[derive(Deserialize, Clone)]
struct RawDomainPolicy {
    enabled: bool,
    max_magnitude_per_action: u64,
}

#[derive(Deserialize)]
struct RawDomains {
    meta: RawMeta,
    domains: HashMap<String, RawDomainEntry>,
}

#[derive(Deserialize)]
struct RawMeta {
    schema_version: String,
}

#[derive(Deserialize, Clone)]
struct RawDomainEntry {
    domain_id: String,
    #[serde(default)]
    max_payload_bytes: Option<usize>,
    #[serde(default)]
    validator: Option<String>,
    #[serde(default)]
    requires_intent: Option<bool>,
}

#[derive(Deserialize)]
struct RawIntents {
    meta: RawMeta,
    #[serde(default)]
    intents: HashMap<String, RawIntentEntry>,
}

#[derive(Deserialize, Clone)]
struct RawIntentEntry {
    binary: String,
    args_template: Vec<String>,
    #[serde(default)]
    validators: Vec<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct RawAllowlist {
    meta: RawMeta,
    #[serde(default)]
    urls: Vec<RawAllowlistUrl>,
}

#[derive(Deserialize, Clone)]
struct RawAllowlistUrl {
    pattern: String,
    methods: Vec<String>,
    #[serde(default)]
    max_body_bytes: Option<usize>,
}

// ── Public types ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DomainPolicy {
    pub enabled: bool,
    pub max_magnitude_per_action: u64,
}

#[derive(Debug, Clone)]
pub struct DomainMapping {
    pub domain_id: String,
    pub max_payload_bytes: Option<usize>,
    pub validator: Option<String>,
    pub requires_intent: bool,
}

#[derive(Debug, Clone)]
pub struct IntentMapping {
    pub binary: String,
    pub args_template: Vec<String>,
    pub validators: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AmaConfig {
    pub workspace_root: PathBuf,
    pub bind_host: String,
    pub bind_port: u16,
    pub log_level: String,
    pub log_output: String,
    pub slime_mode: String,
    pub max_capacity: u64,
    pub domain_policies: HashMap<String, DomainPolicy>,
    pub domain_mappings: HashMap<String, DomainMapping>,
    pub intents: HashMap<String, IntentMapping>,
    pub allowlist: Vec<AllowlistEntry>,
    pub boot_hashes: BootHashes,
}

impl AmaConfig {
    /// Load and validate all config files. Refuses to return on any error.
    pub fn load(config_dir: &Path) -> Result<Self, AmaError> {
        // ── Read raw files ───────────────────────────────────
        let config_bytes = Self::read_file(config_dir, "config.toml")?;
        let domains_bytes = Self::read_file(config_dir, "domains.toml")?;
        let intents_bytes = Self::read_file(config_dir, "intents.toml")?;
        let allowlist_bytes = Self::read_file(config_dir, "allowlist.toml")?;

        // ── Compute SHA-256 hashes ───────────────────────────
        let boot_hashes = BootHashes {
            config_hash: sha256_hex(&config_bytes),
            domains_hash: sha256_hex(&domains_bytes),
            intents_hash: sha256_hex(&intents_bytes),
            allowlist_hash: sha256_hex(&allowlist_bytes),
        };

        // ── Parse TOML ───────────────────────────────────────
        let raw_config: RawConfig = toml::from_str(
            std::str::from_utf8(&config_bytes).map_err(|e| Self::boot_err(format!("config.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("config.toml parse error: {e}")))?;

        let raw_domains: RawDomains = toml::from_str(
            std::str::from_utf8(&domains_bytes).map_err(|e| Self::boot_err(format!("domains.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("domains.toml parse error: {e}")))?;

        let raw_intents: RawIntents = toml::from_str(
            std::str::from_utf8(&intents_bytes).map_err(|e| Self::boot_err(format!("intents.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("intents.toml parse error: {e}")))?;

        let raw_allowlist: RawAllowlist = toml::from_str(
            std::str::from_utf8(&allowlist_bytes).map_err(|e| Self::boot_err(format!("allowlist.toml not UTF-8: {e}")))?
        ).map_err(|e| Self::boot_err(format!("allowlist.toml parse error: {e}")))?;

        // ── Validate schema versions ─────────────────────────
        Self::check_schema("ama-domains-v1", &raw_domains.meta.schema_version, "domains.toml")?;
        Self::check_schema("ama-intents-v1", &raw_intents.meta.schema_version, "intents.toml")?;
        Self::check_schema("ama-allowlist-v1", &raw_allowlist.meta.schema_version, "allowlist.toml")?;

        // ── Validate workspace_root ──────────────────────────
        let workspace_root = PathBuf::from(&raw_config.ama.workspace_root);
        if !workspace_root.is_absolute() {
            return Err(Self::boot_err("workspace_root must be absolute".into()));
        }
        if !workspace_root.is_dir() {
            return Err(Self::boot_err(format!(
                "workspace_root does not exist or is not a directory: {}",
                workspace_root.display()
            )));
        }

        // ── Validate bind_host ───────────────────────────────
        if raw_config.ama.bind_host != "127.0.0.1" {
            return Err(Self::boot_err(format!(
                "P0 requires bind_host = 127.0.0.1, got '{}'",
                raw_config.ama.bind_host
            )));
        }

        // ── Validate slime mode ──────────────────────────────
        if raw_config.slime.mode != "embedded" {
            return Err(Self::boot_err(format!(
                "P0 requires slime.mode = embedded, got '{}'",
                raw_config.slime.mode
            )));
        }
        if raw_config.slime.max_capacity == 0 {
            return Err(Self::boot_err("slime.max_capacity must be > 0".into()));
        }

        // ── Build domain policies (underscore -> dot normalization) ──
        let mut domain_policies = HashMap::new();
        for (key, raw_policy) in &raw_config.slime.domains {
            let domain_id = key.replace('_', ".");
            if raw_policy.max_magnitude_per_action == 0 {
                return Err(Self::boot_err(format!(
                    "domain '{}': max_magnitude_per_action must be > 0", domain_id
                )));
            }
            if raw_policy.max_magnitude_per_action > raw_config.slime.max_capacity {
                return Err(Self::boot_err(format!(
                    "domain '{}': max_magnitude_per_action ({}) > max_capacity ({})",
                    domain_id, raw_policy.max_magnitude_per_action, raw_config.slime.max_capacity
                )));
            }
            domain_policies.insert(domain_id, DomainPolicy {
                enabled: raw_policy.enabled,
                max_magnitude_per_action: raw_policy.max_magnitude_per_action,
            });
        }

        // ── Build domain mappings ────────────────────────────
        let mut domain_mappings = HashMap::new();
        for (action, entry) in &raw_domains.domains {
            // Cross-reference: domain_id must exist in config.toml policies
            if !domain_policies.contains_key(&entry.domain_id) {
                return Err(Self::boot_err(format!(
                    "domains.toml action '{}' references domain_id '{}' not in config.toml",
                    action, entry.domain_id
                )));
            }
            domain_mappings.insert(action.clone(), DomainMapping {
                domain_id: entry.domain_id.clone(),
                max_payload_bytes: entry.max_payload_bytes,
                validator: entry.validator.clone(),
                requires_intent: entry.requires_intent.unwrap_or(false),
            });
        }

        // ── Build intent mappings ────────────────────────────
        let mut intents = HashMap::new();
        for (name, raw_intent) in &raw_intents.intents {
            // On Linux, verify binary exists (skip on Windows for dev)
            #[cfg(unix)]
            {
                let bin_path = Path::new(&raw_intent.binary);
                if !bin_path.exists() {
                    return Err(Self::boot_err(format!(
                        "intent '{}': binary '{}' does not exist", name, raw_intent.binary
                    )));
                }
            }
            let working_dir = raw_intent.working_dir.as_ref().map(|wd| {
                wd.replace("{{workspace_root}}", workspace_root.to_str().unwrap_or(""))
            });
            intents.insert(name.clone(), IntentMapping {
                binary: raw_intent.binary.clone(),
                args_template: raw_intent.args_template.clone(),
                validators: raw_intent.validators.clone(),
                working_dir,
            });
        }

        // ── Build allowlist ──────────────────────────────────
        let allowlist: Vec<AllowlistEntry> = raw_allowlist.urls.iter().map(|u| {
            AllowlistEntry {
                pattern: u.pattern.clone(),
                methods: u.methods.clone(),
                max_body_bytes: u.max_body_bytes,
            }
        }).collect();

        Ok(Self {
            workspace_root,
            bind_host: raw_config.ama.bind_host,
            bind_port: raw_config.ama.bind_port,
            log_level: raw_config.ama.log_level,
            log_output: raw_config.ama.log_output,
            slime_mode: raw_config.slime.mode,
            max_capacity: raw_config.slime.max_capacity,
            domain_policies,
            domain_mappings,
            intents,
            allowlist,
            boot_hashes,
        })
    }

    fn read_file(dir: &Path, name: &str) -> Result<Vec<u8>, AmaError> {
        let path = dir.join(name);
        fs::read(&path).map_err(|e| Self::boot_err(format!(
            "cannot read {}: {}", path.display(), e
        )))
    }

    fn check_schema(expected: &str, got: &str, file: &str) -> Result<(), AmaError> {
        if got != expected {
            return Err(Self::boot_err(format!(
                "{}: unrecognized schema_version '{}' (expected '{}')",
                file, got, expected
            )));
        }
        Ok(())
    }

    fn boot_err(msg: String) -> AmaError {
        AmaError::ServiceUnavailable { message: msg }
    }
}
```

- [ ] **Step 4: Add to lib.rs, run tests**

```bash
cargo test --test test_config
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: config loading with boot validation and SHA-256 hashing (TDD)"
```

---

## Chunk 2: Authorization Engine

### Task 5: SLIME Embedded Authorizer (AB-S + CAS)

**Files:**
- Create: `src/slime.rs`
- Create: `tests/test_slime.rs`

- [ ] **Step 1: Write failing tests — including concurrent stress test**

```rust
// tests/test_slime.rs
use ama::slime::*;
use std::sync::Arc;
use std::thread;

#[test]
fn authorizes_valid_domain() {
    let auth = test_authorizer(10000);
    let verdict = auth.try_reserve(&"fs.write.workspace".into(), 10);
    assert!(matches!(verdict, SlimeVerdict::Authorized));
}

#[test]
fn rejects_unknown_domain() {
    let auth = test_authorizer(10000);
    let verdict = auth.try_reserve(&"unknown.domain".into(), 1);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

#[test]
fn rejects_disabled_domain() {
    let auth = test_authorizer_with_disabled(10000);
    let verdict = auth.try_reserve(&"fs.write.workspace".into(), 1);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

#[test]
fn rejects_over_per_action_limit() {
    let auth = test_authorizer(10000);
    // max_magnitude_per_action for fs.write.workspace is 100
    let verdict = auth.try_reserve(&"fs.write.workspace".into(), 101);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}

#[test]
fn capacity_exhaustion() {
    let auth = test_authorizer(100); // max_capacity = 100
    // Each call consumes 50
    assert!(matches!(auth.try_reserve(&"fs.read.workspace".into(), 50), SlimeVerdict::Authorized));
    assert!(matches!(auth.try_reserve(&"fs.read.workspace".into(), 50), SlimeVerdict::Authorized));
    // Now at 100 — next should be impossible
    assert!(matches!(auth.try_reserve(&"fs.read.workspace".into(), 1), SlimeVerdict::Impossible));
}

#[test]
fn capacity_never_exceeds_max_concurrent() {
    let auth = Arc::new(test_authorizer(1000));
    let mut handles = vec![];

    for _ in 0..100 {
        let auth = Arc::clone(&auth);
        handles.push(thread::spawn(move || {
            auth.try_reserve(&"fs.read.workspace".into(), 10)
        }));
    }

    let authorized_count: usize = handles.into_iter()
        .map(|h| h.join().unwrap())
        .filter(|v| matches!(v, SlimeVerdict::Authorized))
        .count();

    // 1000 capacity / 10 per call = exactly 100 can succeed
    assert_eq!(authorized_count, 100);
    // Capacity is exactly at max
    assert_eq!(auth.capacity_used(), 1000);
}

fn test_authorizer(max_cap: u64) -> P0Authorizer {
    // Build with standard test domains
    P0Authorizer::new(max_cap, vec![
        ("fs.write.workspace".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 100 }),
        ("fs.read.workspace".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 500 }),
        ("proc.exec.bounded".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 50 }),
        ("net.out.http".into(), DomainPolicy { enabled: true, max_magnitude_per_action: 200 }),
    ])
}

fn test_authorizer_with_disabled(max_cap: u64) -> P0Authorizer {
    P0Authorizer::new(max_cap, vec![
        ("fs.write.workspace".into(), DomainPolicy { enabled: false, max_magnitude_per_action: 100 }),
    ])
}
```

- [ ] **Step 2: Run tests, verify fail**

- [ ] **Step 3: Implement slime.rs**

```rust
// src/slime.rs
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Binary verdict — no middle ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlimeVerdict {
    Authorized,
    Impossible,
}

/// Domain policy loaded from config.
#[derive(Debug, Clone)]
pub struct DomainPolicy {
    pub enabled: bool,
    pub max_magnitude_per_action: u64,
}

pub type DomainId = String;

/// Trait for SLIME authorization (embedded or remote in P1).
pub trait SlimeAuthorizer: Send + Sync {
    fn try_reserve(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict;
    /// Check authorization without consuming capacity (for dry_run).
    fn check_only(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict;
    fn capacity_used(&self) -> u64;
    fn capacity_max(&self) -> u64;
    fn session_id(&self) -> &Uuid;
}

/// P0 embedded authorizer with atomic CAS capacity accounting.
pub struct P0Authorizer {
    capacity: AtomicU64,
    max_capacity: u64,
    domains: HashMap<DomainId, DomainPolicy>,
    session_id: Uuid,
}

impl P0Authorizer {
    pub fn new(max_capacity: u64, domains: Vec<(DomainId, DomainPolicy)>) -> Self {
        Self {
            capacity: AtomicU64::new(0),
            max_capacity,
            domains: domains.into_iter().collect(),
            session_id: Uuid::new_v4(),
        }
    }

    /// Internal policy check (shared between try_reserve and check_only).
    fn check_policy(&self, domain_id: &DomainId, magnitude: u64) -> Result<&DomainPolicy, SlimeVerdict> {
        // 1. Domain must exist (Closed World)
        let policy = match self.domains.get(domain_id) {
            Some(p) => p,
            None => return Err(SlimeVerdict::Impossible),
        };
        // 2. Domain must be enabled
        if !policy.enabled {
            return Err(SlimeVerdict::Impossible);
        }
        // 3. Per-action magnitude cap
        if magnitude > policy.max_magnitude_per_action {
            return Err(SlimeVerdict::Impossible);
        }
        Ok(policy)
    }
}

impl SlimeAuthorizer for P0Authorizer {
    fn try_reserve(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict {
        // Policy checks
        if let Err(v) = self.check_policy(domain_id, magnitude) {
            return v;
        }
        // 4. Atomic CAS reservation (race-safe, saturating)
        loop {
            let current = self.capacity.load(Ordering::Acquire);
            match current.checked_add(magnitude) {
                Some(new) if new <= self.max_capacity => {
                    match self.capacity.compare_exchange_weak(
                        current, new,
                        Ordering::AcqRel, Ordering::Acquire,
                    ) {
                        Ok(_) => return SlimeVerdict::Authorized,
                        Err(_) => continue, // Retry on concurrent modification
                    }
                }
                _ => return SlimeVerdict::Impossible,
            }
        }
    }

    fn check_only(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict {
        // Policy checks only — no capacity reservation
        if let Err(v) = self.check_policy(domain_id, magnitude) {
            return v;
        }
        // Check if capacity WOULD be available (no reservation)
        let current = self.capacity.load(Ordering::Acquire);
        match current.checked_add(magnitude) {
            Some(new) if new <= self.max_capacity => SlimeVerdict::Authorized,
            _ => SlimeVerdict::Impossible,
        }
    }

    fn capacity_used(&self) -> u64 {
        self.capacity.load(Ordering::Acquire)
    }

    fn capacity_max(&self) -> u64 {
        self.max_capacity
    }

    fn session_id(&self) -> &Uuid {
        &self.session_id
    }
}
```

Add `check_only` test to test_slime.rs after the existing tests:

```rust
#[test]
fn check_only_does_not_consume_capacity() {
    let auth = test_authorizer(100);
    let verdict = auth.check_only(&"fs.write.workspace".into(), 50);
    assert!(matches!(verdict, SlimeVerdict::Authorized));
    // Capacity should still be 0 — check_only doesn't reserve
    assert_eq!(auth.capacity_used(), 0);
}

#[test]
fn check_only_reports_impossible_when_full() {
    let auth = test_authorizer(10);
    // Consume all capacity
    auth.try_reserve(&"fs.read.workspace".into(), 10);
    // check_only should report impossible
    let verdict = auth.check_only(&"fs.read.workspace".into(), 1);
    assert!(matches!(verdict, SlimeVerdict::Impossible));
}
```

- [ ] **Step 4: Run tests**

Expected: ALL PASS (including concurrent stress test with exactly 100 authorized out of 100 threads).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: SLIME embedded authorizer with atomic CAS capacity (TDD)"
```

---

### Task 6: Domain Mapper

**Files:**
- Create: `src/mapper.rs`
- Create: `tests/test_mapper.rs` (inline or separate)

- [ ] **Step 1: Write failing tests**

Test that `file_write` maps to `fs.write.workspace`, unknown action returns error, magnitude is passed through.

- [ ] **Step 2: Implement mapper.rs**

```rust
// src/mapper.rs
use crate::config::AmaConfig;
use crate::errors::AmaError;

pub struct DomainMapping {
    pub domain_id: String,
    pub magnitude: u64,
}

pub fn map_action(action: &str, magnitude: u64, config: &AmaConfig) -> Result<DomainMapping, AmaError> {
    let domain_entry = config.domain_mappings.get(action)
        .ok_or_else(|| AmaError::Validation {
            error_class: "unknown_action".into(),
            message: format!("action '{}' not in domains.toml", action),
        })?;

    Ok(DomainMapping {
        domain_id: domain_entry.domain_id.clone(),
        magnitude,
    })
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat: domain mapper action -> domain_id (TDD)"
```

---

### Task 7: File Actuators (Read + Write)

**Files:**
- Create: `src/actuator/mod.rs`
- Create: `src/actuator/file.rs`
- Create: `tests/test_actuator_file.rs`

- [ ] **Step 1: Write failing tests using tempdir**

```rust
// tests/test_actuator_file.rs
use ama::actuator::file::*;
use ama::newtypes::*;
use tempfile::TempDir;
use std::fs;

#[test]
fn writes_file_atomically() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    let path = WorkspacePath::new("test.txt", workspace).unwrap();
    let content = BoundedBytes::new("hello world".into(), 1_048_576).unwrap();
    let action_id = "test-action-1";

    let result = file_write(&path, &content, action_id).unwrap();
    assert_eq!(result.bytes_written, 11);
    assert_eq!(fs::read_to_string(workspace.join("test.txt")).unwrap(), "hello world");
}

#[test]
fn write_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let path = WorkspacePath::new("a/b/c/file.txt", dir.path()).unwrap();
    let content = BoundedBytes::new("nested".into(), 1_048_576).unwrap();

    let result = file_write(&path, &content, "test-2");
    assert!(result.is_ok());
}

#[test]
fn reads_existing_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("hello.txt"), "content here").unwrap();
    let path = WorkspacePath::new("hello.txt", dir.path()).unwrap();

    let result = file_read(&path, 524_288).unwrap();
    assert_eq!(result.content, "content here");
    assert!(!result.truncated);
}

#[test]
fn read_truncates_large_file() {
    let dir = TempDir::new().unwrap();
    let big = "x".repeat(1000);
    fs::write(dir.path().join("big.txt"), &big).unwrap();
    let path = WorkspacePath::new("big.txt", dir.path()).unwrap();

    let result = file_read(&path, 100).unwrap(); // limit 100 bytes
    assert_eq!(result.bytes_returned, 100);
    assert!(result.truncated);
}

#[test]
fn read_nonexistent_file_fails() {
    let dir = TempDir::new().unwrap();
    let path = WorkspacePath::new("nope.txt", dir.path()).unwrap();
    assert!(file_read(&path, 524_288).is_err());
}

#[cfg(unix)]
#[test]
fn write_rejects_symlink_in_path() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, dir.path().join("link")).unwrap();

    let path = WorkspacePath::new("link/file.txt", dir.path()).unwrap();
    let content = BoundedBytes::new("bad".into(), 1_048_576).unwrap();
    // Should fail because "link" is a symlink
    let result = file_write(&path, &content, "test-sym");
    assert!(result.is_err());
}
```

- [ ] **Step 1b: Add non-UTF-8 rejection test**

```rust
#[test]
fn read_rejects_non_utf8() {
    let dir = TempDir::new().unwrap();
    // Write invalid UTF-8 bytes
    fs::write(dir.path().join("binary.dat"), &[0xFF, 0xFE, 0x00, 0x01]).unwrap();
    let path = WorkspacePath::new("binary.dat", dir.path()).unwrap();
    let result = file_read(&path, 524_288);
    assert!(result.is_err()); // P0 is text-only
}
```

- [ ] **Step 2: Implement actuator/file.rs**

```rust
// src/actuator/file.rs
use crate::errors::AmaError;
use crate::newtypes::{BoundedBytes, WorkspacePath};
use std::fs;
use std::io::Read;
use std::path::Path;

/// Result of a file write operation.
#[derive(Debug)]
pub struct FileWriteResult {
    pub bytes_written: u64,
}

/// Result of a file read operation.
#[derive(Debug)]
pub struct FileReadResult {
    pub content: String,
    pub bytes_returned: u64,
    pub total_bytes: u64,
    pub truncated: bool,
}

/// Atomic file write: write to .ama.<action_id>.tmp then rename.
pub fn file_write(
    path: &WorkspacePath,
    content: &BoundedBytes,
    action_id: &str,
) -> Result<FileWriteResult, AmaError> {
    let target = path.canonical();

    // Verify every path component is not a symlink (Unix)
    verify_no_symlinks(target)?;

    // Verify target is regular file or doesn't exist
    if target.exists() {
        let meta = target.symlink_metadata().map_err(|e| AmaError::ServiceUnavailable {
            message: format!("cannot stat target: {}", e),
        })?;
        if !meta.is_file() {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: "target is not a regular file".into(),
            });
        }
    }

    // Create parent directories if needed
    if let Some(parent) = target.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| AmaError::ServiceUnavailable {
                message: format!("cannot create directories: {}", e),
            })?;
            // Set permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o755))
                    .map_err(|e| AmaError::ServiceUnavailable {
                        message: format!("cannot set dir permissions: {}", e),
                    })?;
            }
        }
    }

    // Write to temp file
    let tmp_name = format!(
        "{}.ama.{}.tmp",
        target.file_name().unwrap_or_default().to_string_lossy(),
        action_id
    );
    let tmp_path = target.with_file_name(&tmp_name);

    let write_result = fs::write(&tmp_path, content.as_str());
    if let Err(e) = write_result {
        // Cleanup temp on failure
        let _ = fs::remove_file(&tmp_path);
        return Err(AmaError::ServiceUnavailable {
            message: format!("write failed: {}", e),
        });
    }

    // Set file permissions (0644) on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o644));
    }

    // Atomic rename
    if let Err(e) = fs::rename(&tmp_path, target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(AmaError::ServiceUnavailable {
            message: format!("atomic rename failed: {}", e),
        });
    }

    Ok(FileWriteResult {
        bytes_written: content.len() as u64,
    })
}

/// Bounded file read with truncation.
pub fn file_read(
    path: &WorkspacePath,
    max_bytes: usize,
) -> Result<FileReadResult, AmaError> {
    let target = path.canonical();

    // Verify no symlinks in path
    verify_no_symlinks(target)?;

    // File must exist
    if !target.exists() {
        return Err(AmaError::ServiceUnavailable {
            message: "file does not exist".into(),
        });
    }

    // Must be regular file
    let meta = target.symlink_metadata().map_err(|e| AmaError::ServiceUnavailable {
        message: format!("cannot stat: {}", e),
    })?;
    if !meta.is_file() {
        return Err(AmaError::Validation {
            error_class: "invalid_target".into(),
            message: "not a regular file".into(),
        });
    }

    let total_bytes = meta.len();

    // Bounded read
    let mut file = fs::File::open(target).map_err(|e| AmaError::ServiceUnavailable {
        message: format!("cannot open: {}", e),
    })?;
    let read_size = std::cmp::min(total_bytes as usize, max_bytes);
    let mut buf = vec![0u8; read_size];
    file.read_exact(&mut buf).map_err(|e| AmaError::ServiceUnavailable {
        message: format!("read error: {}", e),
    })?;

    // UTF-8 check (P0 is text-only)
    let content = String::from_utf8(buf).map_err(|_| AmaError::Validation {
        error_class: "encoding_error".into(),
        message: "file content is not valid UTF-8 (P0 is text-only)".into(),
    })?;

    let truncated = total_bytes as usize > max_bytes;

    Ok(FileReadResult {
        bytes_returned: content.len() as u64,
        total_bytes,
        content,
        truncated,
    })
}

/// Verify no path component is a symlink. On non-Unix, this is a no-op.
fn verify_no_symlinks(path: &Path) -> Result<(), AmaError> {
    #[cfg(unix)]
    {
        // Check every ancestor starting from the root
        let mut check = std::path::PathBuf::new();
        for component in path.components() {
            check.push(component);
            if check.exists() {
                let meta = check.symlink_metadata().map_err(|e| AmaError::ServiceUnavailable {
                    message: format!("lstat failed on {}: {}", check.display(), e),
                })?;
                if meta.file_type().is_symlink() {
                    return Err(AmaError::Validation {
                        error_class: "invalid_target".into(),
                        message: format!("symlink in path: {}", check.display()),
                    });
                }
            }
        }
    }
    let _ = path; // suppress unused warning on Windows
    Ok(())
}

/// Clean up orphan .ama.*.tmp files in a directory (called at startup).
pub fn cleanup_orphan_temps(workspace_root: &Path) -> usize {
    let mut cleaned = 0;
    if let Ok(entries) = walkdir(workspace_root) {
        for path in entries {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.contains(".ama.") && name.ends_with(".tmp") {
                    if fs::remove_file(&path).is_ok() {
                        cleaned += 1;
                    }
                }
            }
        }
    }
    cleaned
}

/// Simple recursive directory walker for cleanup.
fn walkdir(dir: &Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut results = vec![];
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(&path)?);
            } else {
                results.push(path);
            }
        }
    }
    Ok(results)
}
```

- [ ] **Step 3: Implement actuator/mod.rs** (dispatcher stub)

```rust
pub mod file;
#[cfg(unix)]
pub mod shell;
pub mod http;
```

- [ ] **Step 4: Run tests, commit**

```bash
git add -A && git commit -m "feat: file actuators with atomic writes and symlink protection (TDD)"
```

---

### Task 8: Shell Exec Actuator (Linux-only)

**Files:**
- Create: `src/actuator/shell.rs`
- Create: `tests/test_actuator_shell.rs`

- [ ] **Step 1: Write failing tests (cfg(unix) gated)**

```rust
#[cfg(unix)]
mod tests {
    use ama::actuator::shell::*;

    #[tokio::test]
    async fn executes_simple_intent() {
        let result = shell_exec(
            "/bin/echo",
            &["hello", "world"],
            "/tmp",
            "test-id",
            std::time::Duration::from_secs(5),
            65_536,
        ).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn kills_on_timeout() {
        let result = shell_exec(
            "/bin/sleep",
            &["60"],
            "/tmp",
            "test-timeout",
            std::time::Duration::from_secs(1),
            65_536,
        ).await;
        // Should timeout and kill
        assert!(result.is_ok()); // returns result with non-zero exit
    }

    #[tokio::test]
    async fn captures_stderr() {
        let result = shell_exec(
            "/bin/ls",
            &["/nonexistent_path_xyz"],
            "/tmp",
            "test-stderr",
            std::time::Duration::from_secs(5),
            65_536,
        ).await.unwrap();
        assert_ne!(result.exit_code, 0);
        assert!(!result.stderr.is_empty());
    }
}
```

- [ ] **Step 2: Implement shell.rs**

```rust
// src/actuator/shell.rs — Linux only
#![cfg(unix)]

use crate::errors::AmaError;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Result of a shell exec operation.
#[derive(Debug)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub truncated: bool,
}

/// Execute a binary with arguments in a new process group.
///
/// - Uses execv (via Command::new), never sh -c
/// - Fresh minimal environment
/// - Process group isolation (setpgid)
/// - Kill sequence: SIGTERM -> 2s -> SIGKILL to PGID
/// - Bounded output capture
pub async fn shell_exec(
    binary: &str,
    args: &[&str],
    working_dir: &str,
    action_id: &str,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<ShellExecResult, AmaError> {
    use std::os::unix::process::CommandExt;

    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Fresh minimal environment — no inherited variables
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", working_dir)
        .env("LANG", "en_US.UTF-8")
        .env("SAFA_ACTION_ID", action_id);

    // SAFETY: pre_exec runs after fork, before exec in child process.
    // setpgid(0,0) puts child in its own process group for kill containment.
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| AmaError::ServiceUnavailable {
        message: format!("failed to spawn process: {}", e),
    })?;

    let pid = child.id().unwrap_or(0) as i32;

    // Take ownership of stdout/stderr handles
    let mut stdout_handle = child.stdout.take().unwrap();
    let mut stderr_handle = child.stderr.take().unwrap();

    // Bounded output capture with timeout
    let result = tokio::time::timeout(timeout, async {
        let mut stdout_buf = vec![0u8; max_output_bytes];
        let mut stderr_buf = vec![0u8; max_output_bytes];

        let stdout_read = stdout_handle.read(&mut stdout_buf);
        let stderr_read = stderr_handle.read(&mut stderr_buf);

        let (stdout_n, stderr_n) = tokio::join!(stdout_read, stderr_read);
        let stdout_n = stdout_n.unwrap_or(0);
        let stderr_n = stderr_n.unwrap_or(0);

        let status = child.wait().await;

        (stdout_buf, stdout_n, stderr_buf, stderr_n, status)
    }).await;

    match result {
        Ok((stdout_buf, stdout_n, stderr_buf, stderr_n, status)) => {
            let stdout_truncated = stdout_n >= max_output_bytes;
            let stderr_truncated = stderr_n >= max_output_bytes;

            // UTF-8 validation (P0 is text-only)
            let stdout = String::from_utf8(stdout_buf[..stdout_n].to_vec())
                .map_err(|_| AmaError::ServiceUnavailable {
                    message: "stdout contains non-UTF-8 data".into(),
                })?;
            let stderr = String::from_utf8(stderr_buf[..stderr_n].to_vec())
                .map_err(|_| AmaError::ServiceUnavailable {
                    message: "stderr contains non-UTF-8 data".into(),
                })?;

            let exit_code = status
                .map_err(|e| AmaError::ServiceUnavailable {
                    message: format!("wait failed: {}", e),
                })?
                .code()
                .unwrap_or(-1);

            Ok(ShellExecResult {
                stdout,
                stderr,
                exit_code,
                truncated: stdout_truncated || stderr_truncated,
            })
        }
        Err(_) => {
            // Timeout — execute kill sequence
            // SIGTERM to entire process group
            if pid > 0 {
                unsafe { libc::kill(-pid, libc::SIGTERM); }
            }
            // Wait 2 seconds then SIGKILL
            tokio::time::sleep(Duration::from_secs(2)).await;
            if pid > 0 {
                unsafe { libc::kill(-pid, libc::SIGKILL); }
            }
            // Reap the child
            let _ = child.wait().await;

            Ok(ShellExecResult {
                stdout: String::new(),
                stderr: "process killed: timeout exceeded".into(),
                exit_code: -1,
                truncated: false,
            })
        }
    }
}
```

**Note:** Add `libc = "0.2"` to `[dependencies]` in `Cargo.toml` (Task 1 Step 2).

- [ ] **Step 3: Run tests (Linux/WSL only), commit**

```bash
cargo test --test test_actuator_shell
git add -A && git commit -m "feat: shell exec actuator with process isolation and kill sequence (TDD)"
```

---

## Chunk 3: Server & Integration

### Task 9: HTTP Actuator

**Files:**
- Create: `src/actuator/http.rs`
- Create: `tests/test_actuator_http.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/test_actuator_http.rs
use ama::actuator::http::*;

#[tokio::test]
async fn rejects_loopback_ip() {
    assert!(is_private_ip("127.0.0.1".parse().unwrap()));
    assert!(is_private_ip("::1".parse().unwrap()));
}

#[tokio::test]
async fn rejects_rfc1918() {
    assert!(is_private_ip("10.0.0.1".parse().unwrap()));
    assert!(is_private_ip("192.168.1.1".parse().unwrap()));
    assert!(is_private_ip("172.16.0.1".parse().unwrap()));
}

#[tokio::test]
async fn rejects_link_local() {
    assert!(is_private_ip("169.254.1.1".parse().unwrap()));
}

#[tokio::test]
async fn accepts_public_ip() {
    assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
    assert!(!is_private_ip("1.1.1.1".parse().unwrap()));
}

#[tokio::test]
async fn rejects_metadata_endpoint() {
    assert!(is_private_ip("169.254.169.254".parse().unwrap()));
}
```

- [ ] **Step 2: Run tests, verify fail**

```bash
cargo test --test test_actuator_http
```

- [ ] **Step 3: Implement http.rs**

```rust
// src/actuator/http.rs
use crate::errors::AmaError;
use crate::newtypes::{AllowlistedUrl, AllowlistEntry, BoundedBytes, HttpMethod};
use reqwest::redirect::Policy;
use std::net::IpAddr;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 262_144; // 256 KiB
const MAX_REDIRECTS: usize = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = "SAFA/0.1.0";

/// Result of an HTTP request.
#[derive(Debug)]
pub struct HttpResult {
    pub status_code: u16,
    pub body: String,
    pub truncated: bool,
}

/// Check if an IP address is private/loopback/link-local/metadata.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()           // 127.0.0.0/8
            || v4.is_private()         // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            || v4.is_link_local()      // 169.254.0.0/16 (includes metadata 169.254.169.254)
            || v4.is_broadcast()
            || v4.is_unspecified()
            || v4.octets()[0] == 0     // 0.0.0.0/8
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
            || v6.is_unspecified()     // ::
            // IPv4-mapped IPv6 addresses
            || v6.to_ipv4_mapped().map_or(false, |v4| {
                v4.is_loopback() || v4.is_private() || v4.is_link_local()
            })
        }
    }
}

/// Resolve hostname and validate all IPs are safe (not private/loopback).
async fn validate_dns(host: &str) -> Result<(), AmaError> {
    use tokio::net::lookup_host;

    let addrs: Vec<_> = lookup_host(format!("{}:443", host))
        .await
        .map_err(|e| AmaError::ServiceUnavailable {
            message: format!("DNS resolution failed: {}", e),
        })?
        .collect();

    if addrs.is_empty() {
        return Err(AmaError::ServiceUnavailable {
            message: "DNS resolved to no addresses".into(),
        });
    }

    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: format!("URL resolves to private/loopback IP: {}", addr.ip()),
            });
        }
    }

    Ok(())
}

/// Execute an HTTP request with full safety checks.
pub async fn http_request(
    method: HttpMethod,
    url: &AllowlistedUrl,
    body: Option<&BoundedBytes>,
    allowlist: &[AllowlistEntry],
) -> Result<HttpResult, AmaError> {
    let url_str = url.as_str();

    // Extract host for DNS validation
    let parsed = reqwest::Url::parse(url_str).map_err(|e| AmaError::Validation {
        error_class: "invalid_target".into(),
        message: format!("invalid URL: {}", e),
    })?;
    let host = parsed.host_str().ok_or_else(|| AmaError::Validation {
        error_class: "invalid_target".into(),
        message: "URL has no host".into(),
    })?;

    // DNS/IP safety check — resolve and validate before connecting
    validate_dns(host).await?;

    // Build reqwest client with safety constraints
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .redirect(Policy::limited(MAX_REDIRECTS))
        .cookie_store(false)           // No cookies
        .https_only(true)              // HTTPS only
        .danger_accept_invalid_certs(false) // TLS validation ON
        .build()
        .map_err(|e| AmaError::ServiceUnavailable {
            message: format!("HTTP client build failed: {}", e),
        })?;

    // Build request
    let request = match method {
        HttpMethod::Get => client.get(url_str),
        HttpMethod::Post => {
            let mut req = client.post(url_str);
            if let Some(body_data) = body {
                req = req.body(body_data.as_str().to_string())
                    .header("Content-Type", "application/json");
            }
            req
        }
    };

    // Execute
    let response = request.send().await.map_err(|e| {
        if e.is_redirect() {
            AmaError::Validation {
                error_class: "redirect_error".into(),
                message: "redirect limit exceeded or POST redirect rejected".into(),
            }
        } else if e.is_timeout() {
            AmaError::ServiceUnavailable {
                message: "HTTP request timed out".into(),
            }
        } else {
            AmaError::ServiceUnavailable {
                message: format!("HTTP request failed: {}", e),
            }
        }
    })?;

    // Re-validate the actual remote IP after connection (DNS rebinding protection)
    if let Some(remote_addr) = response.remote_addr() {
        if is_private_ip(remote_addr.ip()) {
            return Err(AmaError::Validation {
                error_class: "invalid_target".into(),
                message: format!("response came from private IP: {}", remote_addr.ip()),
            });
        }
    }

    // Validate final URL against allowlist (after redirects)
    let final_url = response.url().as_str();
    if final_url != url_str {
        // Re-check allowlist for redirect target
        let _ = AllowlistedUrl::new(final_url, allowlist).map_err(|_| AmaError::Validation {
            error_class: "redirect_error".into(),
            message: "redirect target not in allowlist".into(),
        })?;
    }

    let status_code = response.status().as_u16();

    // Bounded body read (256 KiB max)
    let body_bytes = response.bytes().await.map_err(|e| AmaError::ServiceUnavailable {
        message: format!("failed to read response body: {}", e),
    })?;

    let truncated = body_bytes.len() > MAX_RESPONSE_BYTES;
    let read_bytes = if truncated {
        &body_bytes[..MAX_RESPONSE_BYTES]
    } else {
        &body_bytes[..]
    };

    // UTF-8 check (P0 text-only)
    let body_text = String::from_utf8(read_bytes.to_vec()).map_err(|_| AmaError::ServiceUnavailable {
        message: "response body is not valid UTF-8 (P0 is text-only)".into(),
    })?;

    Ok(HttpResult {
        status_code,
        body: body_text,
        truncated,
    })
}
```

- [ ] **Step 4: Run tests, commit**

```bash
cargo test --test test_actuator_http
git add -A && git commit -m "feat: HTTP actuator with DNS/IP safety and redirect validation (TDD)"
```

---

### Task 10: Idempotency Cache

**Files:**
- Create: `src/idempotency.rs`
- Create: `tests/test_idempotency.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/test_idempotency.rs
use ama::idempotency::*;
use uuid::Uuid;
use std::time::Duration;

#[test]
fn validates_uuid_v4_format() {
    assert!(validate_idempotency_key("550e8400-e29b-41d4-a716-446655440000").is_ok());
    assert!(validate_idempotency_key("not-a-uuid").is_err());
    assert!(validate_idempotency_key("").is_err());
}

#[test]
fn rejects_key_over_128_bytes() {
    let long = "a".repeat(129);
    assert!(validate_idempotency_key(&long).is_err());
}

#[test]
fn insert_and_lookup() {
    let cache = IdempotencyCache::new(10_000, Duration::from_secs(300));
    let key = Uuid::new_v4();

    // First insert: should mark as in-flight
    let status = cache.check_or_insert(key);
    assert!(matches!(status, IdempotencyStatus::New));

    // Second check: should detect in-flight
    let status = cache.check_or_insert(key);
    assert!(matches!(status, IdempotencyStatus::InFlight));
}

#[test]
fn returns_cached_result() {
    let cache = IdempotencyCache::new(10_000, Duration::from_secs(300));
    let key = Uuid::new_v4();
    let cached_body = r#"{"status":"authorized"}"#.to_string();

    cache.check_or_insert(key);
    cache.complete(key, cached_body.clone());

    let status = cache.check_or_insert(key);
    match status {
        IdempotencyStatus::Cached(body) => assert_eq!(body, cached_body),
        _ => panic!("expected Cached"),
    }
}

#[test]
fn cache_full_returns_service_unavailable() {
    let cache = IdempotencyCache::new(3, Duration::from_secs(300));

    // Fill the cache
    for _ in 0..3 {
        let key = Uuid::new_v4();
        cache.check_or_insert(key);
        cache.complete(key, "done".into());
    }

    // Next insert should return Full (503)
    let key = Uuid::new_v4();
    let status = cache.check_or_insert(key);
    assert!(matches!(status, IdempotencyStatus::Full));
}

#[test]
fn expired_entries_are_purged() {
    let cache = IdempotencyCache::new(3, Duration::from_millis(10));
    let key = Uuid::new_v4();
    cache.check_or_insert(key);
    cache.complete(key, "done".into());

    // Wait for expiry
    std::thread::sleep(Duration::from_millis(20));

    // Insert should succeed after purge
    let key2 = Uuid::new_v4();
    let status = cache.check_or_insert(key2);
    assert!(matches!(status, IdempotencyStatus::New));
}
```

- [ ] **Step 2: Run tests, verify fail**

```bash
cargo test --test test_idempotency
```

- [ ] **Step 3: Implement idempotency.rs**

```rust
// src/idempotency.rs
use crate::errors::AmaError;
use dashmap::DashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

const UUID_V4_REGEX: &str = r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$";
const MAX_KEY_BYTES: usize = 128;

/// Validate Idempotency-Key header format.
pub fn validate_idempotency_key(key: &str) -> Result<Uuid, AmaError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(AmaError::BadRequest {
            message: "Idempotency-Key must be 1-128 bytes".into(),
        });
    }
    // Parse as UUID v4
    let uuid = Uuid::parse_str(key).map_err(|_| AmaError::BadRequest {
        message: "Idempotency-Key must be a valid UUID v4".into(),
    })?;
    // Verify it's actually version 4
    if uuid.get_version_num() != 4 {
        return Err(AmaError::BadRequest {
            message: "Idempotency-Key must be UUID v4".into(),
        });
    }
    Ok(uuid)
}

/// Status returned when checking the idempotency cache.
#[derive(Debug)]
pub enum IdempotencyStatus {
    /// Key not seen before — proceed with processing.
    New,
    /// Key is currently being processed by another request.
    InFlight,
    /// Key was processed before — return cached result.
    Cached(String),
    /// Cache is full and all entries within TTL — 503.
    Full,
}

#[derive(Debug)]
struct CacheEntry {
    result: Option<String>,
    created_at: Instant,
    in_flight: bool,
}

/// Idempotency cache with TTL and fail-closed overflow.
pub struct IdempotencyCache {
    entries: DashMap<Uuid, CacheEntry>,
    max_entries: usize,
    ttl: Duration,
}

impl IdempotencyCache {
    pub fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            max_entries,
            ttl,
        }
    }

    /// Check if key exists. If new, insert as in-flight.
    pub fn check_or_insert(&self, key: Uuid) -> IdempotencyStatus {
        // Check existing entry first
        if let Some(entry) = self.entries.get(&key) {
            if entry.created_at.elapsed() > self.ttl {
                // Expired — remove and treat as new
                drop(entry);
                self.entries.remove(&key);
            } else if entry.in_flight {
                return IdempotencyStatus::InFlight;
            } else if let Some(ref result) = entry.result {
                return IdempotencyStatus::Cached(result.clone());
            }
        }

        // Purge expired entries before checking capacity
        self.purge_expired();

        // Check capacity (fail-closed: 503 if full and all within TTL)
        if self.entries.len() >= self.max_entries {
            return IdempotencyStatus::Full;
        }

        // Insert as in-flight
        self.entries.insert(key, CacheEntry {
            result: None,
            created_at: Instant::now(),
            in_flight: true,
        });

        IdempotencyStatus::New
    }

    /// Mark a key as completed with its cached result.
    pub fn complete(&self, key: Uuid, result: String) {
        if let Some(mut entry) = self.entries.get_mut(&key) {
            entry.in_flight = false;
            entry.result = Some(result);
        }
    }

    /// Remove a key (e.g., on processing failure — allow retry).
    pub fn remove(&self, key: &Uuid) {
        self.entries.remove(key);
    }

    /// Purge entries beyond TTL.
    fn purge_expired(&self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| {
            now.duration_since(entry.created_at) < self.ttl
        });
    }

    /// Current cache size (for /ama/status).
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
```

- [ ] **Step 4: Run tests, commit**

```bash
cargo test --test test_idempotency
git add -A && git commit -m "feat: idempotency cache with TTL and fail-closed overflow (TDD)"
```

---

### Task 11: Audit Logging

**Files:**
- Create: `src/audit.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/test_audit.rs
use ama::audit::*;

#[test]
fn request_hash_is_deterministic() {
    let hash1 = compute_request_hash("file_write", "test.txt", 1);
    let hash2 = compute_request_hash("file_write", "test.txt", 1);
    assert_eq!(hash1, hash2);
}

#[test]
fn request_hash_changes_with_input() {
    let hash1 = compute_request_hash("file_write", "test.txt", 1);
    let hash2 = compute_request_hash("file_write", "other.txt", 1);
    assert_ne!(hash1, hash2);
}

#[test]
fn request_hash_is_hex_sha256() {
    let hash = compute_request_hash("file_write", "test.txt", 1);
    assert_eq!(hash.len(), 64); // SHA-256 = 64 hex chars
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn audit_entry_formats_correctly() {
    let entry = AuditEntry {
        timestamp: "2026-03-13T12:00:00Z".into(),
        session_id: "test-session".into(),
        action_id: "test-action".into(),
        adapter: "generic".into(),
        action: "file_write".into(),
        domain_id: "fs.write.workspace".into(),
        magnitude_effective: 1,
        duration_ms: 42,
        status: "authorized".into(),
        request_hash: "abc123".into(),
        truncated: false,
    };
    // Verify all fields are set (no panics)
    assert_eq!(entry.action, "file_write");
    assert_eq!(entry.status, "authorized");
}
```

- [ ] **Step 2: Run tests, verify fail**

```bash
cargo test --test test_audit
```

- [ ] **Step 3: Implement audit.rs**

```rust
// src/audit.rs
use sha2::{Sha256, Digest};

/// Audit log entry — metadata only, never contains payload content.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: String,
    pub session_id: String,
    pub action_id: String,
    pub adapter: String,
    pub action: String,
    pub domain_id: String,
    pub magnitude_effective: u64,
    pub duration_ms: u64,
    pub status: String,      // "authorized" | "impossible" | "error"
    pub request_hash: String, // SHA-256 of canonical action
    pub truncated: bool,
}

/// Compute SHA-256 hash of the canonical action representation.
/// Hashes over (action, target, magnitude) — NOT raw JSON.
pub fn compute_request_hash(action: &str, target: &str, magnitude: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(action.as_bytes());
    hasher.update(b"|");
    hasher.update(target.as_bytes());
    hasher.update(b"|");
    hasher.update(magnitude.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// Emit audit log entry via tracing (structured JSON).
pub fn log_audit(entry: &AuditEntry) {
    tracing::info!(
        timestamp = %entry.timestamp,
        session_id = %entry.session_id,
        action_id = %entry.action_id,
        adapter = %entry.adapter,
        action = %entry.action,
        domain_id = %entry.domain_id,
        magnitude = entry.magnitude_effective,
        duration_ms = entry.duration_ms,
        status = %entry.status,
        request_hash = %entry.request_hash,
        truncated = entry.truncated,
        "SAFA_AUDIT"
    );
}
```

- [ ] **Step 4: Run tests, commit**

```bash
cargo test --test test_audit
git add -A && git commit -m "feat: structured audit logging with SHA-256 request hashing (TDD)"
```

---

### Task 12: Pipeline (Full Request Flow)

**Files:**
- Create: `src/pipeline.rs`
- Create: `tests/test_pipeline.rs`

- [ ] **Step 1: Write failing tests**

Test the full flow: valid file_write -> 200, unknown action -> 422, impossible (capacity) -> 403, dry_run -> 200 with no actuation and no capacity consumed.

- [ ] **Step 2: Implement pipeline.rs**

```rust
// src/pipeline.rs
use crate::audit::{compute_request_hash, log_audit, AuditEntry};
use crate::canonical::{ActionResult, CanonicalAction};
use crate::config::AmaConfig;
use crate::errors::AmaError;
use crate::mapper::map_action;
use crate::newtypes::*;
use crate::schema::{ActionRequest, ActionResponse, validate_magnitude};
use crate::slime::{P0Authorizer, SlimeAuthorizer, SlimeVerdict};
use std::time::{Duration, Instant};

/// Per-action timeout durations from spec.
fn action_timeout(action: &str) -> Duration {
    match action {
        "file_write" | "file_read" => Duration::from_secs(5),
        "shell_exec" | "http_request" => Duration::from_secs(15),
        _ => Duration::from_secs(5),
    }
}

/// Validate mutual exclusivity of payload/args per action class (M4).
pub fn validate_field_exclusivity(request: &ActionRequest) -> Result<(), AmaError> {
    match request.action.as_str() {
        "file_write" => {
            if request.payload.is_none() {
                return Err(AmaError::Validation {
                    error_class: "missing_field".into(),
                    message: "file_write requires 'payload'".into(),
                });
            }
            if request.args.is_some() {
                return Err(AmaError::Validation {
                    error_class: "invalid_field".into(),
                    message: "file_write does not accept 'args'".into(),
                });
            }
        }
        "file_read" => {
            if request.payload.is_some() {
                return Err(AmaError::Validation {
                    error_class: "invalid_field".into(),
                    message: "file_read does not accept 'payload'".into(),
                });
            }
            if request.args.is_some() {
                return Err(AmaError::Validation {
                    error_class: "invalid_field".into(),
                    message: "file_read does not accept 'args'".into(),
                });
            }
        }
        "shell_exec" => {
            if request.args.is_none() {
                return Err(AmaError::Validation {
                    error_class: "missing_field".into(),
                    message: "shell_exec requires 'args'".into(),
                });
            }
            if request.payload.is_some() {
                return Err(AmaError::Validation {
                    error_class: "invalid_field".into(),
                    message: "shell_exec does not accept 'payload'".into(),
                });
            }
        }
        "http_request" => {
            // payload optional (POST body), args forbidden
            if request.args.is_some() {
                return Err(AmaError::Validation {
                    error_class: "invalid_field".into(),
                    message: "http_request does not accept 'args'".into(),
                });
            }
            // method is required
            if request.method.is_none() {
                return Err(AmaError::Validation {
                    error_class: "missing_field".into(),
                    message: "http_request requires 'method'".into(),
                });
            }
        }
        _ => {} // Unknown action handled by mapper
    }
    Ok(())
}

/// Canonicalize: construct type-safe newtypes from raw request (C3).
fn canonicalize(request: &ActionRequest, config: &AmaConfig) -> Result<CanonicalAction, AmaError> {
    match request.action.as_str() {
        "file_write" => {
            let path = WorkspacePath::new(&request.target, &config.workspace_root)?;
            let max_payload = config.domain_mappings
                .get("file_write")
                .and_then(|m| m.max_payload_bytes)
                .unwrap_or(1_048_576);
            let content = BoundedBytes::new(
                request.payload.clone().unwrap_or_default(),
                max_payload,
            )?;
            Ok(CanonicalAction::FileWrite { path, content })
        }
        "file_read" => {
            let path = WorkspacePath::new(&request.target, &config.workspace_root)?;
            Ok(CanonicalAction::FileRead { path })
        }
        "shell_exec" => {
            let intent = IntentId::new(&request.target)?;
            // Validate intent exists in config
            let intent_config = config.intents.get(intent.as_str())
                .ok_or_else(|| AmaError::Validation {
                    error_class: "unknown_intent".into(),
                    message: format!("intent '{}' not in intents.toml", intent.as_str()),
                })?;
            // Validate argument count matches template placeholders
            let raw_args = request.args.as_deref().unwrap_or(&[]);
            let placeholder_count = intent_config.args_template.iter()
                .filter(|t| t.contains("{{"))
                .count();
            if raw_args.len() != placeholder_count {
                return Err(AmaError::Validation {
                    error_class: "invalid_args".into(),
                    message: format!(
                        "intent '{}' expects {} args, got {}",
                        intent.as_str(), placeholder_count, raw_args.len()
                    ),
                });
            }
            // Validate each arg with its corresponding validator
            let mut args = Vec::new();
            for (i, raw_arg) in raw_args.iter().enumerate() {
                let safe = SafeArg::new(raw_arg)?;
                // Apply validator if specified
                if let Some(validator) = intent_config.validators.get(i) {
                    match validator.as_str() {
                        "relative_workspace_path" => {
                            WorkspacePath::new(raw_arg, &config.workspace_root)?;
                        }
                        _ => {} // Unknown validators are a config error caught at boot
                    }
                }
                args.push(safe);
            }
            Ok(CanonicalAction::ShellExec { intent, args })
        }
        "http_request" => {
            let method_str = request.method.as_deref().unwrap_or("");
            let method = HttpMethod::parse(method_str)?;
            let url = AllowlistedUrl::new(&request.target, &config.allowlist)?;
            let body = match &request.payload {
                Some(data) => {
                    let max = config.domain_mappings
                        .get("http_request")
                        .and_then(|m| m.max_payload_bytes)
                        .unwrap_or(262_144);
                    Some(BoundedBytes::new(data.clone(), max)?)
                }
                None => None,
            };
            Ok(CanonicalAction::HttpRequest { method, url, body })
        }
        _ => Err(AmaError::Validation {
            error_class: "unknown_action".into(),
            message: format!("unknown action: {}", request.action),
        }),
    }
}

/// Execute the canonical action (actuation step) with per-action timeout (C3, M7).
async fn actuate(
    action: CanonicalAction,
    action_id: &str,
    config: &AmaConfig,
) -> Result<ActionResult, AmaError> {
    match action {
        CanonicalAction::FileWrite { path, content } => {
            let timeout = action_timeout("file_write");
            let result = tokio::time::timeout(timeout, async {
                crate::actuator::file::file_write(&path, &content, action_id)
            }).await.map_err(|_| AmaError::ServiceUnavailable {
                message: "file_write timed out".into(),
            })??;
            Ok(ActionResult::FileWrite {
                bytes_written: result.bytes_written,
            })
        }
        CanonicalAction::FileRead { path } => {
            let timeout = action_timeout("file_read");
            let result = tokio::time::timeout(timeout, async {
                crate::actuator::file::file_read(&path, 524_288) // 512 KiB
            }).await.map_err(|_| AmaError::ServiceUnavailable {
                message: "file_read timed out".into(),
            })??;
            Ok(ActionResult::FileRead {
                content: result.content,
                bytes_returned: result.bytes_returned,
                total_bytes: result.total_bytes,
                truncated: result.truncated,
            })
        }
        #[cfg(unix)]
        CanonicalAction::ShellExec { intent, args } => {
            let intent_config = config.intents.get(intent.as_str())
                .ok_or_else(|| AmaError::ServiceUnavailable {
                    message: "intent config not found at actuation".into(),
                })?;
            // Build argument vector from template
            let mut exec_args: Vec<String> = Vec::new();
            for tmpl in &intent_config.args_template {
                if let Some(idx_str) = tmpl.strip_prefix("{{").and_then(|s| s.strip_suffix("}}")) {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if let Some(arg) = args.get(idx) {
                            exec_args.push(arg.as_str().to_string());
                            continue;
                        }
                    }
                }
                exec_args.push(tmpl.clone());
            }
            let working_dir = intent_config.working_dir
                .as_deref()
                .unwrap_or(config.workspace_root.to_str().unwrap_or("/tmp"));
            let timeout = action_timeout("shell_exec");
            let arg_refs: Vec<&str> = exec_args.iter().map(|s| s.as_str()).collect();
            let result = crate::actuator::shell::shell_exec(
                &intent_config.binary,
                &arg_refs,
                working_dir,
                action_id,
                timeout,
                65_536, // 64 KiB per stream
            ).await?;
            Ok(ActionResult::ShellExec {
                stdout: result.stdout,
                stderr: result.stderr,
                exit_code: result.exit_code,
                truncated: result.truncated,
            })
        }
        #[cfg(not(unix))]
        CanonicalAction::ShellExec { .. } => {
            Err(AmaError::ServiceUnavailable {
                message: "shell_exec is only supported on Unix/Linux".into(),
            })
        }
        CanonicalAction::HttpRequest { method, url, body } => {
            let timeout_dur = action_timeout("http_request");
            let result = tokio::time::timeout(timeout_dur, async {
                crate::actuator::http::http_request(
                    method,
                    &url,
                    body.as_ref(),
                    &config.allowlist,
                ).await
            }).await.map_err(|_| AmaError::ServiceUnavailable {
                message: "http_request timed out".into(),
            })??;
            Ok(ActionResult::HttpResponse {
                status_code: result.status_code,
                body: result.body,
                truncated: result.truncated,
            })
        }
    }
}

/// Full pipeline: validate -> map -> authorize -> actuate.
pub async fn process_action(
    request: ActionRequest,
    config: &AmaConfig,
    authorizer: &P0Authorizer,
    action_id: String,
    session_id: &str,
) -> Result<ActionResponse, AmaError> {
    let start = Instant::now();

    // 1. Validate magnitude
    validate_magnitude(request.magnitude)?;

    // 2. Validate mutual exclusivity of payload/args per action
    validate_field_exclusivity(&request)?;

    // 3. Canonicalize (construct newtypes — structural validation)
    let canonical = canonicalize(&request, config)?;

    // 4. Map to domain
    let mapping = map_action(&request.action, request.magnitude, config)?;

    // Compute request hash for audit
    let request_hash = compute_request_hash(&request.action, &request.target, request.magnitude);

    // 5. Dry-run check BEFORE capacity reservation
    if request.dry_run {
        let verdict = authorizer.check_only(&mapping.domain_id, mapping.magnitude);
        let status_str = match verdict {
            SlimeVerdict::Authorized => "authorized",
            SlimeVerdict::Impossible => "impossible",
        };
        // Audit log
        log_audit(&AuditEntry {
            timestamp: chrono_now(),
            session_id: session_id.into(),
            action_id: action_id.clone(),
            adapter: request.adapter.clone(),
            action: request.action.clone(),
            domain_id: mapping.domain_id.clone(),
            magnitude_effective: mapping.magnitude,
            duration_ms: start.elapsed().as_millis() as u64,
            status: status_str.into(),
            request_hash: request_hash.clone(),
            truncated: false,
        });
        return match verdict {
            SlimeVerdict::Authorized => Ok(ActionResponse {
                status: "authorized".into(),
                action_id,
                dry_run: true,
                result: None,
            }),
            SlimeVerdict::Impossible => Err(AmaError::Impossible),
        };
    }

    // 6. Reserve capacity (atomic CAS)
    match authorizer.try_reserve(&mapping.domain_id, mapping.magnitude) {
        SlimeVerdict::Authorized => {}
        SlimeVerdict::Impossible => {
            log_audit(&AuditEntry {
                timestamp: chrono_now(),
                session_id: session_id.into(),
                action_id: action_id.clone(),
                adapter: request.adapter.clone(),
                action: request.action.clone(),
                domain_id: mapping.domain_id.clone(),
                magnitude_effective: mapping.magnitude,
                duration_ms: start.elapsed().as_millis() as u64,
                status: "impossible".into(),
                request_hash,
                truncated: false,
            });
            return Err(AmaError::Impossible);
        }
    }

    // 7. Actuate
    let result = actuate(canonical, &action_id, config).await;

    let (status_str, truncated) = match &result {
        Ok(r) => ("authorized", r.is_truncated()),
        Err(_) => ("error", false),
    };

    log_audit(&AuditEntry {
        timestamp: chrono_now(),
        session_id: session_id.into(),
        action_id: action_id.clone(),
        adapter: request.adapter.clone(),
        action: request.action.clone(),
        domain_id: mapping.domain_id,
        magnitude_effective: mapping.magnitude,
        duration_ms: start.elapsed().as_millis() as u64,
        status: status_str.into(),
        request_hash,
        truncated,
    });

    let result = result?;

    Ok(ActionResponse {
        status: "authorized".into(),
        action_id,
        dry_run: false,
        result: Some(serde_json::to_value(&result).unwrap()),
    })
}

/// Helper: simple timestamp (no chrono dep — use std).
fn chrono_now() -> String {
    // Use system time formatted as ISO 8601
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}", duration.as_secs())
}
```

Add `is_truncated()` helper to `ActionResult` in `canonical.rs`:

```rust
impl ActionResult {
    pub fn is_truncated(&self) -> bool {
        match self {
            ActionResult::FileWrite { .. } => false,
            ActionResult::FileRead { truncated, .. } => *truncated,
            ActionResult::ShellExec { truncated, .. } => *truncated,
            ActionResult::HttpResponse { truncated, .. } => *truncated,
        }
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat: full request pipeline validate -> map -> authorize -> actuate (TDD)"
```

---

### Task 13: HTTP Server (Axum)

**Files:**
- Create: `src/server.rs`
- Update: `src/main.rs`
- Create: `tests/test_integration.rs`

- [ ] **Step 1: Write integration tests**

```rust
// tests/test_integration.rs
use ama::server::{test_server, test_server_with_capacity};

#[tokio::test]
async fn health_returns_ok() {
    let server = test_server().await;
    let resp = server.get("/health").await;
    resp.assert_status_ok();
    resp.assert_json(&serde_json::json!({"status": "ok"}));
}

#[tokio::test]
async fn version_returns_info() {
    let server = test_server().await;
    let resp = server.get("/version").await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn action_without_idempotency_key_returns_400() {
    let server = test_server().await;
    let resp = server.post("/ama/action")
        .json(&serde_json::json!({
            "adapter": "test",
            "action": "file_write",
            "target": "test.txt",
            "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn valid_file_write_returns_200() {
    let server = test_server().await;
    let resp = server.post("/ama/action")
        .add_header(
            axum::http::header::HeaderName::from_static("idempotency-key"),
            axum::http::header::HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).unwrap(),
        )
        .json(&serde_json::json!({
            "adapter": "test",
            "action": "file_write",
            "target": "test.txt",
            "magnitude": 1,
            "payload": "hello"
        }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn impossible_returns_403() {
    // Exhaust capacity first, then try another action
    let server = test_server_with_capacity(1).await;
    // First call succeeds
    server.post("/ama/action")
        .add_header(
            axum::http::header::HeaderName::from_static("idempotency-key"),
            axum::http::header::HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).unwrap(),
        )
        .json(&serde_json::json!({
            "adapter": "test", "action": "file_write",
            "target": "a.txt", "magnitude": 1, "payload": "x"
        }))
        .await
        .assert_status_ok();
    // Second call: capacity exhausted
    let resp = server.post("/ama/action")
        .add_header(
            axum::http::header::HeaderName::from_static("idempotency-key"),
            axum::http::header::HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).unwrap(),
        )
        .json(&serde_json::json!({
            "adapter": "test", "action": "file_write",
            "target": "b.txt", "magnitude": 1, "payload": "y"
        }))
        .await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
    resp.assert_json(&serde_json::json!({"status": "impossible"}));
}
```

- [ ] **Step 2: Implement server.rs**

```rust
// src/server.rs
use crate::config::AmaConfig;
use crate::errors::AmaError;
use crate::idempotency::{validate_idempotency_key, IdempotencyCache, IdempotencyStatus};
use crate::pipeline::process_action;
use crate::schema::ActionRequest;
use crate::slime::{P0Authorizer, SlimeAuthorizer};

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
use tower_http::limit::RequestBodyLimitLayer;
use uuid::Uuid;

/// Shared application state wrapped in Arc for thread-safe access.
pub struct AppState {
    pub config: AmaConfig,
    pub authorizer: P0Authorizer,
    pub idempotency_cache: IdempotencyCache,
    pub session_id: Uuid,
    pub start_time: Instant,
    /// Per-domain action counters (monotonic, reset on restart).
    pub domain_counters: HashMap<String, AtomicU64>,
    /// Global request counter for rate limiting.
    pub request_counter: AtomicU64,
    pub rate_limit_window_start: std::sync::Mutex<Instant>,
}

impl AppState {
    pub fn new(config: AmaConfig) -> Arc<Self> {
        let max_capacity = config.max_capacity;
        let session_id = Uuid::new_v4();

        // Convert config::DomainPolicy -> slime::DomainPolicy for P0Authorizer
        let slime_domains: Vec<(crate::slime::DomainId, crate::slime::DomainPolicy)> =
            config.domain_policies.iter().map(|(id, policy)| {
                (id.clone(), crate::slime::DomainPolicy {
                    enabled: policy.enabled,
                    max_magnitude_per_action: policy.max_magnitude_per_action,
                })
            }).collect();

        // Initialize per-domain counters from config
        let mut domain_counters = HashMap::new();
        for domain_id in config.domain_policies.keys() {
            domain_counters.insert(domain_id.clone(), AtomicU64::new(0));
        }

        Arc::new(Self {
            authorizer: P0Authorizer::new(max_capacity, slime_domains),
            idempotency_cache: IdempotencyCache::new(10_000, std::time::Duration::from_secs(300)),
            config,
            session_id,
            start_time: Instant::now(),
            domain_counters,
            request_counter: AtomicU64::new(0),
            rate_limit_window_start: std::sync::Mutex::new(Instant::now()),
        })
    }
}

/// Build the Axum router with all middleware and endpoints.
pub fn build_router(state: Arc<AppState>) -> Router {
    let app = Router::new()
        // Endpoints
        .route("/ama/action", post(handle_action))
        .route("/ama/status", get(handle_status))
        .route("/health", get(handle_health))
        .route("/version", get(handle_version))
        // Application state
        .with_state(state.clone())
        // Middleware stack (applied bottom-up)
        .layer(
            ServiceBuilder::new()
                // Body size limit: 1 MiB (outermost — rejects before reading)
                .layer(RequestBodyLimitLayer::new(1_048_576))
                // Concurrency limit: 8 connections (503 on overflow)
                .concurrency_limit(8)
        )
        // Content-Type validation middleware (applied to all routes)
        .layer(middleware::from_fn_with_state(
            state, content_type_middleware,
        ));

    app
}

/// Content-Type validation middleware.
/// POST requests MUST have Content-Type: application/json.
/// GET requests are always allowed.
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
            return AmaError::UnsupportedMediaType.into_response();
        }
    }
    next.run(req).await
}

/// Rate limiter check: 60 req/min global (spec Section 2).
/// Returns true if request is allowed, false if rate limited.
fn check_rate_limit(state: &AppState) -> bool {
    let mut window_start = state.rate_limit_window_start.lock().unwrap();
    let now = Instant::now();
    let elapsed = now.duration_since(*window_start);

    // Reset window every 60 seconds
    if elapsed.as_secs() >= 60 {
        *window_start = now;
        state.request_counter.store(1, Ordering::SeqCst);
        return true;
    }

    let count = state.request_counter.fetch_add(1, Ordering::SeqCst);
    count < 60
}

/// Increment per-domain action counter.
fn increment_domain_counter(state: &AppState, domain_id: &str) {
    if let Some(counter) = state.domain_counters.get(domain_id) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

// === Endpoint Handlers ===

/// GET /health — Liveness check (spec Section 2).
async fn handle_health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

/// GET /version — Returns SAFA version and schema version (spec Section 2).
async fn handle_version() -> impl IntoResponse {
    Json(json!({
        "name": "ama",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": "ama-action-v1"
    }))
}

/// GET /ama/status — Read-only thermodynamic state (M6, spec Section 6).
/// Exposes session_id, capacity, uptime, and per-domain action counts.
/// NEVER exposes full config, allowlists, or intents.
async fn handle_status(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();
    let capacity_used = state.authorizer.capacity_used();
    let capacity_max = state.authorizer.capacity_max();

    // Build per-domain status
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

/// POST /ama/action — Submit action for validation + actuation (spec Section 2).
///
/// Flow:
/// 1. Rate limit check (429 on exceed)
/// 2. Extract + validate Idempotency-Key header (400 if missing/malformed)
/// 3. Check idempotency cache (return cached / reject in-flight / 503 if full)
/// 4. Deserialize JSON body
/// 5. Delegate to pipeline::process_action
/// 6. Cache result and return
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
    let idem_key_str = match headers.get("Idempotency-Key") {
        Some(val) => match val.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return AmaError::BadRequest {
                message: "Idempotency-Key header is not valid ASCII".into(),
            }.into_response(),
        },
        None => return AmaError::BadRequest {
            message: "missing Idempotency-Key header".into(),
        }.into_response(),
    };

    // Validate UUID v4 format
    let idem_key = match validate_idempotency_key(&idem_key_str) {
        Ok(k) => k,
        Err(e) => return e.into_response(),
    };

    // 3. Idempotency cache check
    match state.idempotency_cache.check_or_insert(idem_key) {
        IdempotencyStatus::Cached(cached_response) => {
            // Return cached response body as-is
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                cached_response,
            ).into_response();
        }
        IdempotencyStatus::InFlight => {
            return AmaError::Conflict {
                message: "duplicate Idempotency-Key with in-flight request".into(),
            }.into_response();
        }
        IdempotencyStatus::Full => {
            // Fail-closed: 503 when cache is full (spec Section 2)
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

    // 4. Deserialize request body
    let request: ActionRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            state.idempotency_cache.remove(&idem_key);
            return AmaError::BadRequest {
                message: format!("invalid JSON: {}", e),
            }.into_response();
        }
    };

    // Capture action name for domain counter before moving request
    let action_name = request.action.clone();
    let magnitude = request.magnitude;

    // Generate action_id
    let action_id = Uuid::new_v4().to_string();

    // 5. Process through pipeline
    let result = process_action(
        request,
        &state.config,
        &state.authorizer,
        action_id,
        &state.session_id.to_string(),
    ).await;

    // 6. Build response and cache
    match result {
        Ok(response) => {
            let response_json = serde_json::to_string(&response).unwrap();

            // Increment per-domain action counter (monotonic)
            if let Ok(mapping) = crate::mapper::map_action(
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
            state.idempotency_cache.remove(&idem_key);
            e.into_response()
        }
    }
}

/// Graceful shutdown signal handler (M8).
/// Waits for SIGTERM (Unix) or Ctrl+C.
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
/// Defined as `pub` (not `#[cfg(test)]`) so integration tests in `tests/` can use it.
pub async fn test_server() -> axum_test::TestServer {
    test_server_with_capacity(10_000).await
}

/// Test helper: build a test server with custom capacity.
pub async fn test_server_with_capacity(max_capacity: u64) -> axum_test::TestServer {
    use crate::config::{AmaConfig, DomainPolicy, DomainMapping, BootHashes};

    // Create a temp workspace dir
    let workspace = std::env::temp_dir().join(format!("ama-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&workspace).unwrap();

    // Minimal test config — field names must match config.rs structs exactly
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
        boot_hashes: BootHashes {
            config_hash: "test".into(),
            domains_hash: "test".into(),
            intents_hash: "test".into(),
            allowlist_hash: "test".into(),
        },
    };

    let state = AppState::new(config);
    let app = build_router(state);
    axum_test::TestServer::new(app).unwrap()
}
```

- [ ] **Step 3: Update lib.rs with all modules**

All public modules must be declared in `lib.rs` so both `main.rs` and integration tests can access them:

```rust
// src/lib.rs
pub mod errors;
pub mod newtypes;
pub mod canonical;
pub mod schema;
pub mod config;
pub mod slime;
pub mod mapper;
pub mod actuator;
pub mod audit;
pub mod idempotency;
pub mod pipeline;
pub mod server;
```

- [ ] **Step 4: Update main.rs**

`main.rs` uses `use ama::` (the lib crate) — no `mod` declarations (avoids duplicate module definitions):

```rust
// src/main.rs
use ama::config::AmaConfig;
use ama::server::{AppState, build_router, shutdown_signal};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging
    tracing_subscriber::fmt::init();

    // Load and validate all configuration
    let config = AmaConfig::load(Path::new("config"))?;
    tracing::info!(hashes = ?config.boot_hashes, "Boot integrity verified");

    // M9: Clean up orphan temp files from previous crashed sessions
    let cleaned = ama::actuator::file::cleanup_orphan_temps(&config.workspace_root);
    if cleaned > 0 {
        tracing::warn!(count = cleaned, "Cleaned up orphan temp files from previous session");
    }

    // Build application state
    let bind_addr = format!("{}:{}", config.bind_host, config.bind_port);
    let state = AppState::new(config);

    // Build router with all middleware
    let app = build_router(state);

    // Bind to localhost only (spec Section 2)
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "SAFA P0 listening");

    // Start server with graceful shutdown (M8)
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("SAFA P0 shut down cleanly");
    Ok(())
}
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: axum HTTP server with full pipeline integration (TDD)"
```

---

### Task 14: Smoke Test & Final Verification

- [ ] **Step 1: Manual smoke test (Linux/WSL)**

```bash
# Terminal 1: start SAFA
cargo run

# Terminal 2: test endpoints
curl http://127.0.0.1:8787/health
curl http://127.0.0.1:8787/version
curl http://127.0.0.1:8787/ama/status

# File write
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{"adapter":"test","action":"file_write","target":"hello.txt","magnitude":1,"payload":"Hello SAFA!"}'

# Verify file exists
cat /tmp/ama-workspace/hello.txt
```

- [ ] **Step 2: Run full test suite**

```bash
cargo test -- --test-threads=1
cargo clippy -- -D warnings
```

- [ ] **Step 3: Final commit**

```bash
git add -A && git commit -m "chore: smoke test verification and clippy clean"
git push
```

---

## Summary

| Task | Component | Tests | Key Invariant |
|------|-----------|-------|---------------|
| 1 | Scaffold | — | Compiles |
| 2 | Newtypes | 11 adversarial | Structural impossibility by construction |
| 3 | Canonical + Schema | 6 | Type-safe enum = valid action |
| 4 | Config | 4 | Boot validation, SHA-256 integrity |
| 5 | SLIME AB-S | 6 (inc. concurrent) | Capacity NEVER exceeds max |
| 6 | Mapper | 3 | Closed world mapping |
| 7 | File actuators | 6 | Atomic write, symlink protection |
| 8 | Shell actuator | 3 | execv only, process isolation |
| 9 | HTTP actuator | 5 | DNS/IP safety, redirect validation |
| 10 | Idempotency | 5 | Fail-closed cache overflow |
| 11 | Audit | — | SHA-256 request hash, no payload logged |
| 12 | Pipeline | 4 | Full flow, dry_run correctness |
| 13 | Server | 5 | Rate limit, concurrency, Content-Type |
| 14 | Smoke | manual | End-to-end verification |
