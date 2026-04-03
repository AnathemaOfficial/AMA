---
name: Bug Report
about: Create a report to help us improve
title: '[BUG] '
labels: bug
assignees: ''
---

## Description
<!-- Clear, concise description of the observed behavior. -->

## Reproduction steps
1. Environment: `uname -a`, binary version, commit hash
2. Configuration: `cat config.toml` (sanitized)
3. Command: `...`
4. Observed result: `...`

## Expected behavior
<!-- What should have happened. -->

## Security / conformity impact
- [ ] Structural invariant violation (C1/C2)
- [ ] Information leak / side-channel
- [ ] Denial of service / resource exhaustion
- [ ] Audit / traceability non-conformity
- [ ] No security impact identified

## SYF context (optional)
- Zone: [ ] RZ [ ] EP [ ] IZ
- Potentially violated invariant: `...`

## Environment
- OS: [ ] Linux [ ] Unix [ ] Windows [ ] Other
- Rust version: `rustc --version`
- Commit: `git rev-parse HEAD`
