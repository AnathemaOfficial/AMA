# SAFA Capacity Model

SAFA enforces two independent layers of capacity control on every agent. These layers
serve different purposes and operate on different timescales.

## Layer 1 — Monotonic Capacity (Thermodynamic Law)

**What:** A per-agent budget counter that only ever increases. Once capacity is consumed,
it is gone forever (within the current process lifetime).

**Mechanism:** `AtomicU64` compare-and-swap loop in `P0Authorizer` (safa-core/src/slime.rs).

**Properties:**
- Capacity starts at 0, increases toward `max_capacity` with each action
- Capacity **never decreases** — this is the thermodynamic invariant from SYF-Shield
- Each action's `magnitude` (1-1000) is added to the counter
- When `capacity_used + magnitude > max_capacity` → `Impossible` (403)
- Race-safe via hardware CAS — no locks needed
- Reset requires process restart ("Thermodynamic Cooling")

**Per-agent isolation:** Each agent gets its own `P0Authorizer` instance with its own
`AtomicU64` counter and `max_capacity`. Agent A exhausting its budget has zero effect
on Agent B.

**Why it exists:** This is the fundamental safety law. It guarantees that any agent has
a finite, bounded impact on the system. An agent cannot perform unlimited work — its
total lifetime actuation is bounded by `max_capacity`.

```
Agent "developer" (max_capacity = 10000)
  Request 1: magnitude 100 → capacity_used = 100    ✓ Authorized
  Request 2: magnitude 500 → capacity_used = 600    ✓ Authorized
  ...
  Request N: magnitude 50  → capacity_used = 9990   ✓ Authorized
  Request N+1: magnitude 20 → 9990 + 20 > 10000    ✗ Impossible
```

## Layer 2 — Operational Rate Limits (Safety Valve)

**What:** A per-agent sliding window counter that limits requests per time period.
Unlike Layer 1, this resets periodically.

**Mechanism:** `Mutex<RateLimitState>` per agent in `AppState` (safa-daemon/src/server.rs).

**Properties:**
- Configured via `rate_limit_per_window` and `rate_limit_window_secs` per agent
- Window resets when `elapsed >= window_secs`
- When `count > max_per_window` within the current window → `429 Too Many Requests`
- Checked **before** capacity reservation (Layer 1)

**Per-agent isolation:** Each agent has its own rate limiter. Agent A being rate-limited
does not affect Agent B.

**Why it exists:** Layer 1 is permanent — once capacity is used, it's gone. Layer 2
provides operational safety without permanent cost. It prevents burst abuse (an agent
sending 1000 requests in 1 second) while allowing sustained use over time.

## How They Interact

```
Incoming request
  │
  ├─ Layer 2: Rate limit check (per-agent, resets)
  │   └─ Exceeded? → 429 (no capacity consumed)
  │
  ├─ Layer 1: Capacity reservation (per-agent, permanent)
  │   └─ Exceeded? → 403 Impossible (permanent)
  │
  └─ Actuate (only if both layers pass)
```

**Key insight:** Layer 2 rejections are free — they don't consume Layer 1 capacity.
This means an agent that hits its rate limit can try again later without penalty.
But an agent that exhausts Layer 1 capacity is done for the session.

## Configuration Examples

**Conservative agent** (readonly):
```toml
max_capacity = 1000          # Low total lifetime budget
rate_limit_per_window = 10   # 10 requests per minute
rate_limit_window_secs = 60
```

**High-capacity agent** (developer):
```toml
max_capacity = 50000         # Large lifetime budget
rate_limit_per_window = 120  # 120 requests per minute
rate_limit_window_secs = 60
```

**Burst-tolerant agent** (ci-bot):
```toml
max_capacity = 20000         # Moderate lifetime budget
rate_limit_per_window = 200  # 200 requests per 5-minute window
rate_limit_window_secs = 300
```

## Domain-Level Limits

In addition to the two global layers, each domain within an agent has its own
`max_magnitude_per_action` limit. This prevents a single action from consuming
disproportionate capacity:

```toml
[agent.domains.fs_write_workspace]
enabled = true
max_magnitude_per_action = 100    # No single write > 100 magnitude
```

A domain can also be disabled entirely (`enabled = false`), which makes all actions
in that domain return `Impossible` regardless of capacity.
