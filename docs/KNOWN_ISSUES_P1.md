# SAFA — Known Issues

> Updated 2026-03-14 after P1 completion.
> C2 and C3 were resolved in P1. C1 and I1–I6 remain open for P2+.

---

## C1. WorkspacePath TOCTOU / Symlink Race on Windows

**File:** `src/newtypes.rs`

`WorkspacePath` performs lexical validation (rejects `..` segments) but never calls
`std::fs::canonicalize()` to verify the resolved path remains under `workspace_root`.

- On **Unix**, `verify_no_symlinks` in `actuator/file.rs` compensates partially via
  per-component `lstat`, but a TOCTOU gap exists between verification and actuation.
- On **Windows**, `verify_no_symlinks` is a no-op — junctions/symlinks are not checked.

**P1 fix:** After joining, canonicalize the parent (if it exists) and verify the result
starts with the canonicalized `workspace_root`. Add Windows symlink/junction detection.

---

## ~~C2. Idempotency Cache — Non-Atomic Check-or-Insert~~ ✅ RESOLVED

**File:** `src/idempotency.rs`, `check_or_insert()`

**Status:** Fixed in P1 WS1 (2026-03-14)

The read (`.get()`) and write (`.insert()`) were separate DashMap operations without a
held shard lock across both. Two concurrent requests with the same idempotency key could
both see `New` and both proceed, defeating idempotency.

**Fix applied:** Replaced check-then-insert with `DashMap::entry()` API which holds the
shard lock for the duration, guaranteeing atomicity of the ABSENT → IN_FLIGHT transition.
Also fixed a potential deadlock where `len()`/`retain()` were called while holding an
entry lock (DashMap shard locks are not reentrant).

**Validation:** 8 P1 tests in `tests/p1_idempotency.rs` including concurrent race tests
with 10 threads + barrier. Bug was reproduced (2 threads won ownership) before fix, then
confirmed resolved. 76/76 tests pass, clippy clean.

---

## ~~C3. Rate Limiter Race Condition~~ ✅ RESOLVED

**File:** `src/server.rs`, `check_rate_limit()`

**Status:** Fixed in P1 WS4 (2026-03-14)

The rate limiter used a `Mutex<Instant>` for window_start and a separate `AtomicU64` for
the counter. The gap between mutex release and `fetch_add` allowed interleaving where
counter increments could straddle a window reset.

**Fix applied:** Replaced separate `Mutex<Instant>` + `AtomicU64` with a single
`Mutex<RateLimitState>` containing both `window_start` and `count`. Window reset and
counter increment now happen atomically under the same lock.

**Validation:** 3 rate limiter tests in `tests/p1_rate_limit.rs`. Sequential tests confirm
limit enforcement. Concurrent race is architecturally eliminated (single mutex). 83/83
tests pass, clippy clean.

---

## Additional Findings (Important, not Critical)

| ID  | Issue | File | Notes |
|-----|-------|------|-------|
| I1  | Capacity never released on actuation failure | `pipeline.rs` | By design (spec Section 6) — add failed actuation counter to `/ama/status` |
| I2  | Allowlist doesn't validate HTTP method | `newtypes.rs` | `methods` field loaded but never checked |
| I3  | Shell actuator single `read()` may miss output | `actuator/shell.rs` | Use `read_to_end` with `Take` adapter |
| I4  | HTTP response body fully buffered before truncation | `actuator/http.rs` | Use chunked reading with size limit |
| I5  | Test helper only configures `file_write` domain | `server.rs` | Add all 4 domains for broader integration tests |
| I6  | Duplicate `DomainPolicy` type in `slime.rs` and `config.rs` | Both | Extract shared type |

---

*P0: deterministic, local-only, single-agent. P1: concurrency hardening (HELD 2026-03-14). P2: cross-platform path safety + multi-adapter + OpenClaw.*
