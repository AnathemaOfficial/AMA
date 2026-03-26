---
status: candidate
version: 0.1
last_updated: 2026-03-25
scope: safa
---

# SAFA Hello World Packaging Candidate

## Purpose

Define the smallest package that can honestly be called a `ready-to-ship candidate`
for `SAFA` as an agent-facing constrained action adapter.

This draft proves:

- request validation
- canonicalization
- bounded domain mapping
- proof visibility
- one allowed effect
- one denied effect

It does not yet claim full product integration or full field validation.

## Adapter Role

`SAFA` sits above a constrained execution substrate and below the product layer.

Its job is to:

- accept agent-facing requests
- bind identity
- canonicalize intent
- map to finite domains
- enforce bounded policy
- expose manifest and proof surfaces

In the future execution chain, `SAFA` should ideally rely on
`SLIME-Enterprise` rather than only a public `SLIME-Core` proof surface.

## Hello World Goal

A local operator must be able to:

1. start `safa-daemon`
2. inspect a manifest for one dedicated demo agent
3. submit one allowed action
4. submit one denied action
5. fetch the proof record
6. confirm that only the allowed action produced an effect

## Demo Agent

The package must use a dedicated demo agent:

- `hello-world`

Reference file:

- `config/agents/hello-world.toml`

It must not use:

- `developer`

The demo agent should be narrowly bounded to:

- `file_read`
- `file_write`

Optional later probe:

- `http_request` on a tiny allowlist

Avoid in Hello World:

- broad shell execution
- broad networking
- broad capacities

## Minimal Scenario

### Allowed Action

- route: `POST /ama/action`
- agent: `hello-world`
- action: `file_write`
- target: `hello-world/hello.txt`
- magnitude: `1`
- payload: `hello from safa`

Expected:

- bounded authorization
- file written inside the demo workspace
- `x-safa-policy-hash` present
- proof record retrievable

### Denied Action

- route: `POST /ama/action`
- agent: `hello-world`
- action: `file_write`
- target: `../escape.txt`
- magnitude: `1`
- payload: `nope`

Expected:

- response indicates impossibility or denial
- no file written outside the workspace
- proof or journal still records the verdict

## Candidate Package Contents

Required:

- `safa-daemon`
- minimal `config.toml`
- minimal `domains.toml`
- minimal `allowlist.toml`
- minimal `intents.toml`
- one dedicated demo agent file
- start script
- quick validation script
- README or quickstart

Current repo candidate references:

- `config/agents/hello-world.toml`
- `docs/HELLO_WORLD_QUICKSTART.md`

Recommended demo profile shape:

- no hidden broad developer capacities
- explicit workspace root
- explicit agent id
- explicit placeholder secret policy if HMAC is used

## Validation Contract

### Manifest

```bash
curl http://127.0.0.1:8787/ama/manifest/hello-world
```

Expected:

- manifest exists
- policy hash exists
- exposed domains match the package

### Authorized Write

```bash
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: <uuid>" \
  -H "X-Agent-Id: hello-world" \
  -d '{"adapter":"hello-world","action":"file_write","target":"hello-world/hello.txt","magnitude":1,"payload":"hello from safa"}'
```

Expected:

- authorized bounded write
- file appears only in the package workspace
- proof metadata is visible

### Denied Write

```bash
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: <uuid>" \
  -H "X-Agent-Id: hello-world" \
  -d '{"adapter":"hello-world","action":"file_write","target":"../escape.txt","magnitude":1,"payload":"nope"}'
```

Expected:

- impossibility or denial response
- no out-of-bounds effect

## Release Acceptance Criteria

`SAFA Hello World Candidate` is accepted only if:

1. `cargo test --workspace` is green
2. manifest route works for the demo agent
3. one allowed action works
4. one denied action works
5. proof lookup works
6. package uses a dedicated demo agent, not a broad developer profile
7. documentation describes the package as a `ready-to-ship candidate`, not as a
   fully field-validated autonomous agent substrate

## Remaining Gaps Before True Ship Readiness

This candidate still leaves later work:

- package a dedicated demo config set instead of relying on the general dev tree
- define release artifact and checksum discipline
- define how the package binds to `SLIME-Enterprise`
- validate the full chain under a real appliance deployment

## Relationship to SLIME-Enterprise

`SAFA` is the adapter layer.
`SLIME-Enterprise` is the hardened execution appliance.

The target chain is:

`SCLAPY -> SAFA -> SLIME-Enterprise -> actuator -> system effect`

For now, this package proves the adapter surface on its own.
The full appliance-integrated package remains the next layer.
