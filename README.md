
<img width="1199" height="349" alt="SAFA" src="https://github.com/user-attachments/assets/7e8d87a7-8695-4057-a019-3562f047a119" />

### SAFA — SLIME Adapter for Agents

> Formerly **AMA** (Agent Machine Armor). Universal law-adapter membrane for AI agents.

SAFA translates agent intentions into canonical SLIME domains and permits real-world
actuation only after binary authorization. It is **not** an agent — it is an adapter,
proxy, translator, and minimal executor.

```
Agent → SAFA → SLIME/AB-S → Real world actuation
```

## Status

**SAFA P2 — HELD** (Multi-Agent Capacity System)

- **P0** validated the full pipeline: **validate → map → authorize → actuate**
- **P1** hardened for concurrent use: idempotency races (C2), rate limiter races (C3), bounded admission, execution timeouts
- **P2** introduced multi-agent capacity: workspace split, per-agent budgets, `X-Agent-Id` routing, per-agent rate limiters
- **120 tests**, clippy clean (`-D warnings`)
- Tags: `v0.1.0-p0-held`, `v0.1.0-p1-held`, `v0.2.0-p2-held`

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
  │  X-Agent-Id: openclaw        ← context selector (NOT auth)
  │  Idempotency-Key: <uuid>
  │
  ▼
┌──────────────────── safa-daemon ─────────────────────┐
│  resolve_agent_id() → per-agent rate limit check    │
│                                                     │
│  ┌──────────────── safa-core ──────────────────┐     │
│  │  1. Validate     magnitude, field exclusivity│    │
│  │  2. Canonicalize  → CanonicalAction (newtypes)│   │
│  │  3. Map           → domain_id (domains.toml) │   │
│  │  4. Authorize     → SLIME (per-agent budget) │   │
│  │  5. Actuate       → file/shell/HTTP          │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
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
| GET | `/ama/health` | Liveness check |
| GET | `/ama/version` | Version info |
| GET | `/ama/status` | Per-agent capacity + domain stats |
| POST | `/ama/action` | Execute action (`Idempotency-Key` + optional `X-Agent-Id` headers) |

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
cargo test --workspace --features test-utils    # 120 tests
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

Proprietary — SYFCORP.
