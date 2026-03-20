# SAFA Concurrency Model

## Atomic Capacity Reservation

### Status: Validated — Already Correct in P0
### Date: 2026-03-14
### Authors: GPT-4 (architect) + Claude Code (reviewer) + Fireplank (director)

---

## 1. Purpose

This document describes SAFA's capacity reservation model and confirms
that the P0 implementation is already race-safe under concurrent
multi-agent use.

WS2 in the P1 tasklist required verifying that capacity authorization
is indivisible. This document serves as that verification.

---

## 2. Capacity Semantics

SAFA uses a **thermodynamic capacity model** inspired by SYF-Shield.

The key invariant:

> Total admitted magnitude must never exceed the configured maximum
> capacity for a given session.

Capacity is **monotonically increasing** — once reserved, units are
never released. This is by design (SYF thermodynamic law). A session's
capacity counter represents **used capacity**, not remaining budget.

```
capacity = 0                       (session start)
capacity += magnitude_1            (first action)
capacity += magnitude_2            (second action)
...
capacity <= max_capacity            (invariant: always)
capacity > max_capacity             (impossible by construction)
```

---

## 3. Data Structure

```rust
pub struct P0Authorizer {
    capacity: AtomicU64,                       // used capacity (starts at 0)
    max_capacity: u64,                         // immutable ceiling
    domains: HashMap<DomainId, DomainPolicy>,  // immutable after boot
    session_id: Uuid,                          // immutable
}
```

- `capacity` is `AtomicU64` — the only mutable field
- `max_capacity` and `domains` are immutable after construction
- No locks needed for immutable fields

---

## 4. Atomic Reservation: CAS Loop

The core reservation uses Compare-And-Swap (CAS):

```rust
fn try_reserve(&self, domain_id: &DomainId, magnitude: u64) -> SlimeVerdict {
    // Step 1: policy check (immutable data, no sync needed)
    if let Err(v) = self.check_policy(domain_id, magnitude) {
        return v;
    }

    // Step 2: atomic capacity reservation
    loop {
        let current = self.capacity.load(Ordering::Acquire);
        match current.checked_add(magnitude) {
            Some(new) if new <= self.max_capacity => {
                match self.capacity.compare_exchange_weak(
                    current, new,
                    Ordering::AcqRel, Ordering::Acquire,
                ) {
                    Ok(_) => return SlimeVerdict::Authorized,
                    Err(_) => continue,  // another thread won, retry
                }
            }
            _ => return SlimeVerdict::Impossible,
        }
    }
}
```

### Why this is correct

1. **`checked_add(magnitude)`** — prevents u64 overflow
2. **`compare_exchange_weak(current, new)`** — atomically verifies that
   `capacity` is still `current` and sets it to `new` in one CPU
   instruction. If another thread modified `capacity` between `load`
   and `compare_exchange`, the CAS fails and the loop retries.
3. **`Ordering::AcqRel`** — ensures all threads see consistent state
4. **No gap between check and deduct** — the CAS is the check AND the
   deduct simultaneously

### Concurrency behavior

```
Thread A: load(50) → CAS(50→60) → succeeds ✅
Thread B: load(50) → CAS(50→60) → FAILS (capacity is now 60) → retry
Thread B: load(60) → CAS(60→70) → succeeds ✅
Thread C: load(70) → 70+40=110 > 100 → Impossible ❌
```

No thread can overspend. The invariant holds under arbitrary contention.

---

## 5. Capacity Variable Semantics

**Important clarification** (per GPT review):

The `capacity` field represents **used capacity** (starts at 0, goes up).
It does NOT represent remaining budget (which would go down).

Therefore `checked_add(magnitude)` is correct — we are adding to the
used total, not subtracting from a remaining balance.

The check `new <= self.max_capacity` enforces the ceiling.

---

## 6. Non-Release by Design

Capacity is never released after reservation, even if actuation fails.

This is a deliberate thermodynamic invariant:

- Once an action is admitted, its capacity cost is permanent
- Failed actuations still consume capacity
- This prevents replay attacks and ambiguous state
- The session eventually resets capacity to zero on restart

This aligns with SYF-Shield's monotonic progression law.

---

## 7. Existing Test Coverage

The following tests already validate WS2 invariants:

### `capacity_never_exceeds_max_concurrent` (test_slime.rs)
- 100 threads race to reserve 10 units each in 1000-unit capacity
- Verifies exactly 100 reservations succeed
- Verifies used capacity equals exactly 1000
- **This is the concurrent race test for WS2**

### `capacity_exhaustion` (test_slime.rs)
- Two 50-unit reservations fill 100-unit capacity
- Third request returns Impossible

### `check_only_does_not_consume_capacity` (test_slime.rs)
- Dry-run check does not mutate capacity

### `impossible_returns_403` (test_integration.rs)
- End-to-end: capacity=1, first action succeeds, second returns 403

---

## 8. WS2 Invariants — All Held

| Invariant | Status | Evidence |
|-----------|--------|----------|
| No overspend under concurrent load | ✅ HELD | AtomicU64 CAS loop; 100-thread race test |
| No partial execution after failed reservation | ✅ HELD | Pipeline reserves BEFORE actuation |
| Capacity check is indivisible | ✅ HELD | Single CAS instruction, no gap |
| Budget monotonically increases | ✅ HELD | No decrement path exists |
| Zero remaining budget → Impossible | ✅ HELD | `checked_add` + ceiling check |

---

## 9. Conclusion

WS2 does not require code changes.

The P0 implementation of capacity reservation was already correct by
construction, using the strongest available primitive (`AtomicU64` CAS
with acquire-release ordering).

This document confirms WS2 as validated during P1 review.
