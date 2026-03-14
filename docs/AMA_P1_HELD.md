# AMA P1 — HELD

> **Status**: P1-HELD — Canonical Local Multi-Agent Baseline
> **Date**: 2026-03-14
> **Tag**: `v0.1.0-p1-held`
> **Authors**: GPT-4 (architect) + Claude Code (implementer) + Fireplank (director)

---

## Scope

P1 hardens AMA for **concurrent multi-agent use on a single localhost node**.

This is not distributed. This is not multi-node. This is the local membrane
operating correctly under concurrent load from multiple agents hitting
`127.0.0.1:8787` simultaneously.

---

## What P1 Delivered

### WS1 — Idempotency State Machine (C2 resolved)

- Replaced racy check-then-insert with `DashMap::entry()` atomic API
- Fixed potential deadlock (`len()`/`retain()` moved outside entry lock)
- Established Model A: all terminal outcomes committed to DONE via `complete()`
- Established Policy A: duplicate during IN_FLIGHT returns 409 Conflict
- 8 tests including 10-thread barrier race (bug reproduced before fix)

### WS2 — Atomic Capacity Reservation (already correct)

- AtomicU64 `compare_exchange_weak()` CAS loop was already race-safe in P0
- Documented in `AMA_CONCURRENCY_MODEL.md`
- 100-thread concurrent test confirms no overspend

### WS3 — Bounded Admission (Tower concurrency_limit)

- `concurrency_limit(8)` provides bounded concurrent execution via Tower FIFO semaphore
- Not a custom application queue — bounded concurrency / bounded admission
- 3 tests covering saturation, duplicate prevention, overflow determinism

### WS4 — Race-Safe Rate Limiting (C3 resolved)

- Replaced split `Mutex<Instant>` + `AtomicU64` with unified `Mutex<RateLimitState>`
- Window reset and counter increment now atomic under same lock
- 3 tests including 80-request burst flood

### WS5 — Execution Timeouts and Bounded Completion

- Added `TimeoutLayer(30s)` + `HandleErrorLayer` to router
- Fixed Model A violation: `remove()` replaced with `complete()` in all error paths
- Per-action timeouts (5s file, 15s shell/http) preserved from P0
- 4 tests covering Model A compliance at HTTP level

### Section 9 — Cross-Cutting Adversarial Tests

- 8 composition tests validating invariant interactions across workstreams
- Covers: queue pressure + replay, capacity racing, rate-limit flood + duplicates,
  denial replay budget neutrality, rapid-fire mixed workload
- Multi-adapter deferred (I5: test helper single-domain limitation)

---

## Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Unit tests (lib) | 11 | PASS |
| P0 integration | 6 | PASS |
| P0 SLIME | 8 | PASS |
| P0 server | 10 | PASS |
| P0 idempotency | 6 | PASS |
| P0 pipeline | 5 | PASS |
| P0 actuators (file, http, shell) | 12 | PASS |
| P1 idempotency | 8 | PASS |
| P1 timeouts | 4 | PASS |
| P1 rate limit | 3 | PASS |
| P1 queue | 3 | PASS |
| P1 adversarial | 8 | PASS |
| **Total** | **94** | **ALL PASS** |

Clippy: clean (`-D warnings`)

---

## Known Issues Remaining

| ID | Issue | Severity | Status |
|----|-------|----------|--------|
| C1 | WorkspacePath TOCTOU / symlink race (Windows) | Medium | Open — deferred to P2 |
| I1 | Capacity never released on actuation failure | Low | By design (spec §6) |
| I2 | Allowlist doesn't validate HTTP method | Low | Deferred |
| I3 | Shell actuator single read() may miss output | Low | Deferred |
| I4 | HTTP response body fully buffered | Low | Deferred |
| I5 | Test helper only configures file_write domain | Low | Deferred |
| I6 | Duplicate DomainPolicy type | Low | Deferred |

None of these affect the P1 held condition.

---

## Held Conditions (all met)

- [x] Duplicate request cannot double-execute (WS1: atomic entry)
- [x] Budget cannot overspend under race (WS2: CAS loop)
- [x] Admission is bounded and fail-closed (WS3: concurrency_limit)
- [x] Execution has bounded lifecycle (WS5: TimeoutLayer + Model A)
- [x] Rate limiting cannot be bypassed under burst (WS4: unified mutex)
- [x] Cross-cutting composition preserves all invariants (Section 9: 8 tests)

---

## Architecture Unchanged

P1 added no new modules, no new dependencies, no new API surface.

Changes were surgical:
- `idempotency.rs`: ~40 lines changed (entry API + deadlock fix)
- `server.rs`: ~30 lines changed (rate limiter + Model A + TimeoutLayer)
- 0 new crates added
- 0 API endpoints changed
- 0 config format changes

---

## What P1 Did NOT Do

- No distributed routing
- No capability discovery across nodes
- No remote node admission
- No Machine-Suit integration
- No LLM explanation layer
- No policy engine
- No orchestration UI
- No multi-adapter test coverage (deferred: I5)

---

## Next Phase: P2 Candidates

1. **C1 resolution**: Cross-platform path safety (canonicalize + symlink detection)
2. **Multi-adapter testing**: Configure all 4 domains in test helper (I5)
3. **OpenClaw adapter**: Ana → AMA integration
4. **Per-adapter rate limiting**: Replace global limiter with per-domain limits
5. **Allowlist method validation**: Check HTTP methods against allowlist (I2)
6. **Actuator hardening**: Shell read_to_end (I3), HTTP chunked reading (I4)

---

*"The membrane is sealed. What enters is admitted. What is admitted is bounded.
What is bounded completes. What completes is recorded. What is recorded replays."*
