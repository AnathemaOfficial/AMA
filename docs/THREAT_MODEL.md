# AMA Threat Model

> Extracted from the P0 Design Specification, Section 7.

## Trust Boundary

AMA assumes a **single-host local trust boundary**.

- **Trusted:** AMA binary, embedded AB-S, configuration files (SHA-256 hashed at boot)
- **Untrusted:** All incoming HTTP requests (regardless of local origin), actuation targets (filesystem, network responses, process outputs)
- **Out of scope:** Host OS compromise (root attacker), physical access

## Threats Prevented Structurally

| Threat | Prevention | Type |
|--------|-----------|------|
| Shell Injection | `execv()` direct + intent mapping. No shell interpreter ever invoked. | Structural Impossibility |
| Path Traversal | `WorkspacePath` newtype rejects `..`, absolute paths. Per-component validation. | Structural Impossibility |
| Symlink Escape | `lstat` on every path component. Symlink → rejection. | Structural Impossibility |
| Arbitrary Command | Closed-set intents (`intents.toml`). Unknown = `422`. | Structural Impossibility |
| SSRF / Internal Net | DNS/IP filter: rejects loopback, RFC1918, link-local, metadata. IP re-validated post-connect. | Active Validation |
| Redirect Hijack | Every redirect re-validated against allowlist. POST redirect rejected. Max 3 hops. | Active Validation |
| Capacity DoS | Atomic CAS counter + rate limit (60 req/min) + concurrency cap (8). | Structural Limit |
| Action Replay | Mandatory `Idempotency-Key` (UUID v4). 5-min cache, 10K max entries. | Deduplication |
| Capacity Overflow | CAS with `checked_add`. `capacity` never exceeds `max_capacity`. | Structural Impossibility |
| Unknown Domain | Closed World Assumption. Absent domain → `Impossible`, never error. | Structural Impossibility |
| Policy Fuzzing | `403` returns strictly `{"status":"impossible"}`. Zero leakage. | Opacity |
| Partial Write | Atomic `.ama.<action_id>.tmp` + `rename()`. Crash = no file or old file. | Atomicity |
| Orphan Processes | `setpgid` + kill to process group. Best-effort containment. | Containment |
| Environment Leakage | Fresh minimal env. No host variables inherited. | Isolation |
| TLS Downgrade | HTTPS required + certificate validation enforced. | Enforcement |
| Memory Exhaustion | Body max 1 MiB, per-domain payload limits, bounded reading. | Limitation |
| Output Flooding | stdout/stderr 64 KiB, HTTP response 256 KiB. Truncation flagged. | Limitation |

## Known Limitations (P0/P1)

| Threat | Why Not Covered | Future Mitigation |
|--------|----------------|-------------------|
| Compromised Host | Cannot defend against root/kernel attacks | seccomp, namespaces |
| Semantic Malice | AMA validates form, not content. Writing valid but malicious content is permitted by design. | Agent responsibility |
| Config Tampering | TOML modified before boot → bad laws loaded | P0+: SHA-256 logged at boot. P1: signatures. |
| Timing Side-Channels | Response times vary by action type | Constant-time padding |
| Restart Loop | Forced restarts reset capacity | Detection via `session_id`. OS-level limits (systemd). |
| File Race Conditions | Concurrent writes = last-rename-wins | Optional file locking |
| Audit Persistence | Logs are local. Crash may lose entries. | WAL, syslog forward |
| Multi-tenancy | Single workspace, single trust domain | Per-agent namespaces |
| TOCTOU / Symlink (Windows) | `verify_no_symlinks` is no-op on Windows (C1) | Canonicalize + junction detection |

## Security Invariants (Normative)

These MUST hold at all times. Violation is a critical bug.

1. **No Shell Interpretation** — Every process uses `execv()` with pre-validated arg vector
2. **Workspace Containment** — No resolved path exits `workspace_root`
3. **Capacity Hard Limit** — `capacity` never exceeds `max_capacity` (hardware CAS)
4. **Closed World** — Unknown `domain_id` or intent → `Impossible`, never `Error`
5. **Zero Leakage** — `403` reveals nothing about policy state
6. **Fail-Closed** — Unexpected error = no actuation
7. **Static Law** — Config loaded once at boot, never reloaded
8. **Boot Integrity** — SHA-256 of all config files logged at startup
