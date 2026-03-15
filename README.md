# AMA — Agent Machine Armor

> Universal law-adapter membrane for AI agents.

AMA translates agent intentions into canonical SLIME domains and permits real-world
actuation only after binary authorization. It is **not** an agent — it is an adapter,
proxy, translator, and minimal executor.

```
Agent → AMA → SLIME/AB-S → Real world actuation
```

## Status

**AMA P1 — HELD** (Concurrent Multi-Agent Baseline)

- **P0** validated the full pipeline: **validate → map → authorize → actuate**
- **P1** hardened for concurrent multi-agent use: idempotency races (C2), rate limiter races (C3), bounded admission, execution timeouts
- **94 tests**, clippy clean (`-D warnings`)
- Tags: `v0.1.0-p0-held`, `v0.1.0-p1-held`

Known issues documented in [`docs/KNOWN_ISSUES_P1.md`](docs/KNOWN_ISSUES_P1.md).

## Architecture

```
POST /ama/action
    │
    ├─ Ingress: Rate limit, body limit, idempotency check
    ├─ Schema:  JSON validation, field exclusivity
    ├─ Mapper:  action → domain_id (via domains.toml)
    ├─ SLIME:   binary authorization (Authorized / Impossible)
    ├─ Actuator: file write/read, shell exec, HTTP request
    └─ Audit:   structured tracing, SHA-256 request hash
```

**Key properties:**
- **Closed world** — unknown domain_ids → Impossible
- **Fail-closed** — any error = no actuation
- **Finite action universe** — bounded, enumerable transition space
- **Correctness by construction** — Rust newtypes with private constructors
- **Thermodynamic capacity** — monotonic entropy via AtomicU64 CAS
- **Idempotent** — UUID v4 deduplication, atomic DashMap entry API

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for full details.

## Stack

Rust 1.93 · axum 0.8 · tokio · serde · dashmap · sha2 · reqwest · tracing

## Running

```bash
cargo build --release
./target/release/ama
# Listens on 127.0.0.1:8787
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/ama/health` | Liveness check |
| GET | `/ama/version` | Version info |
| GET | `/ama/status` | Runtime metrics (capacity, domains) |
| POST | `/ama/action` | Execute action (requires `Idempotency-Key` header) |

## Quick Example

```bash
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{"adapter":"generic","action":"file_write","target":"hello.txt","magnitude":1,"payload":"hello world"}'
```

See [`examples/`](examples/) for more.

## Tests

```bash
cargo test --features test-utils    # 94 tests
cargo clippy --features test-utils -- -D warnings
```

## Documentation

| Document | Description |
|----------|-------------|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | System architecture and design decisions |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Threat model and security invariants |
| [`docs/KNOWN_ISSUES_P1.md`](docs/KNOWN_ISSUES_P1.md) | Known issues and resolution status |
| [`docs/p1/`](docs/p1/) | P1 workstream documentation |
| [`docs/superpowers/specs/`](docs/superpowers/specs/) | Design specifications |

## License

Proprietary — SYF Corp.
