# AMA Timeout Model

## Bounded Execution and Terminal Outcome Semantics

### Status: P1 Implemented
### Date: 2026-03-14
### Authors: GPT-4 (architect) + Claude Code (implementer) + Fireplank (director)

---

## 1. Purpose

This document defines AMA's timeout model and bounded completion
guarantees.

Every admitted action must have a bounded lifecycle. No request may
hang indefinitely. All terminal outcomes, including timeouts, must be
committed as replayable results (Model A).

---

## 2. Timeout Layers

AMA enforces timeouts at three levels:

### 2.1 Per-Action Timeout (Pipeline Layer)

Each action type has a hardcoded execution deadline:

| Action | Timeout | Enforcement |
|--------|---------|-------------|
| `file_write` | 5 seconds | `tokio::time::timeout()` |
| `file_read` | 5 seconds | `tokio::time::timeout()` |
| `shell_exec` | 15 seconds | Passed to actuator + SIGTERM/SIGKILL |
| `http_request` | 15 seconds | `tokio::time::timeout()` + reqwest timeout |

These timeouts protect against slow or hanging actuations.

### 2.2 Request-Level Timeout (Router Layer)

A global 30-second `TimeoutLayer` wraps the entire HTTP handler.

This protects against hangs in pre-actuation stages (validation,
canonicalization, authorization) that are not covered by per-action
timeouts.

If the 30-second deadline fires, the client receives:

```json
{
    "status": "error",
    "error_class": "timeout",
    "message": "request exceeded 30s global deadline"
}
```

### 2.3 HTTP Client Timeout (Actuator Layer)

The HTTP actuator configures `reqwest::Client` with:

- `connect_timeout`: 5 seconds
- `timeout`: 15 seconds (total request)

This provides defense-in-depth for outbound HTTP.

---

## 3. Shell Process Lifecycle

The shell actuator enforces bounded completion with a kill sequence:

```
1. Process spawned with setpgid() (Unix: process group isolation)
2. tokio::time::timeout(15s, child.wait())
3. On timeout:
   a. SIGTERM to entire process group
   b. Sleep 2 seconds (grace period)
   c. SIGKILL to entire process group
   d. child.wait() to reap zombie
4. Returns ShellExecResult { exit_code: -1, stderr: "process killed: timeout exceeded" }
```

Maximum wall-clock time: 17 seconds (15s timeout + 2s SIGKILL wait).

---

## 4. Model A: Terminal Outcome Commitment

### P1 Fix: `complete()` Replaces `remove()`

In P0, pipeline errors caused the idempotency key to be removed:

```rust
// P0 bug (server.rs)
Err(e) => {
    state.idempotency_cache.remove(&idem_key);  // WRONG
    e.into_response()
}
```

This violated Model A because:

1. Key returns to ABSENT after error
2. Retry with same key re-executes instead of replaying
3. Duplicate execution is possible on retry after timeout/denial

### P1 Fix

All terminal outcomes are now committed via `complete()`:

```rust
// P1 fix
Err(e) => {
    let cached_json = /* serialize error */;
    state.idempotency_cache.complete(idem_key, cached_json);
    e.into_response()
}
```

This applies to:
- Pipeline errors (capacity denial, validation failure)
- Deserialization errors (invalid JSON)
- Timeout results
- Any other terminal outcome

### Replay Behavior

After a terminal error is committed:

| Retry Scenario | P0 Behavior | P1 Behavior |
|----------------|-------------|-------------|
| Retry after capacity denial | Re-executes (remove) | Replays denial (complete) |
| Retry after bad JSON | Re-parses (remove) | Replays error (complete) |
| Retry after timeout | Re-executes (remove) | Replays timeout (complete) |
| Retry after success | Replays ✅ | Replays ✅ (unchanged) |

---

## 5. Invariants

**T1. Bounded execution**

Every admitted action completes within its per-action timeout.

**T2. Bounded request lifecycle**

Every HTTP request completes within the 30-second global deadline.

**T3. Terminal commitment**

All terminal outcomes are committed to DONE, not removed.
Retry with the same key replays the committed result.

**T4. Process cleanup**

Shell processes are terminated (SIGTERM + SIGKILL) on timeout.
No zombie processes remain.

---

## 6. Known Limitations (P1)

- Timeouts are hardcoded, not configurable via TOML
- DNS resolution in HTTP actuator has no explicit timeout
- The 30s global timeout is applied by `TimeoutLayer` outside the
  handler, so the idempotency key may remain IN_FLIGHT until TTL
  expiry (5 minutes) if the global timeout fires before the handler
  can call `complete()`
- Per-capability timeout overrides are not yet supported

These are acceptable for P1 (localhost, single-node) and may be
addressed in future phases.

---

## 7. Test Coverage

| Test | File | Validates |
|------|------|-----------|
| `test_error_commits_to_done_not_remove` | `tests/p1_timeouts.rs` | Model A: denial → complete, not remove |
| `test_bad_json_commits_to_done_not_remove` | `tests/p1_timeouts.rs` | Model A: bad JSON → complete, not remove |
| `test_success_replay_returns_identical_result` | `tests/p1_timeouts.rs` | Success replay baseline |
| `test_inflight_returns_conflict` | `tests/p1_timeouts.rs` | Policy A: 409 on duplicate |
| `kills_on_timeout` | `tests/test_actuator_shell.rs` | Shell SIGTERM/SIGKILL cleanup |
