# AMA P0 Design Specification

**Document ID:** `MN-001-AMA-P0-SPEC-20260313`
**Status:** SEALED
**Date:** 2026-03-13
**Authors:** Sebastien Bouchard (Fireplank), Claude, GPT-4, Qwen
**Schema Version:** `ama-spec-v1`

---

## Section 1 — Purpose & Non-Goals

### Purpose

AMA (Agent Machine Armor) is a universal membrane placed between an AI agent and real actuation surfaces: filesystem, bounded binary execution, and outbound network access. It translates agent intentions into canonical actions, validates them through SLIME/AB-S, and permits real-world actuation only after binary authorization.

AMA is not an agent. It is an adapter, proxy, validator, and minimal actuator.

**One-line definition:** *"AMA translates agent intentions into canonical SLIME domains and permits real-world actuation only after binary authorization."*

### Non-Goals (P0)

- **Not an agent:** AMA makes no semantic or strategic decisions.
- **Not a latency optimization layer:** Interception standardization is the goal.
- **No inbound network actuation** (listen/accept) in P0.
- **No TLS in P0:** Localhost-only transport.
- **No complex authentication in P0.**
- **No multi-tenancy in P0.**
- **No arbitrary shell execution:** Intent mapping only, never raw shell strings.
- **No semantic explanation of decisions:** AMA may return minimal status and local structural errors, but never policy-style reasoning ("why this action was forbidden").
- P0 is designed for single-host local deployment and binds only to `127.0.0.1`. It does not attempt to defend against a fully compromised local host.

---

## Section 2 — Transport Protocol

### Binding & Network

- **Host:** `127.0.0.1` (strictly bound, never `0.0.0.0`).
- **Port:** `8787` (configurable via `config.toml`).
- **TLS:** Disabled (P0 assumes single-host local trust boundary).
- **Protocol:** HTTP/1.1 (Keep-Alive enabled).

### Endpoints

| Method | Path           | Purpose                                          |
|--------|----------------|--------------------------------------------------|
| `POST` | `/ama/action`  | Submit action for validation + actuation.        |
| `GET`  | `/health`      | Liveness check.                                  |
| `GET`  | `/version`     | Returns AMA version and schema version.          |
| `GET`  | `/ama/status`  | Read-only thermodynamic state (capacity, domains).|

### Headers & Content-Type

- **Request `Content-Type`:** `application/json` (required). Else -> `415 Unsupported Media Type`.
- **Response `Content-Type`:** `application/json` (always).
- **Encoding:** UTF-8.
- **`Idempotency-Key`:** **Required** on `POST /ama/action`. Must be a well-formed UUID v4 (validated via regex `^[0-9a-f]{8}-...`), max 128 bytes ASCII.
  - Duplicate key within 5-minute window returns cached response without re-execution.
  - Missing, malformed, or non-UUID-v4 key -> `400 Bad Request`.
  - **Cache semantics:** The idempotency cache is authorization-preserving: capacity was already consumed on the first call, so a cached `200` is valid even if subsequent actions have exhausted the remaining capacity.
  - If the original request is still in-flight when a duplicate key arrives, the duplicate is rejected with `409 Conflict`.
  - The cache does NOT survive process restart (session-scoped).
  - Max cache size: 10,000 entries. Eviction policy: oldest-first beyond TTL, then LRU if size exceeded.

### Limits (P0)

| Parameter        | Value       | Rationale                                          |
|------------------|-------------|-----------------------------------------------------|
| **Body Max**     | 1 MiB       | Prevents memory exhaustion; sufficient for text files.|
| **Concurrency**  | 8 connections | Single agent focus; small burst buffer.             |
| **Rate Limit**   | 60 req/min  | Global (all clients combined). P0 is localhost-only, so per-IP = global. Prevents infinite loops from agents. |

**Timeouts (per action class):**

| Action Class      | Timeout |
|-------------------|---------|
| `file_write`      | 5s      |
| `file_read`       | 5s      |
| `shell_exec`      | 15s     |
| `http_request`    | 15s     |

**Overload behavior:** Beyond concurrency cap -> immediate `503` rejection. No request queue.

### HTTP Status Codes

| Code  | Meaning                              | Body Content Rules                                       |
|-------|--------------------------------------|-----------------------------------------------------------|
| `200` | Authorized & Executed (or Dry-Run).  | `{"status":"authorized","action_id":"...","dry_run":false,"result":{...}}` |
| `400` | Malformed JSON, missing fields, wrong types, bad Idempotency-Key. | Minimal error message. |
| `403` | **Impossible** (AB-S Refusal).       | **STRICT:** `{"status":"impossible"}`. No reason, no codes, no leakage. |
| `409` | Conflict: duplicate Idempotency-Key with in-flight request. | Minimal error. |
| `413` | Payload Too Large (> 1 MiB global).  | Minimal error. |
| `415` | Unsupported Media Type.              | Minimal error. |
| `422` | Semantic validation error (unknown action, invalid path, URL not allowlisted, magnitude out of bounds, per-domain payload exceeded). | Minimal error message (`error_class` + `message`). |
| `429` | Rate Limit Exceeded.                 | Minimal error. |
| `503` | Service Unavailable / Overloaded.    | **Fail-Closed.** Action rejected. |

**Error response format (400/422):**
```json
{
  "status": "error",
  "error_class": "invalid_target",
  "message": "target rejected by local validation"
}
```
Public error messages are kept minimal. Detailed diagnostics go to local logs only.

### Response Schemas

**Health:**
```json
{ "status": "ok" }
```

**Version:**
```json
{
  "name": "ama",
  "version": "0.1.0",
  "schema_version": "ama-action-v1"
}
```

### Dry-Run Behavior

When `"dry_run": true`:
- Full pipeline execution: Parse -> Validate -> Map -> AB-S Check.
- **Actuation step skipped.**
- Response mirrors what *would* have happened (`200` or `403`), with `"dry_run": true`.
- No fake actuation result returned. Response is status-only.
- Consumes no capacity (magnitude not reserved).

**Normative rule:** `dry_run` MUST still pass all local validation and AB-S authorization checks; only final actuation is skipped.

### Failure Mode

- **Fail-Closed:** If AB-S is unreachable or any unexpected error occurs, AMA MUST return `503` and MUST NOT perform any actuation.
- Logs internal error for operator diagnosis; returns generic `503` to client.

---

## Section 3 — Canonical Action Model

### Input Schema (Agent -> AMA)

Agents submit a universal JSON structure. AMA enforces strict structural validation before any semantic processing.

```json
{
  "adapter": "generic",
  "action": "file_write",
  "target": "workspace/test.txt",
  "magnitude": 1,
  "dry_run": false,
  "payload": "hello world"
}
```

| Field       | Type           | Required    | Description & Constraints |
|-------------|----------------|-------------|---------------------------|
| `adapter`   | string         | Yes         | Agent identifier (e.g., `"openclaw"`, `"generic"`, `"langchain"`). Used for **audit trails** and observability. Does not affect authorization logic. **MUST NOT** affect validation, authorization, or actuation. |
| `action`    | string         | Yes         | Canonical action class. Must match an entry in `domains.toml`. Unknown action -> `422`. |
| `target`    | string         | Yes         | Action target: relative file path, shell intent ID, or URL. Format validated per action type. |
| `magnitude` | u64            | Yes         | Claimed cost units. Range: `1 <= magnitude <= 1000` (P0). Out of bounds -> `422`. AMA MAY reject, clamp, or recompute effective magnitude per domain rules before SLIME authorization. |
| `dry_run`   | bool           | No          | Default `false`. If `true`, skips actuation step only. |
| `method`    | string         | Conditional | For `http_request` only. Must be `"GET"` or `"POST"` (P0). Required when `action` is `http_request`. Absent or invalid -> `422`. |
| `payload`   | string or null | Conditional | Action data (file content, HTTP body). Required for `file_write`. Optional for `http_request` (POST body). Null for read-only ops. Max per-domain limit. |
| `args`      | string[]       | Conditional | For `shell_exec` only. Validated arguments for the intent. |

**Mutual exclusivity:** Exactly one of `payload` or `args` must be present, depending on `action`:
- `file_write`: `payload` required, `args` forbidden.
- `file_read`: neither `payload` nor `args`.
- `shell_exec`: `args` required, `payload` forbidden.
- `http_request`: `payload` optional (for POST body), `args` forbidden.
Providing the wrong field for an action class -> `422`.

### P0 Action Matrix

| Action Class | Target Format            | Data Field     | Security Constraint |
|--------------|--------------------------|----------------|---------------------|
| `file_write` | Relative workspace path  | `payload`      | Path traversal check, no absolute paths, no symlinks. |
| `file_read`  | Relative workspace path  | (none)         | Path traversal check. Must exist. |
| `shell_exec` | **Intent ID**            | `args: [...]`  | **NO RAW SHELL.** Args validated per intent validators before mapping to binary. |
| `http_request` | Absolute URL           | `payload` (optional) | HTTPS only. URL allowlist + method check. |

**Note:** `http_get` and `http_post` are unified as `http_request` with a mandatory `method` field in both the input JSON and internal representation.

### Internal Representation (Rust Enum)

Upon successful validation, JSON is deserialized into a type-safe enum. Construction of these variants guarantees structural validity (Newtype Pattern).

```rust
pub enum CanonicalAction {
    FileWrite {
        path: WorkspacePath,     // Guarantees: inside workspace, no traversal, no symlink
        content: BoundedBytes,   // Guarantees: < per-domain max, valid UTF-8
    },
    FileRead {
        path: WorkspacePath,
    },
    ShellExec {
        intent: IntentId,        // Guarantees: exists in intents.toml
        args: Vec<SafeArg>,      // Guarantees: sanitized, validated per intent validators
    },
    HttpRequest {
        method: HttpMethod,      // GET or POST (P0)
        url: AllowlistedUrl,     // Guarantees: https, matched allowlist, safe IP
        body: Option<BoundedBytes>,
    },
}
```

*If parsing fails to construct a variant -> `400` or `422`. If a variant exists, it is structurally sound by construction.*

### Transformation Pipeline

```
1. INGESTION
   Raw JSON received. Idempotency-Key checked.

2. SCHEMA VALIDATION
   JSON well-formed, required fields present, types correct.
   Fail -> 400.

3. STRUCTURAL VALIDATION
   Action known in domains.toml, target format valid per validator,
   magnitude in [1, max], payload size within domain limit.
   Fail -> 422.

4. CANONICALIZATION
   Deserialization into CanonicalAction enum (type-safe).

5. DOMAIN MAPPING
   Lookup domain_id in domains.toml. Compute effective magnitude.

6. AB-S AUTHORIZATION
   (domain_id, magnitude) -> SLIME.
   IMPOSSIBLE -> 403 {"status": "impossible"}.
   AUTHORIZED -> proceed.

7. ACTUATION (if not dry_run)
   Execute real-world effect. Return typed result.
```

### Canonical Action Versioning

Canonical actions are versioned implicitly by the `schema_version` field in `domains.toml`. Breaking changes require a schema version bump.

### Semantic Boundary

**AMA does not interpret agent intent beyond canonical mapping. All semantic meaning is discarded before SLIME authorization.**

---

## Section 4 — Domain Mapping & Validation

### Philosophy

AMA acts as a stateless translator. It maps agent intents to SLIME domains using static, versioned configuration files. No semantic interpretation occurs. If the mapping fails, the action is structurally invalid.

### Configuration Hierarchy

```
config.toml       -> SLIME capacity, domain caps, global settings
domains.toml       -> Action -> Domain ID mapping, validators
intents.toml       -> Shell intent -> Binary + Args mapping
allowlist.toml     -> HTTP URL patterns, methods
```

All configuration files are loaded **once at startup** and **never reloaded at runtime**. Changes require a full process restart.

### 4.1 — `domains.toml` (Action -> Domain ID)

```toml
[meta]
schema_version = "ama-domains-v1"
max_magnitude_claim = 1000

[domains.file_write]
domain_id = "fs.write.workspace"
max_payload_bytes = 1_048_576
validator = "relative_workspace_path"

[domains.file_read]
domain_id = "fs.read.workspace"
validator = "relative_workspace_path"

[domains.shell_exec]
domain_id = "proc.exec.bounded"
requires_intent = true

[domains.http_request]
domain_id = "net.out.http"
max_payload_bytes = 262_144
validator = "allowlisted_url"
```

**`workspace_root`** is defined in `config.toml` and MUST resolve to an absolute path at startup. Relative canonical examples like `./workspace` are not accepted.

### 4.2 — `intents.toml` (Shell Intent Mapping)

```toml
[meta]
schema_version = "ama-intents-v1"

[intents.list_dir]
binary = "/bin/ls"
args_template = ["-la", "{{0}}"]
validators = ["relative_workspace_path"]
description = "List directory contents"

[intents.read_file_cat]
binary = "/bin/cat"
args_template = ["{{0}}"]
validators = ["relative_workspace_path"]
description = "Read file contents via cat"

[intents.git_status]
binary = "/usr/bin/git"
args_template = ["status"]
validators = []
working_dir = "{{workspace_root}}"
description = "Git status in workspace"

[intents.git_log]
binary = "/usr/bin/git"
args_template = ["log", "--oneline", "-20"]
validators = []
working_dir = "{{workspace_root}}"
description = "Recent git history"
```

**Security rules:**
- `binary` MUST be an absolute path to an executable.
- `args_template` uses `{{N}}` for positional argument substitution.
- `validators[N]` validates `{{N}}` — positional mapping is strict.
- Number of placeholders MUST exactly match the number of provided arguments: `args.len() != placeholder_count` -> `422`. Both too few AND too many arguments are rejected.
- No extra arguments accepted. No unresolved placeholders may survive.
- If any argument fails its validator, the command vector is **never constructed**. Request returns `422`.
- Arguments are constructed via **safe vector concatenation of typed values**, never string interpolation.
- `working_dir` MUST be absolute or derived deterministically from `workspace_root` at startup. Never injected by the client.

### 4.3 — `allowlist.toml` (Network Boundaries)

```toml
[meta]
schema_version = "ama-allowlist-v1"

[[urls]]
pattern = "https://api.weather.com/*"
methods = ["GET"]

[[urls]]
pattern = "https://api.github.com/*"
methods = ["GET", "POST"]
max_body_bytes = 262_144

[[urls]]
pattern = "https://httpbin.org/*"
methods = ["GET", "POST"]
notes = "Testing only"
```

**Rules:**
- `https` only. No HTTP cleartext.
- Glob matching (`*`) — simple, deterministic, no regex.
- URL normalized before matching (scheme, host, port, path). Userinfo, fragments rejected.
- Method must be in `methods[]`.
- Unmatched URL -> `422`.

### 4.4 — Validators

#### `relative_workspace_path`

This validator MUST:
1. Reject absolute paths (starting with `/` or drive letter).
2. Reject `..` segments.
3. Reject empty or ambiguous segments.
4. Normalize lexically before joining with `workspace_root`.
5. Join with `workspace_root` (absolute path).
6. Resolve symlinks via `lstat` on **every path component** (not just leaf).
7. Verify the final canonical resolved path remains under `workspace_root`.

Failure at any step -> `422`.

#### `allowlisted_url`

This validator MUST:
1. Verify scheme is `https`.
2. Reject userinfo, fragments, non-standard ports unless explicitly allowed.
3. Normalize URL.
4. Match against `allowlist.toml` patterns.

### 4.5 — Result Schemas (per action)

```rust
pub enum ActionResult {
    FileWrite {
        bytes_written: u64,
    },
    FileRead {
        content: String,
        bytes_returned: u64,
        total_bytes: u64,
        truncated: bool,
    },
    ShellExec {
        stdout: String,
        stderr: String,
        exit_code: i32,
        truncated: bool,
    },
    HttpResponse {
        status_code: u16,
        body: String,
        truncated: bool,
    },
}
```

**Truncation limits (P0):**
- `FileRead`: Max 512 KiB returned. AMA stops reading at the cap (bounded reading).
- `ShellExec`: Max 64 KiB per stream (stdout/stderr).
- `HttpResponse`: Max 256 KiB body.

If data is truncated, `truncated: true` AND `total_bytes` (when known) are returned. The agent MUST be aware of data loss. Hiding truncation is forbidden.

**Text-oriented P0:** AMA P0 supports text-oriented outputs only. Non-UTF-8 data results in actuator failure (`503`). Binary output support is out of scope for P0.

---

## Section 5 — Actuator Rules

### Philosophy

The Actuator is the final mechanical stage. It executes ONLY if authorized by SLIME. It possesses zero decision logic. Its sole purpose is safe, deterministic effectuation.

### 5.1 — File Write

| Rule                    | P0 Value                                           |
|-------------------------|-----------------------------------------------------|
| Workspace root          | Absolute path, resolved at startup.                 |
| Paths accepted          | Relative only, no `..`, no absolute.                |
| Symlink policy          | **STRICT NOFOLLOW** on every path component.        |
| Target type             | Regular files only. Non-regular targets (dir, device, fifo, socket, symlink) MUST be rejected. |
| Max size                | 1 MiB (verified before write).                      |
| Directory creation      | `mkdir -p` implicit. Permissions: `0755`.           |
| Overwrite               | Allowed (last-rename-wins).                         |
| Atomicity               | Write to `<target>.ama.<action_id>.tmp`, then `rename()`. Unique temp per action prevents concurrent write corruption. |
| File permissions        | `0644` (`rw-r--r--`).                               |
| Encoding                | P0 FileWrite is text-only. Valid UTF-8 enforced.    |
| Result                  | `{ bytes_written: u64 }`                            |
| Timeout                 | 5s                                                  |

**Execution sequence:**
1. Resolve canonical path -> verify it remains under `workspace_root`.
2. Verify **every path component** is not a symlink (`lstat`).
3. Verify target is a regular file or does not yet exist.
4. Create parent directories if needed (`0755`).
5. Write to `<target>.ama.<action_id>.tmp` (unique per action, prevents concurrent corruption).
6. Atomic `rename()` to `<target>`.
7. Return `bytes_written`.

**Cleanup:** If write fails mid-operation, the `.ama.<action_id>.tmp` file is deleted. No orphan files.

### 5.2 — File Read

| Rule                | P0 Value                                     |
|---------------------|-----------------------------------------------|
| Paths accepted      | Relative, under `workspace_root`, no symlinks.|
| File must exist     | Yes. Absent -> `503` (actuation failure). File existence is checked at actuation time (step 7), not at validation time. Capacity IS consumed even if the file does not exist, because AB-S authorization (step 6) occurs before actuation. |
| Target type         | Regular files only.                           |
| Max returned        | 512 KiB (bounded reading — stop at cap).      |
| Encoding            | UTF-8 enforced. Non-UTF-8 -> `422`.           |
| Truncation          | `truncated: true` + `total_bytes` if > 512 KiB.|
| Timeout             | 5s                                            |

### 5.3 — Shell Exec

| Rule                   | P0 Value                                        | Security Mechanism |
|------------------------|-------------------------------------------------|---------------------|
| Invocation             | `execv()` direct.                               | **NEVER** `sh -c`. No shell interpreter. |
| Commands accepted      | Only intents defined in `intents.toml`.         | Closed set. |
| Args construction      | Vector concatenation of typed args.             | No string interpolation. |
| Process isolation      | New process group (`setpgid`).                  | Allows killing entire family tree. |
| Working directory      | Set on child process directly.                  | Parent AMA process CWD **never** changed. |
| Environment            | Fresh minimal. Not inherited from parent.       | `PATH=/usr/bin:/bin`, `HOME=<workspace_root>`, `LANG=en_US.UTF-8`, `AMA_ACTION_ID=<uuid>` |
| stdout/stderr          | Captured separately, max 64 KiB each.           | Truncation flagged. |
| Non-UTF-8 output       | Actuator failure (`503`).                       | P0 text-oriented. |
| Timeout                | 15s hard limit.                                 | Kill sequence below. |
| Exit code              | Returned as-is.                                 | AMA does not interpret success/failure. |
| Descendant containment | Process-group termination where supported.      | P0 does not guarantee perfect prevention across all platforms. |

**Kill sequence:**
1. `t=0`: Spawn in new process group (`setpgid`).
2. `t=15s`: Send `SIGTERM` to **PGID**.
3. `t=17s`: Send `SIGKILL` to **PGID** if still alive.
4. Collect outputs & exit code.

### 5.4 — HTTP Request

| Rule               | P0 Value                                         |
|---------------------|---------------------------------------------------|
| Scheme              | `https` only. HTTP -> `422`.                      |
| TLS                 | Certificate validation MUST be enabled. Invalid certs MUST be rejected. |
| Allowlist           | Match against `allowlist.toml` (pattern + method).|
| DNS/IP safety       | Reject targets resolving to loopback, RFC1918, link-local, or metadata endpoints. DNS resolved and validated at request time. Remote IP re-validated after connection establishment. |
| Redirects           | Max 3. Re-validate **every** redirect against allowlist. POST redirects rejected in P0. |
| Timeouts            | Connect: 5s. Total: 15s.                         |
| POST body max       | 256 KiB (per-domain configurable).                |
| Response body max   | 256 KiB. Truncated with `truncated: true`.        |
| Non-UTF-8 response  | Rejected. P0 text-oriented.                       |
| User-Agent          | `AMA/0.1.0` (fixed, not configurable by agent).  |
| Headers              | No custom headers in P0.                          |
| Cookies             | Disabled. No HTTP state.                          |
| Streaming           | No. Complete buffered response.                   |

**DNS rebinding protection:** DNS resolution MUST be performed at request time with DNS caching disabled (or TTL=0). The resolved IP MUST be validated against the IP safety policy before connection. After connection establishment, the **actual TCP peer IP** (not a separate DNS lookup) MUST be re-validated. This prevents DNS rebinding attacks where DNS returns a safe IP initially but the connection goes to a different IP.

### 5.5 — Transversal Rules

| Rule               | Application                                       |
|---------------------|---------------------------------------------------|
| **Fail-Closed**    | Any unexpected error during actuation -> `503`. No partial results fabricated. |
| **No Retry**       | AMA never retries a failed actuation. Agent must resubmit. |
| **Cleanup**        | Failed writes delete `.ama.<action_id>.tmp`. No orphan files. |
| **Concurrency**    | No file locking in P0. Concurrent outcomes are timing-dependent (last successful atomic rename wins). |
| **Logging**        | Metadata-only per action. See Audit section below. |

### 5.6 — Audit Logging

Each actuation is logged with metadata only. **Full payloads, file contents, HTTP bodies, and stdout are NEVER logged by default.**

**Log fields:**
```
timestamp
session_id
action_id
adapter
action
domain_id
magnitude_effective
duration_ms
status (authorized / impossible / error)
request_hash (SHA-256 of canonicalized action)
truncated (bool)
```

Output limits apply **before** logging — truncated outputs are not written to logs.

The `request_hash` is computed over the canonical action representation (not raw JSON, which may vary in field order). This enables non-repudiation: auditors can verify exactly what was executed without storing full payloads.

---

## Section 6 — SLIME Integration (Embedded AB-S)

### Architecture Choice: Embedded

For P0, Anathema-Breaker State (AB-S) is **embedded directly** into the AMA binary as a Rust library (`no_std` compatible).

- **No Network Dependency:** Zero latency, zero external SPOF.
- **Fail-Secure:** If AMA runs, the Law runs.
- **Deterministic:** Pure function evaluation. Same input -> Same verdict, always.

The authorization interface MUST remain identical between embedded and remote modes. Remote SLIME via HTTP/gRPC is deferred to P1.

### Authorization Interface

```rust
pub trait SlimeAuthorizer {
    /// Atomic reservation attempt. Returns verdict immediately.
    fn try_reserve(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict;
}

/// Binary verdict — no middle ground
pub enum SlimeVerdict {
    Authorized,  // Capacity reserved, transition valid
    Impossible,  // Transition invalid OR capacity exhausted
}
```

**Properties:**
- **Deterministic:** Same input = same output, always.
- **Pure:** No side-effects, no I/O (except atomic counter).
- **Total:** Covers all possible domain_ids (unknown -> Impossible).
- **Synchrone:** No async, no timeout.

**DomainId values are stable string identifiers defined by the AMA schema.** They do not change between versions without a schema version bump.

### Thermodynamic Accounting (SYF-Shield)

Capacity is treated as **entropy**: it can only increase (be consumed), never decrease within a session.

**Capacity is consumed at authorization time, regardless of actuator success.** This is thermodynamically correct: energy expended does not return.

#### Atomic Reservation Logic (CAS)

```rust
pub struct P0Authorizer {
    capacity: AtomicU64,
    max_capacity: u64,
    domains: HashMap<DomainId, DomainPolicy>,
    session_id: Uuid,
}

pub struct DomainPolicy {
    enabled: bool,
    max_magnitude_per_action: u64,
}

impl SlimeAuthorizer for P0Authorizer {
    fn try_reserve(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict {
        // 1. Domain must exist (Closed World)
        let policy = match self.domains.get(domain_id) {
            Some(p) => p,
            None => return SlimeVerdict::Impossible,
        };

        // 2. Domain must be enabled
        if !policy.enabled {
            return SlimeVerdict::Impossible;
        }

        // 3. Per-action magnitude cap
        if magnitude > policy.max_magnitude_per_action {
            return SlimeVerdict::Impossible;
        }

        // 4. Atomic CAS reservation (race-safe, saturating)
        loop {
            let current = self.capacity.load(Ordering::Acquire);
            match current.checked_add(magnitude) {
                Some(new) if new <= self.max_capacity => {
                    match self.capacity.compare_exchange_weak(
                        current, new,
                        Ordering::AcqRel, Ordering::Acquire
                    ) {
                        Ok(_) => return SlimeVerdict::Authorized,
                        Err(_) => continue, // Retry on concurrent modification
                    }
                }
                _ => return SlimeVerdict::Impossible,
            }
        }
    }
}
```

**Guarantee:** `capacity` NEVER exceeds `max_capacity`. Structural impossibility enforced by hardware atomic instructions.

### Session & Reset Policy

- **Monotonicity:** Capacity only increases. No API to decrement or reset during runtime.
- **Session Boundary:** Capacity resets to `0` ONLY on full process restart (voluntary, crash, or upgrade).
- **Session ID:** On boot, AMA generates a random `session_id` (UUID v4). All audit logs include `session_id`. Enables detection of abnormal restart loops.
- **Cold Start Only:** To change laws or reset capacity, the operator must stop and restart the process. This is a feature, not a bug (Thermodynamic Cooling).
- **Restart-loop protection** is delegated to the operating system (systemd restart limits, container policies). AMA does not self-protect against forced restarts.

### Configuration (config.toml — The Law at Boot)

```toml
[ama]
workspace_root = "/var/lib/ama/workspace"  # MUST be absolute
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
```

**Closed World Assumption:** Any domain ID not listed explicitly is structurally `Impossible`. No domains can be added at runtime.

#### `config.toml` Formal Schema

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `ama.workspace_root` | string (absolute path) | **Yes** | — | Absolute path to workspace. Must exist and be a directory at startup. |
| `ama.bind_host` | string (IP) | No | `"127.0.0.1"` | Bind address. P0 MUST be `127.0.0.1`. |
| `ama.bind_port` | u16 | No | `8787` | Listen port. |
| `ama.log_level` | string | No | `"info"` | One of: `error`, `warn`, `info`, `debug`, `trace`. |
| `ama.log_output` | string | No | `"stderr"` | Log destination: `"stderr"` or `"file:<path>"`. |
| `slime.mode` | string | **Yes** | — | Must be `"embedded"` for P0. |
| `slime.max_capacity` | u64 | **Yes** | — | Global capacity ceiling. Must be > 0. |
| `slime.domains.<domain_key>.enabled` | bool | **Yes** | — | Whether this domain accepts actions. |
| `slime.domains.<domain_key>.max_magnitude_per_action` | u64 | **Yes** | — | Per-action magnitude cap. Must be > 0 and <= `max_capacity`. |

**Domain ID normalization:** TOML keys use underscores (`fs_write_workspace`), which map to dotted domain IDs (`fs.write.workspace`) via automatic `_` → `.` conversion at load time. The canonical form used in code, logs, and `domains.toml` is dotted: `fs.write.workspace`. The underscore form exists only because TOML keys cannot contain dots.

#### Startup Validation Rules

AMA MUST refuse to start (exit non-zero) if ANY of the following conditions are met:
1. Any configuration file (`config.toml`, `domains.toml`, `intents.toml`, `allowlist.toml`) is absent or unparseable.
2. Any `schema_version` field is unrecognized (unknown version -> refuse to start).
3. `workspace_root` does not exist, is not a directory, or is not an absolute path.
4. Any domain referenced in `domains.toml` has no corresponding entry in `config.toml`'s `[slime.domains]`.
5. Any intent in `intents.toml` references a `binary` that does not exist or is not executable.
6. `bind_host` is anything other than `127.0.0.1` in P0.
7. Any internal inconsistency between config files (e.g., action referencing non-existent domain).

On startup failure, AMA MUST log the specific validation error and exit. No partial operation is permitted.

#### Graceful Shutdown

On `SIGTERM` (or equivalent platform signal):
1. Stop accepting new connections immediately.
2. Wait up to 5 seconds for in-flight requests to complete.
3. Send `SIGKILL` to any shell_exec child process groups still alive.
4. Flush audit log buffer.
5. Exit.

On `SIGKILL` or crash: no cleanup guaranteed. Orphan `.ama.<action_id>.tmp` files may remain (cleaned on next startup).

#### Platform Target

P0 targets **Linux** as the primary platform. POSIX-specific APIs (`setpgid`, `execv`, `lstat`, `SIGTERM`, `SIGKILL`) are used directly. macOS is expected to work with minimal changes. **Windows** is NOT a P0 target for the actuator layer — the AMA binary compiles on Windows for development, but shell_exec and POSIX process isolation features are Linux-only.

#### Magnitude Semantics

Magnitude is **agent-declared and AMA-validated**. The agent claims a cost, and AMA enforces bounds:
- Range: `1 <= magnitude <= domain.max_magnitude_per_action`.
- AMA does NOT recompute magnitude from action properties in P0 (e.g., file size does not auto-set magnitude).
- The agent is responsible for declaring reasonable magnitudes. An agent declaring `magnitude: 1` for every action is allowed but will exhaust capacity faster than an agent declaring proportional values.
- P1 MAY introduce AMA-computed magnitude overrides (e.g., `magnitude = max(declared, file_size_kb)`).

### Observability (Read-Only)

`GET /ama/status`:

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "capacity_used": 1547,
  "capacity_max": 10000,
  "capacity_remaining": 8453,
  "uptime_seconds": 3600,
  "domains": {
    "fs.write.workspace": { "enabled": true, "actions_count": 42 },
    "fs.read.workspace": { "enabled": true, "actions_count": 15 },
    "proc.exec.bounded": { "enabled": true, "actions_count": 5 },
    "net.out.http": { "enabled": true, "actions_count": 23 }
  }
}
```

This endpoint is **read-only**. No mutation of state via HTTP. The only way to modify the law is to change `config.toml` and restart.

`/ama/status` exposes only counters, capacity, and uptime. It NEVER exposes full config, allowlists, or intents. `actions_count` per domain is monotonic and resets on restart.

### Integration Pipeline

1. **Map:** `CanonicalAction` -> `(domain_id, magnitude)`.
2. **Check Existence:** Is `domain_id` in `config.toml`? No -> `Impossible`.
3. **Check Enabled:** Is `domain.enabled == true`? No -> `Impossible`.
4. **Check Per-Action Limit:** Is `magnitude <= domain.max_magnitude_per_action`? No -> `Impossible`.
5. **Atomic Reserve:** Call `try_reserve(magnitude)`.
   - Success -> `Authorized`. Proceed to Actuator.
   - Failure -> `Impossible`. Return `403`.

---

## Section 7 — Threat Model & Security Invariants

### 7.1 — Trust Boundaries

AMA assumes a **Single-Host Local Trust Boundary**.

- **Trusted:** AMA binary, embedded AB-S, configuration files (loaded at boot, hashed).
- **Untrusted:** All incoming HTTP requests (regardless of local origin), actuation targets (filesystem, network responses, process outputs).
- **Out of Scope:** Host OS compromise (root attacker), physical access.

All incoming HTTP requests to AMA are untrusted input, regardless of origin (local agents, scripts, or other processes).

### 7.2 — Threats Prevented Structurally

| Threat                    | Prevention Mechanism                                     | Type                        |
|---------------------------|-----------------------------------------------------------|-----------------------------|
| **Shell Injection**       | `execv()` direct + intent mapping. No shell interpreter.  | Structural Impossibility    |
| **Path Traversal**        | `WorkspacePath` newtype. Rejects `..`, absolute paths. Validates every component post-symlink resolution. | Structural Impossibility |
| **Symlink Escape**        | `lstat` on **every path component**. Rejection if symlink. | Structural Impossibility   |
| **Arbitrary Command**     | Closed-set intents (`intents.toml`). Unknown = `422`.     | Structural Impossibility    |
| **SSRF / Internal Net**   | DNS/IP filter: rejects loopback, RFC1918, link-local, metadata. IP re-validated post-connect. DNS rebinding protected. | Active Validation |
| **Redirect Hijack**       | Every redirect re-validated against allowlist. POST redirect rejected. Max 3 hops. | Active Validation |
| **Capacity DoS**          | Atomic CAS counter + rate limit (60 req/min) + concurrency cap (8). | Structural Limit |
| **Action Replay**         | Mandatory `Idempotency-Key` (UUID v4, <=128 bytes). 5-min cache. | Deduplication |
| **Capacity Overflow**     | CAS with `checked_add`. `capacity` NEVER exceeds `max_capacity`. | Structural Impossibility |
| **Unknown Domain**        | Closed World Assumption. Absent domain -> `Impossible`, never error. | Structural Impossibility |
| **Policy Fuzzing**        | `403` returns strictly `{"status":"impossible"}`. Zero leakage. | Opacity |
| **Partial Write**         | Atomic `.ama.<action_id>.tmp` + `rename()`. Crash = no file or old file. | Atomicity                   |
| **Orphan Processes**      | `setpgid` + kill to process group. Best-effort containment. | Containment |
| **Environment Leakage**   | Fresh minimal env. No host variables inherited.           | Isolation                   |
| **TLS Downgrade**         | HTTPS required + certificate validation enforced.        | Enforcement                 |
| **Memory Exhaustion**     | Body max 1 MiB, per-domain payload limits, bounded reading. | Limitation |
| **Output Flooding**       | stdout/stderr 64 KiB, HTTP response 256 KiB, with truncation flag. Limits apply before logging. | Limitation |

### 7.3 — Assumed Limitations (P0 Scope)

| Threat                       | Why Not Covered                                     | Future Mitigation           |
|------------------------------|------------------------------------------------------|-----------------------------|
| **Compromised Host**         | AMA cannot defend against root/kernel attacks.       | P1: seccomp, namespaces.    |
| **Semantic Malice**          | AMA validates **form**, not **content**. Writing valid but malicious content is structurally permitted. This is by design: AMA enforces the *physics* of action, the Agent owns the *logic*. | Out of scope (agent responsibility). |
| **Config Tampering**         | TOML modified before boot -> bad laws loaded.        | P0+: SHA-256 hashes logged at boot. P1: signatures. |
| **Timing Side-Channels**     | Response times may vary by action type.              | P1: constant-time padding.  |
| **Restart Loop**             | Forced restarts reset capacity.                      | Detection via `session_id`. Protection delegated to OS (systemd limits). |
| **File Race Conditions**     | Concurrent writes = last-rename-wins.                | P1: optional file locking.  |
| **Audit Persistence**        | Logs are local. Crash might lose last entries.       | P1: WAL, syslog forward.   |
| **Multi-tenancy**            | Single workspace, single trust domain.               | P1+: per-agent namespaces.  |

### 7.4 — Security Invariants (Normative)

These invariants MUST hold true at all times during AMA execution. Violation is a critical bug.

1. **No Shell Interpretation:** Every process execution uses `execv()` with a pre-validated argument vector. No string concatenation for commands.
2. **Workspace Containment:** No resolved path ever exits `workspace_root`. Verified after canonicalization and symlink resolution on every component.
3. **Capacity Hard Limit:** `capacity` NEVER exceeds `max_capacity`. Guaranteed by hardware atomic CAS.
4. **Closed World:** Any unknown `domain_id` or intent returns `Impossible`. Never `Error`, never `Authorized`.
5. **Zero Leakage:** A `403` verdict reveals nothing about the policy state.
6. **Fail-Closed:** Any unexpected error results in no actuation. Specifically:
   - **Pre-actuation errors** (SLIME/AB-S unavailability, config loading failure): Return `Impossible` (`403`), not `Error`.
   - **Actuator I/O errors** (filesystem inaccessible, process spawn failure, network error during HTTP): Return `503`. These are distinct from policy refusals.
   - In both cases: no actuation occurs.
7. **Static Law:** Configuration files are loaded once at startup and never reloaded at runtime. Change requires restart.
8. **Boot Integrity:** SHA-256 hashes of all loaded configuration files (`config.toml`, `domains.toml`, `intents.toml`, `allowlist.toml`) are computed and logged at startup to establish a cryptographic audit baseline.

---

## Appendix A — File Structure

```
AMA/
├── src/
│   ├── main.rs           # Entry point, server setup
│   ├── config.rs          # TOML loading, validation, hashing
│   ├── server.rs          # HTTP server (axum/actix)
│   ├── schema.rs          # JSON deserialization, structural validation
│   ├── canonical.rs       # CanonicalAction enum, newtypes
│   ├── mapper.rs          # Domain mapping (action -> domain_id)
│   ├── slime.rs           # Embedded AB-S authorizer
│   ├── actuator/
│   │   ├── mod.rs         # Actuator dispatcher
│   │   ├── file.rs        # File read/write actuator
│   │   ├── shell.rs       # Shell exec actuator
│   │   └── http.rs        # HTTP request actuator
│   ├── audit.rs           # Logging, request hashing
│   └── errors.rs          # Error types, HTTP status mapping
├── config/
│   ├── config.toml        # Global config + SLIME domains
│   ├── domains.toml       # Action -> domain_id mapping
│   ├── intents.toml       # Shell intent definitions
│   └── allowlist.toml     # HTTP URL allowlist
├── docs/
│   ├── ARCHITECTURE.md
│   ├── THREAT_MODEL.md
│   └── P0_SCOPE.md
├── examples/
│   ├── file_write.json
│   ├── shell_exec.json
│   └── http_request.json
├── Cargo.toml
└── README.md
```

## Appendix B — Example Requests

### File Write
```json
{
  "adapter": "openclaw",
  "action": "file_write",
  "target": "reports/weekly.md",
  "magnitude": 1,
  "payload": "# Weekly Report\n\nAll systems nominal."
}
```

### Shell Exec (Intent)
```json
{
  "adapter": "generic",
  "action": "shell_exec",
  "target": "list_dir",
  "magnitude": 1,
  "args": ["src"]
}
```

### HTTP GET
```json
{
  "adapter": "langchain",
  "action": "http_request",
  "method": "GET",
  "target": "https://api.weather.com/current",
  "magnitude": 1
}
```

### Dry Run
```json
{
  "adapter": "generic",
  "action": "file_write",
  "target": "test/hello.txt",
  "magnitude": 1,
  "dry_run": true,
  "payload": "test content"
}
```
