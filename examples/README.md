# SAFA Examples

Example requests for the SAFA action endpoint.

## Usage

```bash
# File write
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d @file_write.json

# Shell exec (intent-mapped)
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d @shell_exec.json

# HTTP GET (allowlisted URL)
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d @http_request.json

# Dry run (no actuation)
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{"adapter":"generic","action":"file_write","target":"test.txt","magnitude":1,"dry_run":true,"payload":"test"}'

# Health check
curl http://127.0.0.1:8787/ama/health

# Status (capacity + domain stats)
curl http://127.0.0.1:8787/ama/status
```
