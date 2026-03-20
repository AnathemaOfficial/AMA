# SAFA Architecture

> SLIME Adapter for Agents — universal law-adapter membrane for AI agents.

## Overview

SAFA sits between AI agents and real-world actuation surfaces. It translates agent
intentions into canonical actions, validates them through embedded SLIME/AB-S, and
permits actuation only after binary authorization.

```
Agent (OpenClaw, LangChain, etc.)
  │
  │  X-Agent-Id: openclaw
  ▼
┌──────────────────── safa-daemon ─────────────────────┐
│  HTTP transport layer (axum 0.8)                    │
│  - X-Agent-Id resolution (context selector)         │
│  - Per-agent rate limiting                          │
│  - Body size limits, timeouts, admission control    │
│  - Idempotency key validation                       │
│                                                     │
│  ┌──────────────── safa-core ──────────────────┐     │
│  │  Decision law engine (zero HTTP dependency) │     │
│  │                                             │     │
│  │  POST /ama/action                           │     │
│  │    ├─ 1. Validate      magnitude, fields    │     │
│  │    ├─ 2. Canonicalize  → CanonicalAction    │     │
│  │    ├─ 3. Map           → (domain_id, mag)   │     │
│  │    ├─ 4. Authorize     → SLIME/AB-S         │     │
│  │    │     Authorized ──► 5. Actuate          │     │
│  │    │     Impossible ──► 403                 │     │
│  │    └─ 5. Actuate       Execute + audit      │     │
│  └─────────────────────────────────────────────┘     │
│                                                     │
│  GET /ama/health    Liveness                        │
│  GET /ama/version   Version info                    │
│  GET /ama/status    Per-agent capacity + domains    │
└─────────────────────────────────────────────────────┘
  │
  ▼
Real World (filesystem, processes, HTTPS)
```

## Workspace Layout

SAFA is a Cargo workspace with two crates that enforce a strict separation:

- **safa-core** contains the decision law — all validation, canonicalization, mapping,
  authorization, and actuation logic. It has **zero HTTP dependencies**. If you can
  express it as a pure function of `(request, config, authorizer) → result`, it belongs
  in safa-core.

- **safa-daemon** handles HTTP transport only — axum routing, middleware (rate limiting,
  timeouts, body limits, admission control), `X-Agent-Id` header resolution, and
  serialization of `AmaError` into HTTP responses. It depends on safa-core as a library.

```
safa-core/src/
├── lib.rs           Crate root, module declarations
├── config.rs        TOML loading, AgentConfig, SHA-256 boot hashing
├── errors.rs        Error types + http_status_and_body() (no axum)
├── schema.rs        JSON deserialization, field validation
├── canonical.rs     CanonicalAction enum
├── newtypes.rs      WorkspacePath, IntentId, AllowlistedUrl, SafeArg, BoundedBytes
├── mapper.rs        action → domain_id mapping via domains.toml
├── pipeline.rs      Orchestrates validate → map → authorize → actuate
├── slime.rs         SLIME authorizer (P0Authorizer, AgentRegistry)
├── idempotency.rs   UUID-keyed deduplication cache (DashMap)
├── audit.rs         Structured tracing, SHA-256 request hashing
└── actuator/
    ├── mod.rs       Actuator dispatcher
    ├── file.rs      Atomic file write/read (tmp + rename)
    ├── shell.rs     execv direct, setpgid, kill sequence
    └── http.rs      HTTPS-only, allowlist, SSRF protection

safa-daemon/src/
├── main.rs          Entry point, boot integrity, server start
├── lib.rs           Crate root
└── server.rs        axum router, AppState, middleware, X-Agent-Id routing
```

## Configuration

All config is static TOML, loaded once at boot, never reloaded at runtime.

```
config/
├── config.toml      Global settings (bind address, slime mode)
├── domains.toml     action → domain_id mapping + validators
├── intents.toml     Shell intent → binary + args mapping
├── allowlist.toml   HTTPS URL patterns + allowed methods
└── agents/          Per-agent capacity configurations
    ├── default.toml     Default agent (backward compat)
    ├── readonly.toml    Read-only agent example
    ├── developer.toml   High-capacity developer agent example
    └── ci-bot.toml      CI pipeline agent example
```

Boot integrity: SHA-256 of all config files (including agent configs) computed and
logged at startup. Any modification requires a restart.

### Agent Configuration

Each agent gets its own TOML file in `config/agents/`. An agent config defines:
- `agent_id` — unique identifier (used with `X-Agent-Id` header)
- `max_capacity` — monotonic thermodynamic budget (never resets)
- `rate_limit_per_window` / `rate_limit_window_secs` — operational rate limit
- `domains` — which domains are enabled and per-action magnitude limits

See `config/agents/` for concrete examples with different capacity profiles.

## X-Agent-Id Header

`X-Agent-Id` is a **context selector** in P2. It routes the request to the correct
agent's capacity budget and rate limiter.

**Important:** `X-Agent-Id` is NOT authentication. It does not verify identity or
bind a caller to a runtime. Any client that knows a valid agent_id can select it.
In P2, this is acceptable because SAFA runs on `127.0.0.1` (localhost only).

Behavior:
- **Header present + valid** → route to that agent's authorizer
- **Header present + unknown** → `400 Bad Request` (not 401/403)
- **Header absent + single agent** → use default agent (backward compat)
- **Header absent + multiple agents** → `400 Bad Request`

A future phase (P3) may introduce identity binding where `X-Agent-Id` is verified
against a capability token or Machine-Suit admission credential.

## Capacity Model

SAFA enforces two independent layers of capacity control. See
[`docs/CAPACITY_MODEL.md`](CAPACITY_MODEL.md) for full details.

| Layer | Mechanism | Scope | Resets? |
|-------|-----------|-------|---------|
| **Layer 1: Monotonic Capacity** | AtomicU64 CAS loop | Per-agent | Never (process restart) |
| **Layer 2: Operational Rate Limits** | Mutex<RateLimitState> | Per-agent | Per time window |

## Key Design Decisions

### Newtypes with Private Constructors

`WorkspacePath`, `IntentId`, `AllowlistedUrl`, `SafeArg`, `BoundedBytes` — all enforce
invariants at construction time. If you have one, it's valid. The compiler prevents
constructing invalid instances.

### Thermodynamic Capacity (SYF-Shield)

Capacity is entropy: it only increases, never decreases within a session. Implemented
via `AtomicU64` compare-and-swap loop. Hardware guarantee: `capacity` never exceeds
`max_capacity`. Reset requires process restart (Thermodynamic Cooling). Each agent
has its own independent capacity counter.

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

The idempotency cache is **global** (not per-agent). A UUID used by one agent cannot
be reused by another.

## Concurrency Model (P1/P2)

| Layer | Mechanism | Scope | Purpose |
|-------|-----------|-------|---------|
| Admission | `concurrency_limit(8)` (Tower) | Global | Bounded concurrent requests |
| Rate limit | `Mutex<RateLimitState>` | Per-agent | Operational burst protection |
| Capacity | `AtomicU64` CAS loop | Per-agent | Thermodynamic budget |
| Idempotency | `DashMap::entry()` | Global | Atomic deduplication |
| Timeout | `TimeoutLayer(30s)` + per-action limits | Global | Bounded execution lifecycle |

## Phases

| Phase | Status | Scope |
|-------|--------|-------|
| P0 | HELD (v0.1.0-p0-held) | Single-agent local baseline |
| P1 | HELD (v0.1.0-p1-held) | Concurrent multi-agent hardening |
| P2 | HELD (v0.2.0-p2-held) | Multi-agent capacity system, workspace split |
| P3 | Planned | Identity binding, capability manifests, per-agent workspaces |

## References

- [P0 Design Specification](superpowers/specs/2026-03-13-ama-p0-design.md)
- [Capacity Model](CAPACITY_MODEL.md)
- [P1 HELD Summary](p1/SAFA_P1_HELD.md)
- [Known Issues](KNOWN_ISSUES_P1.md)
- [Concurrency Model](p1/SAFA_CONCURRENCY_MODEL.md)
- [Idempotency State Machine](p1/SAFA_IDEMPOTENCY_STATE_MACHINE.md)
