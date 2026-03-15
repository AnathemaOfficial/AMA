# AMA — Agent Machine Armor

> Universal law-adapter membrane for AI agents.

AMA translates agent intentions into canonical SLIME domains and permits real-world
actuation only after binary authorization. It is **not** an agent — it is an adapter,
proxy, translator, and minimal executor.

```
Agent → AMA → SLIME/AB-S → Real world actuation
```

## Status

**AMA P0 — HELD** (Canonical Local Baseline)

P0 validates the full end-to-end architecture:
**validate → map → authorize → actuate**, with bounded localhost serving,
structured audit, and closed-world authorization.

P0 is not the final hardened release. Known concurrency and path-safety issues
are documented in [`docs/KNOWN_ISSUES_P1.md`](docs/KNOWN_ISSUES_P1.md) and
explicitly deferred to P1.

## Architecture

```
POST /ama/action
    │
    ├─ Ingress: JSON schema validation, field exclusivity
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
| GET | `/ama/status` | Runtime metrics |
| POST | `/ama/action` | Execute action (requires `Idempotency-Key` header) |

## Tests

```bash
cargo test --features test-utils    # 68 tests
cargo clippy --features test-utils -- -D warnings
```

## License

Proprietary — SYFCORP.
