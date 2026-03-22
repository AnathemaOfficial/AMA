# SAFA P3 — Gap Analysis

> **Status: HISTORICAL** — This document was the pre-implementation gap
> analysis for P3. P3 is now **HELD** (v0.3.0-p3-held). All gaps
> identified below have been resolved.

> Baseline: `master` at P2 HELD (v0.2.0-p2-held)
> Resolved: P3 HELD (v0.3.0-p3-held)
> Date: 2026-03-20

---

## 1. What Is Already Held (P0–P2)

| Component | Phase | Status | Notes |
|-----------|-------|--------|-------|
| Pipeline: validate → map → auth → actuate | P0 | HELD | Complete, tested |
| Binary verdict: `Authorized` / `Impossible` | P0 | HELD | `SlimeVerdict` enum |
| Newtypes with private constructors | P0 | HELD | WorkspacePath, SafeArg, etc. |
| Actuators: file, shell, HTTP | P0 | HELD | With sandboxing |
| Audit logging + request hashing | P0 | HELD | SHA-256 per request |
| Idempotency dedup (DashMap CAS) | P1 | HELD | C2 resolved |
| Rate limiter (Mutex window) | P1 | HELD | C3 resolved |
| Bounded admission + timeouts | P1 | HELD | Per-action configurable |
| Multi-agent routing (`X-Agent-Id`) | P2 | HELD | `AgentRegistry` |
| Per-agent capacity budgets | P2 | HELD | Monotonic thermodynamic |
| Per-agent rate limits | P2 | HELD | Independent windows |
| Per-agent domain policies | P2 | HELD | Enable/disable + magnitude |
| All tests pass, clippy clean (`-D warnings`) | P2 | HELD | 20 test files |

---

## 2. What P3 Must Add (Delta)

### 2.1 Identity Binding

**Current state:**
- `X-Agent-Id` is a plain-text header with no verification.
- Any HTTP client can claim any agent identity.
- Acceptable in P2 (localhost only, documented in ARCHITECTURE.md).

**Gap:**
- No secret, token, or signature ties a request to a registered agent.
- Agent impersonation is trivially possible over any network.

**Required changes:**

| File | Change |
|------|--------|
| `config/agents/*.toml` | Add `secret` field (HMAC shared secret hash) |
| `safa-core/src/config.rs` | Parse `secret` from agent config |
| `safa-daemon/src/server.rs` | Add middleware: verify HMAC signature (`X-Agent-Signature`) against `agent_id + timestamp + request_body_hash` |
| `safa-daemon/src/server.rs` | Validate `X-Agent-Timestamp` within ±300s window (replay prevention) |
| `safa-daemon/tests/p3_identity.rs` | New: identity verification + replay prevention tests |

---

### 2.2 Capability Manifest & Proof-of-Constraint

**Current state:**
- Agent capabilities are defined in TOML files.
- Capabilities are loaded at boot but not hashed or versioned.
- No external endpoint to query an agent's constraints.
- `ActionResponse` does not indicate which rules were applied.

**Gap:**
- No tamper-evident representation of agent constraints.
- No way for external observer to verify what rules are active.
- No proof trail linking a verdict to a specific rule set.

**Required changes:**

| File | Change |
|------|--------|
| `safa-core/src/slime.rs` | Compute `manifest_hash` per agent at registration |
| `safa-core/src/schema.rs` | Add `manifest_hash` field to `ActionResponse`, add `PublicManifest` type (never exposes secret) |
| `safa-daemon/src/server.rs` | Add `X-Safa-Policy-Hash` response header |
| `safa-daemon/src/server.rs` | New route: `GET /ama/manifest/{agent_id}` |
| `safa-daemon/src/server.rs` | New route: `GET /ama/proof/{request_id}` |
| `safa-core/src/audit.rs` | Store verdict + manifest_hash per request_id |
| `safa-daemon/tests/p3_proof.rs` | New: proof-of-constraint tests |

---

### 2.3 Per-Agent Workspace Isolation

**Current state:**
- `WorkspacePath` validates relative paths against a single global
  `workspace_root`.
- All agents share the same filesystem namespace.
- C1 (TOCTOU/symlink on Windows) is OPEN — `verify_no_symlinks` is a
  no-op on Windows.

**Gap:**
- Agent "ci-bot" can read files written by agent "developer".
- No filesystem boundary between agents.
- Windows symlink/junction attack vector is unmitigated.

**Required changes:**

| File | Change |
|------|--------|
| `safa-core/src/newtypes.rs` | `WorkspacePath::new()` takes `agent_id`, enforces `workspace_root/{agent_id}/` prefix |
| `safa-core/src/newtypes.rs` | Add `fs::canonicalize()` after join, verify prefix (fixes C1) |
| `safa-core/src/newtypes.rs` | Add Windows symlink/junction detection |
| `safa-core/src/pipeline.rs` | Pass `agent_id` to `WorkspacePath` construction |
| `safa-core/src/actuator/file.rs` | Update `verify_no_symlinks` for Windows |
| `safa-core/tests/p3_workspace.rs` | New: workspace isolation tests |

---

## 3. Open Issues Assessment

### Blocking for P3

| ID | Issue | Why Blocking | Resolution |
|----|-------|-------------|------------|
| **C1** | WorkspacePath TOCTOU/symlink on Windows | Required for Pillar 3 (workspace isolation needs canonicalization) | Fix as part of WorkspacePath refactor |

### Non-Blocking (can remain open)

| ID | Issue | Why Non-Blocking |
|----|-------|-----------------|
| I1 | Capacity never released on failure | By design (spec Section 6) |
| I2 | Allowlist doesn't validate HTTP method | Correctness issue, not P3 scope |
| I3 | Shell actuator single read() | Reliability, not security |
| I4 | HTTP body fully buffered | Performance, not security |
| I5 | Test helper only configures file_write | Test coverage, fixable incrementally |
| I6 | Duplicate DomainPolicy type | Code hygiene, not blocking |

---

## 4. Documentation Updates Required

| Document | Action |
|----------|--------|
| `ARCHITECTURE.md` | Update P3 row: Planned → HELD, add P3 scope summary |
| `KNOWN_ISSUES_P1.md` | Mark C1 as RESOLVED after P3 workspace work |
| `THREAT_MODEL.md` | Add identity binding to "threats prevented" section |
| `README.md` | Update status to P3 HELD, add Proof-of-Constraint section |
| `CAPACITY_MODEL.md` | No changes needed (model unchanged in P3) |

---

## 5. What Is NOT in P3 (Confirmed Out of Scope)

| Item | Rationale | Target |
|------|-----------|--------|
| New action domains (email, calendar) | Product binding, not structural | P4 |
| Claw-friendly aliases | Product naming | P4 |
| Consumer agent profiles | Product config | P4 |
| Endpoint rename /ama/ → /safa/ | Non-breaking cosmetic | P4 |
| Session tracking | Requires cumulative context design | P5 |
| Policy Editor | Requires UX/DSL design | P5+ |
| Mobile app / Lobster / Snapy Clapy | Separate product entirely | N/A |

---

## 6. Dependency Graph

```
Pillar 3 (Workspace Isolation)
  └── requires C1 fix (symlink/TOCTOU)
      └── requires canonicalize() + Windows junction detection

Pillar 2 (Capability Manifest)
  └── requires audit.rs storage for proof endpoint
  └── independent of Pillars 1 and 3

Pillar 1 (Identity Binding)
  └── requires config.rs update for secret_hash
  └── independent of Pillars 2 and 3
```

**Recommended execution order:**
1. Pillar 1 (Identity) — smallest, unblocks secure testing
2. Pillar 2 (Manifest) — independent, high visibility
3. Pillar 3 (Workspace) — largest, resolves C1, depends on nothing

---

## 7. Risk Assessment

| Risk | Mitigation |
|------|-----------|
| C1 fix is complex on Windows | Test on Windows CI; use `std::fs::canonicalize` + junction detection via `winapi` or `std::os::windows` |
| Manifest hash non-deterministic (TOML key ordering) | Use canonical serialization (sorted keys) before hashing |
| Proof endpoint stores unbounded data | TTL-based cleanup, same pattern as idempotency cache |
| Identity binding adds latency | HMAC is O(1), negligible vs network round-trip |
| Scope creep into product features | This document is the canonical boundary |
