# litellm-platform-integration — proxy config

LiteLLM proxy running at `:4000`, bridging 7 provider backends to a single
OpenAI-compatible endpoint. All API keys are sourced from macOS Keychain via
`la_keychain_secret_provider.py`; nothing is stored in plaintext or `.env` files.

---

## Prerequisites

```bash
pip install litellm[proxy]      # or: pip install 'litellm[proxy,otel]'
pip install opentelemetry-exporter-otlp
```

SigNoz must be running with the OTLP gRPC receiver open on port `4317`
(default SigNoz configuration; see `~/Projects/observability/`).

---

## Starting the proxy

From this directory:

```bash
litellm --config litellm_config.yaml
```

LiteLLM will call `LAKeychainSecretProvider.get_secret()` for every
`os.environ/la-*-credential` reference before binding the port. If a Keychain
entry is missing, that model is skipped with a warning — the proxy still starts.

Default listen address: `http://0.0.0.0:4000`

---

## Health check

```bash
curl http://localhost:4000/health
# Returns: {"status":"healthy","litellm_status":"Connected",...}
```

Model-level liveness (sends a minimal prompt to each backend):

```bash
curl http://localhost:4000/health/liveliness
```

---

## Creating the IronClaw virtual key

The IronClaw worker authenticates with a virtual key scoped to `max_budget: 0.50`
per day. Create it once against the running proxy:

```bash
curl -s -X POST http://localhost:4000/key/generate \
  -H "Authorization: Bearer $(security find-generic-password -s la-litellm-master-key -a api_key -w)" \
  -H "Content-Type: application/json" \
  -d '{
    "key_alias": "ironclaw-worker",
    "max_budget": 0.50,
    "budget_duration": "1d",
    "metadata": {"owner": "ironclaw", "env": "local"}
  }' | jq -r '.key'
```

Store the returned key in Keychain:

```bash
security add-generic-password -s la-litellm-ironclaw-key -a api_key -w "<returned-key>"
```

The IronClaw worker then authenticates all proxy requests with:

```
Authorization: Bearer <ironclaw-virtual-key>
```

When the daily budget is exhausted the proxy returns HTTP 429; the worker should
back off and retry after the `budget_duration` window resets.

---

## OTLP trace correlation

Every LiteLLM request emits an OTEL trace to SigNoz at `http://127.0.0.1:4317`.

The response header `x-litellm-call-id` carries the LiteLLM call identifier.
Use this to correlate proxy-level spans with upstream provider spans in SigNoz:

```bash
curl -i http://localhost:4000/chat/completions \
  -H "Authorization: Bearer <virtual-key>" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-6","messages":[{"role":"user","content":"ping"}]}'

# Response headers include:
# x-litellm-call-id: <uuid>
```

In SigNoz, search for `litellm.call_id = <uuid>` or filter by
`service.name = litellm-proxy` to see the full trace.

---

## Keychain entries reference

| Keychain service | Account | Credential |
|---|---|---|
| `la-anthropic-credential` | `api_key` | Anthropic API key |
| `la-openai-credential` | `api_key` | OpenAI API key |
| `la-ollama-cloud-credential` | `api_key` | Ollama Cloud Bearer token |
| `la-deepseek-credential` | `api_key` | DeepSeek API key |
| `la-vertex-project` | `api_key` | GCP project ID |
| `la-vertex-credential` | `api_key` | GCP service account JSON (base64 or path) |
| `la-mistral-credential` | `api_key` | Mistral API key |
| `la-litellm-master-key` | `api_key` | LiteLLM proxy admin key |

Create a missing entry:

```bash
security add-generic-password -s <service> -a api_key -w "<secret>"
```
