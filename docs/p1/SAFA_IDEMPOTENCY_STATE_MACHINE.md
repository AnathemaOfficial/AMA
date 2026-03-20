# SAFA Idempotency State Machine

## Canonical Request Lifecycle for Idempotent Action Admission

### Status: Draft Canonical Spec
### Date: 2026-03-14
### Authors: GPT-4 (architect) + Claude Code (Rust reviewer) + Fireplank (director)

---

## 1. Purpose

This document defines the canonical idempotency model for SAFA.

The purpose of idempotency in SAFA is not merely to cache prior results.

Its purpose is to guarantee that the same logical action request, identified
by a valid idempotency key, cannot produce multiple executions under
concurrent submission.

In SAFA, idempotency is part of the admission law.

It prevents duplicate actuation.

---

## 2. Scope

This document covers only the lifecycle of a request identified by an
idempotency key within a single SAFA node.

It defines:

- canonical request states
- allowed transitions
- concurrent submission behavior
- replay behavior
- timeout / failure interaction
- terminal result semantics

This document does **not** define:

- distributed idempotency across nodes
- remote capability routing
- global orchestration semantics
- long-term retention or persistence guarantees beyond local node policy

---

## 3. Design Principle

SAFA must guarantee the following invariant:

> Two requests carrying the same valid idempotency key must never produce
> two independent executions of the protected action.

This guarantee must hold even if the requests arrive concurrently.

Idempotency is therefore modeled as a **state machine**, not as a simple
result cache.

---

## 4. Canonical States

Each valid idempotency key exists in exactly one of the following states:

### `ABSENT`

The key has no active or committed request record on the local SAFA node.

No execution has been reserved.

### `IN_FLIGHT`

The key has been atomically reserved by exactly one request execution path.

The associated action has been admitted for execution but has not yet
reached terminal result commit.

This state represents exclusive ownership of execution for that
idempotency key.

### `DONE`

The request has reached terminal result commit.

The execution outcome is fixed for replay purposes.

Any subsequent request using the same idempotency key must not re-execute
the action.

---

## 5. State Transition Model

Canonical lifecycle:

```text
ABSENT → IN_FLIGHT → DONE
```

No other forward states are defined in P1.

### Allowed transitions

- `ABSENT → IN_FLIGHT`
- `IN_FLIGHT → DONE`

### Forbidden transitions

- `DONE → IN_FLIGHT`
- `DONE → ABSENT`
- `IN_FLIGHT → ABSENT`
- any transition that would permit a second execution path for the same key

---

## 6. Transition Semantics

### 6.1 ABSENT → IN_FLIGHT

This transition must be **atomic**.

It represents the reservation of exclusive execution ownership for the key.

Exactly one request may successfully perform this transition.

If two or more concurrent requests attempt to reserve the same key, at most
one may win.

All others must observe that the key is no longer `ABSENT`.

**Canonical meaning**: The request that successfully performs
`ABSENT → IN_FLIGHT` becomes the sole execution owner for that key.

### 6.2 IN_FLIGHT → DONE

This transition occurs exactly once, at terminal result commit.

The result committed in `DONE` is the canonical replay result for that key.

The transition to `DONE` must be deterministic and terminal for the local
lifecycle of that key.

---

## 7. Concurrency Semantics

### 7.1 Concurrent first submissions

If two requests with the same idempotency key arrive concurrently while
the key is `ABSENT`:

- exactly one request may transition the key to `IN_FLIGHT`
- all others must fail to acquire execution ownership
- no second execution may begin

This is the core idempotency guarantee.

### 7.2 Duplicate request during IN_FLIGHT

If a second request arrives while the same key is already `IN_FLIGHT`,
SAFA must not start a new execution.

For P1, the canonical behavior should be one of the following
implementation policies:

**Policy A — explicit in-flight response**

Return a deterministic response indicating that execution is already in
progress for that key.

**Policy B — wait-then-replay**

Block until the first execution reaches `DONE`, then return the committed
result.

### P1 Decision: Policy A

**Rationale** (Claude Code + GPT consensus):

- Simpler implementation (no notify/wait coordination)
- More fail-closed (caller knows immediately)
- No risk of blocking the membrane if execution hangs
- Caller can retry after a delay if needed
- Consistent with SAFA's philosophy: deterministic, minimal, non-blocking

The only forbidden behavior is duplicate execution.

### 7.3 Replay after DONE

If a request arrives with a key already in `DONE`, SAFA must:

- not re-execute the action
- return the committed terminal result associated with that key

This includes both successful and unsuccessful terminal outcomes, if the
implementation commits both as replayable results.

---

## 8. Terminal Result Semantics

The `DONE` state must contain a terminal committed result for replay.

At minimum, the committed result must be sufficient to deterministically
answer subsequent duplicate requests.

The replayed result must be semantically identical to the original
committed outcome for that key.

Replay semantics depend on local retention policy and are valid as long
as the entry remains present on the node. Once an entry is evicted by
TTL or capacity policy, a subsequent request with the same key may be
treated as a new `ABSENT` key. P1 does not define long-term retention
guarantees.

**Canonical requirement**: Once `DONE` is reached, the local node must
treat the request as execution-complete and replay-only. No further
actuation is permitted for that key.

---

## 9. Failure Semantics

Idempotency must define what happens when execution does not succeed
normally.

This is critical because failure can otherwise create ambiguous replay
behavior.

### P1 Decision: Model A — Commit All Terminal Outcomes

Any terminal execution outcome transitions to `DONE`, including:

- success
- deterministic denial
- execution failure
- timeout

In this model, replay always returns the committed terminal outcome.

**Rationale**: Simple, deterministic, closed-world. No ambiguous states.

---

## 10. Timeout Semantics

If execution exceeds its allowed timeout and timeout is treated as
terminal, then the request should transition:

```text
IN_FLIGHT → DONE
```

with a committed timeout result.

This preserves replay determinism.

A duplicate request after timeout must not start a fresh execution unless
a later phase explicitly introduces different semantics.

---

## 11. Deterministic Denial Semantics

If a request is denied after successfully acquiring execution ownership
and the denial is terminal, the node should commit that denial into `DONE`.

This ensures that subsequent replay with the same key returns the same
denial rather than attempting admission again.

This is especially important if capacity reservation, policy checks, or
execution rules occur after the idempotency reservation step.

---

## 12. Relation to Capacity and Queue

Idempotency reservation is logically distinct from capacity reservation
and queue admission.

However, the order of operations in the request pipeline must preserve
the invariant that duplicate requests cannot produce duplicate execution.

P1 implementation must therefore define clearly:

- whether `IN_FLIGHT` begins before queue entry or at worker acquisition
- whether capacity reservation occurs before or after queue admission
- how timeout and queue cleanup interact with the idempotency state machine

P1 does not resolve this ordering yet; it only requires that whichever
ordering is chosen must preserve single execution ownership. This
document does not fix those ordering choices, but it requires that
the chosen ordering preserve the idempotency invariant.

---

## 13. Invariants

The following invariants are canonical.

**I1. Single execution ownership**

At most one execution path may own a given idempotency key.

**I2. No duplicate actuation**

A single idempotency key must never produce two independent protected
executions.

**I3. Terminal replay only**

Once a key reaches `DONE`, all future requests with that key are
replay-only.

**I4. No ambiguous in-flight duplication**

A duplicate arriving during `IN_FLIGHT` must not begin execution.

**I5. Deterministic local semantics**

The same key on the same SAFA node must follow a deterministic lifecycle.

---

## 14. P0 Bug Analysis

The current P0 implementation in `src/idempotency.rs` uses a
**check-then-insert** pattern via `DashMap`:

```rust
// Step 1: GET — check if key exists
if let Some(entry) = self.entries.get(&key) { ... }

// GAP — another thread can observe ABSENT here

// Step 2: INSERT — mark as in-flight
self.entries.insert(key, CacheEntry { in_flight: true, ... });
```

Between Step 1 and Step 2, a concurrent request can also observe `ABSENT`
and proceed to insert. This violates invariant **I1** and **I2**.

### Required P1 fix

Replace the two-step check-then-insert with an atomic
**insert-if-not-exists** operation.

In Rust with `DashMap`, this is `entry()` API:

```rust
use dashmap::mapref::entry::Entry;

match self.entries.entry(key) {
    Entry::Vacant(vacant) => {
        vacant.insert(CacheEntry { in_flight: true, ... });
        IdempotencyStatus::New  // we won the reservation
    }
    Entry::Occupied(occupied) => {
        // key already exists — check state
        let entry = occupied.get();
        if entry.in_flight {
            IdempotencyStatus::InFlight
        } else if let Some(ref result) = entry.result {
            IdempotencyStatus::Cached(result.clone())
        } else {
            IdempotencyStatus::InFlight // defensive
        }
    }
}
```

This makes `ABSENT → IN_FLIGHT` atomic within `DashMap`'s shard lock.

---

## 15. Required Test Cases

The implementation must be validated with at least the following tests.

### Sequential replay
- same key submitted twice sequentially
- first execution runs once
- second request returns replayed result
- no second execution occurs

### Concurrent duplicate submission
- same key submitted concurrently by two request paths
- exactly one execution occurs
- the loser does not begin execution

### Duplicate during active execution
- same key submitted while original request remains `IN_FLIGHT`
- duplicate request does not begin execution
- returned behavior matches Policy A (in-flight response)

### Replay after success
- successful execution commits to `DONE`
- replay returns committed success result

### Replay after timeout
- timed-out execution commits terminal timeout result
- replay returns same timeout result
- no second execution occurs

### Replay after deterministic denial
- denied execution commits terminal denial result
- replay returns same denial
- no second execution occurs

---

## 16. Explicit Non-Goals

This document does not require:

- cross-node idempotency
- durable distributed storage
- global key uniqueness beyond local node policy
- semantic deduplication of requests without keys
- re-issuance of execution under the same key after terminal commit

---

## 17. Final Statement

In SAFA, idempotency is not an optimization.

It is a law of execution ownership.

The key question is not "have we seen this request before?"

The key question is:

> "Has this action key already acquired or completed the sole admissible
> execution path?"

That is why SAFA idempotency must be implemented as a state machine.

A cache is not sufficient.

A law of exclusive execution is required.
