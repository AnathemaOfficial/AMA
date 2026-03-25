# SAFA Hello World Quickstart

This quickstart validates the `SAFA Hello World Packaging Candidate`.

It uses the dedicated demo agent:

- `hello-world`

It does not use:

- `developer`

## Preconditions

- build workspace available
- local config tree present
- `config/agents/hello-world.toml` present

## Start

```bash
cargo build --workspace --release
./target/release/safa-daemon
```

## Check manifest

```bash
curl http://127.0.0.1:8787/ama/manifest/hello-world
```

Expected:

- manifest exists
- policy hash exists
- only read and write workspace domains are enabled

## Allowed action

```bash
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: 11111111-1111-1111-1111-111111111111" \
  -H "X-Agent-Id: hello-world" \
  -d '{"adapter":"hello-world","action":"file_write","target":"hello-world/hello.txt","magnitude":1,"payload":"hello from safa"}'
```

Expected:

- authorized response
- file appears under `workspace/hello-world/hello.txt`
- response includes `x-safa-policy-hash`

## Denied action

```bash
curl -X POST http://127.0.0.1:8787/ama/action \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: 22222222-2222-2222-2222-222222222222" \
  -H "X-Agent-Id: hello-world" \
  -d '{"adapter":"hello-world","action":"file_write","target":"../escape.txt","magnitude":1,"payload":"nope"}'
```

Expected:

- impossible or denial response
- no out-of-bounds file appears

## Proof lookup

Fetch the `request_id` returned by the allowed or denied action, then:

```bash
curl http://127.0.0.1:8787/ama/proof/<request_id>
```

Expected:

- proof record exists
- verdict is visible
- policy hash is visible

## Notes

- this is a package candidate, not a fully field-validated deployment
- later integration should place SAFA above `SLIME-Enterprise`
