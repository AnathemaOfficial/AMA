<img width="1199" height="349" alt="SAFA" src="https://github.com/user-attachments/assets/7e8d87a7-8695-4057-a019-3562f047a119" />

### SAFA - SLIME Adapter for Agents

> Agent-facing adaptation layer over a constrained actuation substrate.

SAFA is the layer that accepts agent requests, validates and canonicalizes them,
maps them to finite domains, and allows actuation only after constrained
authorization.

SAFA is **not** an agent. It is an adapter, proxy, translator, and bounded
executor.

This project was initially developed under the working name **AMA**
(`Agent Machine Armor`). The runtime surface still uses `/ama/` routes for
backward compatibility.

## Status

**Current truth:** P3 substrate resealed, with real containment properties, but
documentation must not overstate full maturity beyond the checked workspace and
tests.

- real HTTP contract
- real HMAC identity binding
- real per-agent manifests and proof hash surfaces
- real per-agent workspace isolation
- embedded SLIME mode by default
- `/ama/*` prefix still present for compatibility

## Architecture

SAFA is a Cargo workspace with two crates:

| Crate | Role | HTTP dependency |
|-------|------|-----------------|
| `safa-core` | decision engine (validate, map, authorize, actuate) | none |
| `safa-daemon` | HTTP transport wrapper | `axum` |

Current request path:

```
Agent -> SAFA -> constrained substrate -> actuation
```

The substrate exposed today is narrower than the broadest historical wording:

- `file_write -> fs.write.workspace`
- `file_read -> fs.read.workspace`
- `shell_exec -> proc.exec.bounded`
- `http_request -> net.out.http`

## Endpoints

> Endpoints currently use the `/ama/` prefix for backward compatibility.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | liveness check |
| GET | `/version` | version info |
| GET | `/ama/status` | per-agent capacity + domain stats |
| GET | `/ama/manifest/{agent_id}` | capability manifest + policy hash |
| POST | `/ama/action` | execute action |
| GET | `/ama/proof/{request_id}` | proof record lookup |

## Running

```bash
cargo build --workspace --release
./target/release/safa-daemon
```

## Tests

```bash
cargo test --workspace --features test-utils
cargo clippy --workspace --features test-utils -- -D warnings
```

## Documentation

| Document | Description |
|----------|-------------|
| `docs/ARCHITECTURE.md` | system architecture and design decisions |
| `docs/CAPACITY_MODEL.md` | two-layer capacity model |
| `docs/THREAT_MODEL.md` | threat model and security invariants |
| `docs/KNOWN_ISSUES_P1.md` | known issues and resolution status |
| `docs/HELLO_WORLD_PACKAGING_CANDIDATE.md` | package-level Hello World candidate for the bounded adapter surface |
| `docs/HELLO_WORLD_QUICKSTART.md` | validation flow for the hello-world demo agent |

## License

Proprietary - SYFCORP.
