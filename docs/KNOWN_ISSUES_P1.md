# AMA P0 — Known Issues for P1

> Findings from final code review (2026-03-13). All are out-of-scope for P0
> (localhost-only, `127.0.0.1`, `max_concurrency: 8`). To be addressed in P1 hardening.

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

## C2. Idempotency Cache — Non-Atomic Check-or-Insert

**File:** `src/idempotency.rs`, `check_or_insert()`

The read (`.get()`) and write (`.insert()`) are separate DashMap operations without a
held shard lock across both. Two concurrent requests with the same idempotency key could
both see `New` and both proceed, defeating idempotency.

**P1 fix:** Use `DashMap::entry()` API which holds the shard lock for the duration,
guaranteeing atomicity of the check-then-insert operation.

---

## C3. Rate Limiter Race Condition

**File:** `src/server.rs`, `check_rate_limit()`

The rate limiter reads `window_start` under a mutex, then separately does `fetch_add` on
the atomic counter. Interleaving between mutex release and atomic increment can cause
double-counting on window reset.

**P1 fix:** Use a single mutex protecting both the window timestamp and counter, or
replace with a proper token bucket. For P0 with 8 max concurrency on localhost, the
practical impact is negligible.

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

*P0 scope: deterministic, local-only, single-agent. P1 = concurrency hardening + cross-platform path safety.*
