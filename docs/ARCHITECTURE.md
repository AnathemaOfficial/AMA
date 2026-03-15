# AMA Architecture

> Agent Machine Armor — universal law-adapter membrane for AI agents.

## Overview

AMA sits between AI agents and real-world actuation surfaces. It translates agent
intentions into canonical actions, validates them through embedded SLIME/AB-S, and
permits actuation only after binary authorization.

```
Agent (OpenClaw, LangChain, etc.)
  │
  ▼
┌─────────────────────────────────────────────┐
│  AMA  (127.0.0.1:8787)                     │
│                                             │
│  POST /ama/action                           │
│    │                                        │
│    ├─ 1. Ingress         Rate limit, body   │
│    │                     limit, idempotency  │
│    ├─ 2. Schema          JSON validation     │
│    ├─ 3. Canonicalize    → CanonicalAction   │
│    ├─ 4. Map             → (domain_id, mag)  │
│    ├─ 5. Authorize       → SLIME/AB-S        │
│    │       Authorized ──► 6. Actuate         │
│    │       Impossible ──► 403                │
│    └─ 6. Actuate         Execute + audit     │
│                                             │
│  GET /ama/health    Liveness                │
│  GET /ama/version   Version info            │
│  GET /ama/status    Capacity + domain stats │
└─────────────────────────────────────────────┘
  │
  ▼
Real World (filesystem, processes, HTTPS)
```

## Source Layout

```
src/
├── main.rs          Entry point, boot integrity, server start
├── lib.rs           Crate root, module declarations
├── config.rs        TOML loading, SHA-256 boot hashing
├── server.rs        axum router, middleware (rate limit, timeout, admission)
├── schema.rs        JSON deserialization, field validation
├── canonical.rs     CanonicalAction enum
├── newtypes.rs      WorkspacePath, IntentId, AllowlistedUrl, SafeArg, BoundedBytes
├── mapper.rs        action → domain_id mapping via domains.toml
├── pipeline.rs      Orchestrates validate → map → authorize → actuate
├── slime.rs         Embedded AB-S authorizer (AtomicU64 CAS)
├── idempotency.rs   UUID-keyed deduplication cache (DashMap)
├── audit.rs         Structured tracing, SHA-256 request hashing
├── errors.rs        Error types → HTTP status mapping
└── actuator/
    ├── mod.rs       Actuator dispatcher
    ├── file.rs      Atomic file write/read (tmp + rename)
    ├── shell.rs     execv direct, setpgid, kill sequence
    └── http.rs      HTTPS-only, allowlist, SSRF protection
```

## Configuration

All config is static TOML, loaded once at boot, never reloaded at runtime.

```
config/
├── config.toml      Global settings + SLIME capacity + domain policies
├── domains.toml     action → domain_id mapping + validators
├── intents.toml     Shell intent → binary + args mapping
└── allowlist.toml   HTTPS URL patterns + allowed methods
```

Boot integrity: SHA-256 of all four files computed and logged at startup.

## Key Design Decisions

### Newtypes with Private Constructors

`WorkspacePath`, `IntentId`, `AllowlistedUrl`, `SafeArg`, `BoundedBytes` — all enforce
invariants at construction time. If you have one, it's valid. The compiler prevents
constructing invalid instances.

### Thermodynamic Capacity (SYF-Shield)

Capacity is entropy: it only increases, never decreases within a session. Implemented
via `AtomicU64` compare-and-swap loop. Hardware guarantee: `capacity` never exceeds
`max_capacity`. Reset requires process restart (Thermodynamic Cooling).

### Closed World Assumption

Unknown domain IDs → `Impossible` (not error). Unknown intents → `422`. The action
universe is finite, bounded, and enumerable at boot.

### Fail-Closed

Any unexpected error = no actuation. Pre-actuation errors → `403`. Actuator I/O
errors → `503`. In both cases: nothing happens.

### Idempotency

Every `POST /ama/action` requires a UUID v4 `Idempotency-Key` header. Duplicate
within 5-minute window returns cached response. In-flight duplicate → `409 Conflict`.
Cache uses `DashMap::entry()` for atomic check-or-insert (P1 fix).

## Concurrency Model (P1)

| Layer | Mechanism | Purpose |
|-------|-----------|---------|
| Admission | `concurrency_limit(8)` (Tower) | Bounded concurrent requests |
| Rate limit | `Mutex<RateLimitState>` (60 req/min) | Burst protection |
| Capacity | `AtomicU64` CAS loop | Race-safe thermodynamic budget |
| Idempotency | `DashMap::entry()` | Atomic deduplication |
| Timeout | `TimeoutLayer(30s)` + per-action limits | Bounded execution lifecycle |

## Phases

| Phase | Status | Scope |
|-------|--------|-------|
| P0 | HELD (v0.1.0-p0-held) | Single-agent local baseline |
| P1 | HELD (v0.1.0-p1-held) | Multi-agent concurrency hardening |
| P2 | Planned | Cross-platform path safety, OpenClaw adapter, multi-adapter testing |

## References

- [P0 Design Specification](superpowers/specs/2026-03-13-ama-p0-design.md)
- [P1 HELD Summary](p1/AMA_P1_HELD.md)
- [Known Issues](KNOWN_ISSUES_P1.md)
- [Concurrency Model](p1/AMA_CONCURRENCY_MODEL.md)
- [Idempotency State Machine](p1/AMA_IDEMPOTENCY_STATE_MACHINE.md)
