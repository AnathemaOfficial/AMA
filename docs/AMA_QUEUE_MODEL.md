# AMA Queue Model

## Bounded Admission Under Load

### Status: P1 Validated — Existing Implementation Sufficient
### Date: 2026-03-14
### Authors: GPT-4 (architect) + Claude Code (implementer) + Fireplank (director)

---

## 1. Purpose

This document defines AMA's bounded admission model under concurrent
load and answers the design questions from the P1 tasklist.

---

## 2. P1 Queue Implementation

AMA uses tower's `concurrency_limit(8)` layer as its bounded admission
control. This limits the number of concurrent in-flight requests to 8.

When the limit is reached, additional requests are held in tower's
internal queue until a slot becomes available. The 30-second global
`TimeoutLayer` ensures that no queued request waits indefinitely.

This is sufficient for P1 (localhost, single-node).

---

## 3. Design Decisions

### Does queue admission happen before or after capacity reservation?

**After idempotency check, before capacity reservation.**

The request lifecycle is:

```
1. Rate limit check          (pre-queue — fast, mutex-protected)
2. Idempotency check         (pre-queue — atomic via DashMap::entry)
3. Queue admission            (tower concurrency_limit — up to 8 slots)
4. Deserialization            (in-queue)
5. Capacity reservation       (in-queue — atomic CAS)
6. Actuation                  (in-queue — per-action timeout)
7. Result commit              (in-queue — idempotency complete())
```

Note: In the current implementation, the concurrency limit wraps the
entire handler, so steps 1-2 also consume a slot. This is acceptable
for P1 since those steps are fast (microseconds). A future phase could
move idempotency and rate limiting to middleware that runs before the
concurrency gate.

### Does `IN_FLIGHT` begin before queue entry or at worker acquisition?

**Before queue entry.** The idempotency `check_or_insert()` call
happens inside the handler, which runs within a concurrency slot.
The key transitions to IN_FLIGHT at the moment of atomic reservation,
which is after queue admission.

### What response is returned when queue is full?

Tower's `concurrency_limit` does not reject — it holds the request.
The 30-second `TimeoutLayer` converts a stalled request into:

```json
{
    "status": "error",
    "error_class": "timeout",
    "message": "request exceeded 30s global deadline"
}
```

This is fail-closed: no request can wait indefinitely.

### Is queue per capability or global for P1?

**Global.** One concurrency limit for all capabilities. Per-capability
limits are a future optimization.

---

## 4. Overflow Semantics

| Condition | Behavior |
|-----------|----------|
| < 8 concurrent requests | Immediate admission |
| = 8 concurrent requests | New request queued (waits for slot) |
| Queued > 30 seconds | TimeoutLayer fires → 503 response |
| Rate limit exceeded | 429 before queue entry |

The overflow model is deterministic and fail-closed.

---

## 5. Interaction with Idempotency State Machine

The queue does not create duplicate execution paths because:

1. Idempotency check runs inside the concurrency slot
2. `DashMap::entry()` is atomic — only one thread wins per key
3. If a queued request has the same key as an in-flight request,
   it receives `InFlight` (409 Conflict) and does not execute

---

## 6. Invariants

**Q1. Bounded concurrency**

At most 8 requests execute simultaneously.

**Q2. Bounded wait time**

No request waits longer than 30 seconds (TimeoutLayer).

**Q3. Fail-closed overflow**

Requests that cannot be served within the deadline receive an error.

**Q4. No duplicate execution from queuing**

Queued duplicates are caught by the idempotency state machine.

---

## 7. Known Limitations (P1)

- Queue is implicit (tower layer), not a first-class AMA construct
- No visibility into queue depth (no `/ama/status` field for it)
- No per-capability admission control
- Queued requests consume a tokio task but not a concurrency slot
  until admitted
- The 30s timeout applies to total request time, not just queue wait

These are acceptable for P1 and may be refined in future phases.
