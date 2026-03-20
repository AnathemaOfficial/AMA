# SAFA P1 Plan

## Multi-Agent Hardening Plan for SAFA

### Status: Draft Canonical Plan
### Date: 2026-03-14
### Authors: GPT-4 (architect) + Claude Code (validator) + Fireplank (director)

---

## 1. Purpose

SAFA P0 proved that the architecture works in real-world actuation:

- binary authorization
- intent/capability validation
- bounded execution
- capacity accounting
- real machine execution across hosts (Windows Studio → LT via Tailscale)
- GPU actuation via CLI-Anything → Blender (RTX 5070 Ti, ~4s render)

P1 is **not** about expanding scope.
P1 is about making SAFA **locally correct under concurrency**.

The objective of P1 is to transform SAFA from a **single-agent / controlled-context prototype** into a **multi-agent-safe local membrane**.

---

## 2. Canonical Scope

### In Scope

SAFA P1 covers only local runtime hardening:

1. **Idempotency state machine**
2. **Atomic capacity reservation**
3. **Bounded action queue**
4. **Race-safe rate limiting**
5. **Execution timeouts / bounded completion**
6. **Concurrency semantics for local capabilities**

### Out of Scope

The following are explicitly excluded from P1:

- distributed capability routing
- inter-machine orchestration
- remote capability discovery
- global admission gateways
- Machine-Suit network routing
- capability fabric across nodes
- policy reasoning / planning / agent cognition

These belong to later phases.

---

## 3. Canonical Framing

### SAFA P0
Proof that agent-under-law actuation is viable.

### SAFA P1
Make SAFA **locally safe under concurrent multi-agent use**.

### SAFA P1.5
Canonicalize the capability model.

### SAFA P2
Inter-machine admission and capability routing.

---

## 4. Core Architectural Principle

SAFA must remain:

- deterministic
- closed-world
- fail-closed
- non-cognitive
- actuation-focused

SAFA does **not** reason.
SAFA does **not** interpret intent semantically.
SAFA does **not** plan.

SAFA only decides whether an admissible action can cross the membrane.

---

## 5. Primary Problems Identified in P0

P0 is valid in controlled single-agent usage, but it contains structural limits under concurrency:

### C1. WorkspacePath TOCTOU / symlink race
Path validation on Windows has time-of-check-to-time-of-use vulnerability.

### C2. Idempotency race
Check-then-insert is non-atomic and can allow duplicate execution under simultaneous POSTs.

### C3. Rate limiter race
Admission counters may drift or fail under concurrent requests.

### C4. No execution queue
P0 is synchronous and assumes controlled usage.

### C5. Long-running action risk
A capability may block the membrane if execution is not bounded.

---

## 6. P1 Design Goals

P1 must guarantee:

1. Two equivalent requests cannot both become the "first".
2. Budget cannot be overspent under concurrent admission.
3. Excess load is rejected or queued deterministically.
4. Long-running actions cannot stall the membrane indefinitely.
5. Every admitted execution has a clearly bounded lifecycle.
6. The membrane remains minimal and understandable.

---

## 7. P1 Architecture

Canonical request flow for P1:

```
request
  → canonical validation
  → idempotency reservation
  → rate-limit admission
  → queue admission
  → atomic capacity reservation
  → bounded execution
  → result commit
  → response
```

This sequence is the architectural target for P1.

---

## 8. P1 Work Items

### 8.1 Idempotency State Machine

Replace "result cache only" semantics with a real request lifecycle state machine.

Canonical states:

- `ABSENT`
- `IN_FLIGHT`
- `DONE`

#### Required semantics

- If key is `ABSENT`, SAFA may reserve it and transition to `IN_FLIGHT`.
- If key is already `IN_FLIGHT`, SAFA must not start a second execution.
- If key is `DONE`, SAFA returns the committed result.
- State transitions must be atomic.

#### Goal

Prevent duplicate execution during concurrent submissions of the same logical action.

---

### 8.2 Atomic Capacity Reservation

Budget authorization must not be implemented as:

```
check → deduct
```

This must become a single atomic reservation step.

#### Required semantics

- reserve only if sufficient capacity exists
- fail closed if reserve cannot be completed atomically
- no negative or double-spent budget under concurrency

#### Goal

Preserve thermodynamic budget semantics under multi-agent load.

---

### 8.3 Bounded Action Queue

P1 introduces a bounded local queue.

#### Required semantics

- queue size must be finite
- overflow must fail closed
- queued requests must preserve deterministic admission order
- queue behavior must remain simple and inspectable

#### Goal

Protect the membrane from uncontrolled parallel actuation.

---

### 8.4 Race-Safe Rate Limiting

Rate limiting must be concurrency-safe and applied at admission.

Possible dimensions:

- per adapter
- per capability
- per source class
- global local node limit

#### Required semantics

- limiter must not drift under concurrent access
- denial must be explicit and fail-closed
- limiter must remain deterministic and minimal

#### Goal

Prevent valid-but-excessive request floods from exhausting the membrane.

---

### 8.5 Execution Timeouts

Every executable capability must have bounded execution time.

#### Required semantics

- timeout defined per capability or inherited from default
- timeout expiration produces terminal failure
- timeout must not leave the membrane in ambiguous state
- result commit semantics must remain deterministic

#### Goal

Avoid zombie actions and blocked workers.

---

### 8.6 Local Concurrency Semantics

P1 should define explicit concurrency behavior per capability class.

Examples:

- `max_concurrency = 1` for GPU render
- bounded parallelism for safe local compute
- exclusive lock for capabilities using singleton resources

#### Goal

Move from ad hoc execution behavior to explicit local admission law.

---

## 9. Acceptance Criteria

P1 is considered held only if the following are demonstrated:

1. **Duplicate-request race test**
   - simultaneous same-key requests do not double-execute

2. **Capacity race test**
   - concurrent requests cannot overspend budget

3. **Rate-limit race test**
   - concurrent flood respects limiter deterministically

4. **Queue saturation test**
   - overflow fails closed without undefined execution

5. **Timeout test**
   - blocked or hung execution terminates cleanly

6. **Multi-agent local test**
   - at least two concurrent adapters can use SAFA without breaking invariants

---

## 10. Proposed Deliverables

Suggested deliverables for P1:

- `docs/SAFA_P1_PLAN.md` (this document)
- `docs/SAFA_CONCURRENCY_MODEL.md`
- `docs/SAFA_IDEMPOTENCY_STATE_MACHINE.md`
- `docs/SAFA_QUEUE_MODEL.md`
- `docs/SAFA_TIMEOUTS.md`
- `KNOWN_ISSUES_P1.md` updated as work closes
- adversarial tests for race / queue / timeout behavior

---

## 11. Non-Goals

P1 must not become:

- a distributed orchestrator
- a remote scheduler
- an agent platform
- a policy engine
- an LLM mediator
- an explanation system

SAFA remains a **law-layer membrane for actuation**.

---

## 12. P1.5 Preview — Capability Model Canonicalization

After P1, SAFA should evolve from loose "intent" declarations toward explicit **capability manifests**.

Illustrative direction:

```toml
[capability.blender_render]
domain = "proc.exec.bounded"
binary = "/snap/bin/blender"
args_template = ["--background", "--python", "{{0}}"]
arg_schema = ["ScriptPath"]
magnitude = 10
timeout_ms = 30000
max_concurrency = 1
idempotent = true
adapter_allow = ["claude-code"]
description = "Render a Blender scene via background execution"
```

This is not P1 scope. This is the next canonicalization step after P1 runtime hardening.

---

## 13. P2 Preview — Capability Fabric / Admission Across Nodes

Only after P1 is held locally, and after the capability model is stabilized, should SAFA expand toward:

- multi-node routing
- remote capability discovery
- node admission
- Machine-Suit gateway semantics
- distributed capability fabrics

Canonical law:

> **Do not distribute before sealing the local membrane.**

---

## 14. Final Statement

SAFA P1 is the phase where SAFA stops being merely a successful prototype and becomes a locally reliable law-layer for multi-agent actuation.

P1 does not expand ambition. P1 seals correctness.

That discipline is the condition for everything that may follow.

---

## 15. Historical Context

This plan was co-designed on 2026-03-14 during a live session where:

- Claude Code deployed SAFA on Little-Terminator (Linux, RTX 5070 Ti)
- CLI-Anything was installed and used to generate Blender CLI
- First real GPU render was executed through SAFA's `proc.exec.bounded` domain
- The full pipeline was demonstrated: Agent → SAFA → CLI-Anything → Blender → GPU render in ~4s
- GPT-4 provided architectural analysis and drafted this canonical plan
- Claude Code validated technical details and corrected implementation specifics
- Fireplank directed the session and connected the strategic vision

This represents the first known functional demonstration of "Agent Under Law" with real GPU actuation.
