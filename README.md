# SAFA — SLIME Adapter for Agents

> An agent adapter over the SLIME law-layer.

**SAFA** is the agent-facing adaptation layer that connects autonomous agents to a constrained actuation path enforced by SLIME. It translates agent intentions into canonical SLIME domains and permits real-world actuation only after binary authorization.

SAFA is **not** an agent — it is an adapter, proxy, translator, and minimal executor.

> *This project was initially developed under the working name **AMA** (Agent Machine Armor). The rename to SAFA reflects the architecture more accurately.*

```
Agent → SAFA → SLIME/AB-S → Real world actuation
```

## Status

**SAFA P3 — HELD** (Agent Containment)

- **P0** validated the full pipeline: **validate → map → authorize → actuate**
- **P1** hardened for concurrent use: idempotency races (C2), rate limiter races (C3), bounded admission, execution timeouts
- **P2** introduced multi-agent capacity: workspace split, per-agent budgets, `X-Agent-Id` routing, per-agent rate limiters
- **P3** sealed agent containment: HMAC identity binding, capability manifests with Proof-of-Constraint, per-agent workspace isolation with symlink detection (C1 fix)
- **100+ tests**, clippy clean (`-D warnings`)
- Tags: `v0.1.0-p0-held`, `v0.1.0-p1-held`, `v0.2.0-p2-held`, `v0.3.0-p3-held`

Known issues documented in [`docs/KNOWN_ISSUES_P1.md`](docs/KNOWN_ISSUES_P1.md).

## Architecture

SAFA is a Cargo workspace with two crates:

| Crate | Role | HTTP dependency |
|-------|------|-----------------|
| **safa-core** | Decision law engine (validate, map, authorize, actuate) | None |
| **safa-daemon** | HTTP transport wrapper (axum, rate limiting, routing) | axum 0.8 |

```
Agent (OpenClaw, LangChain, etc.)
  │
  │  POST /ama/action
  │  X-Agent-Id: openclaw
  │  X-Agent-Timestamp: <epoch>      ← P3: HMAC identity
  │  X-Agent-Signature: <hmac-hex>   ← P3: HMAC identity
  │  Idempotency-Key: <uuid>
  │
  ▼
┌──────────────────── safa-daemon ─────────────────────┐
│  0. resolve_agent_id()                              │
│  0.5 P3: verify_identity() — HMAC-SHA256 check      │
│  1. per-agent rate limit check                      │
│                                                     │
│  ┌──────────────── safa-core ──────────────────┐     │
│  │  2. Validate     magnitude, field exclusivity│    │
│  │  3. Canonicalize  → CanonicalAction (newtypes)│   │
│  │     P3: per-agent workspace isolation         │   │
│  │     P3: canonicalize() symlink detection      │   │
│  │  4. Map           → domain_id (domains.toml) │   │
│  │  5. Authorize     → SLIME (per-agent budget) │   │
│  │  6. Actuate       → file/shell/HTTP          │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
│  Response: X-Safa-Policy-Hash (Proof-of-Constraint) │
│  Audit: structured tracing + SHA-256 request hash   │
└─────────────────────────────────────────────────────┘
```

**Key properties:**
- **Closed world** — unknown domain_ids → Impossible
- **Fail-closed** — any error = no actuation
- **Finite action universe** — bounded, enumerable transition space
- **Correctness by construction** — Rust newtypes with private constructors
- **Thermodynamic capacity** — monotonic entropy via AtomicU64 CAS (per-agent)
- **Idempotent** — UUID v4 deduplication, atomic DashMap entry API (global)

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for full details.

## Stack

Rust 1.93 · axum 0.8 · tokio · serde · dashmap · sha2 · reqwest · tracing

## Running

```bash
cargo build --workspace --release
./target/release/safa-daemon
# Listens on 127.0.0.1:8787
```

## Endpoints

> **Note:** Endpoints currently use the `/ama/` prefix for backward compatibility.
> A future release will migrate to `/safa/` with dual-support transition.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Liveness check |
| GET | `/version` | Version info |
| GET | `/ama/status` | Per-agent capacity + domain stats |
| GET | `/ama/manifest/{agent_id}` | P3: Agent capability manifest + policy hash |
| POST | `/ama/action` | Execute action (returns `X-Safa-Policy-Hash` header) |

## Quick Example

```bash
# Single-agent mode (default agent, no X-Agent-Id needed)
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{"adapter":"generic","action":"file_write","target":"hello.txt","magnitude":1,"payload":"hello world"}'

# Multi-agent mode (specify agent)
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -H "X-Agent-Id: developer" \
  -d '{"adapter":"generic","action":"file_write","target":"hello.txt","magnitude":1,"payload":"hello world"}'
```

See [`examples/`](examples/) for more.

## Tests

```bash
cargo test --workspace --features test-utils    # 100+ tests
cargo clippy --workspace --features test-utils -- -D warnings
```

## Documentation

| Document | Description |
|----------|-------------|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | System architecture and design decisions |
| [`docs/CAPACITY_MODEL.md`](docs/CAPACITY_MODEL.md) | Two-layer capacity model (monotonic + rate limits) |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Threat model and security invariants |
| [`docs/KNOWN_ISSUES_P1.md`](docs/KNOWN_ISSUES_P1.md) | Known issues and resolution status |
| [`docs/p1/`](docs/p1/) | P1 workstream documentation |

## License

Proprietary — SYF Corp.
