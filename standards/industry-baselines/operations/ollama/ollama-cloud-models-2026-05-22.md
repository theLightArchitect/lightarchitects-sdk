<!-- uuid: 5549d70d-b09b-410d-965f-04de02fa1177 -->
<!-- source: https://docs.ollama.com/cloud | version: documented as of 2026-05-22 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: per Ollama docs revision -->
<!-- gate: [O] -->

# Ollama Cloud Models — Routing Behaviour

## Cloud Models — definition

> Ollama's cloud models are a new kind of model in Ollama that can run without a powerful GPU. Instead, cloud models are automatically offloaded to Ollama's cloud service while offering the same capabilities as local models, making it possible to keep using your local tools while running larger models that wouldn't fit on a personal computer.

A model with the `:cloud` suffix in Ollama (e.g. `gpt-oss:120b-cloud`, `deepseek-v3.2:cloud`, `nemotron-3-super:cloud`, `glm-4.7:cloud`) **is not stored locally**. The local Ollama daemon ships a small manifest (~300–400 bytes) and proxies all generation requests to `https://ollama.com:443`.

## Identifying a cloud model on the local daemon

`curl http://127.0.0.1:11434/api/tags` exposes `remote_model` + `remote_host` fields for cloud models:

```json
{
  "name": "gpt-oss:120b-cloud",
  "model": "gpt-oss:120b-cloud",
  "remote_model": "gpt-oss:120b",
  "remote_host": "https://ollama.com:443",
  "size": 384
}
```

The `size: 384` indicates the local manifest size in bytes, **not** model weight size. True local models register full GB-scale sizes.

## Authentication & subscription tier

```bash
ollama signin                  # interactive sign-in to ollama.com
ollama pull gpt-oss:120b-cloud # registers the routing manifest
```

Free tier and Pro tier (~$20/month) gate access to larger cloud models. Without an authenticated session, `:cloud` models return auth errors at inference time.

## Direct API access pattern (skipping local daemon)

```bash
export OLLAMA_API_KEY=<key from https://ollama.com/settings/keys>
```

Python:

```python
from ollama import Client
import os

client = Client(
    host="https://ollama.com",
    headers={'Authorization': 'Bearer ' + os.environ['OLLAMA_API_KEY']}
)

for part in client.chat('gpt-oss:120b', messages=[{'role':'user','content':'…'}], stream=True):
    print(part['message']['content'], end='', flush=True)
```

## Implications for Light Architects deployment on Khadas

A locally-running `ollama serve` listing only `:cloud` models is **not performing local inference** — every prompt egresses to `ollama.com`. For true local inference on Khadas Edge2:

- Use `ollama pull <model-without-:cloud-suffix>` to download real weights (e.g. `ollama pull nomic-embed-text` for embeddings, `ollama pull qwen2.5:3b` for chat)
- OR bypass Ollama entirely and use **RKLLM toolkit** to run quantised models on the RK3588 NPU (see `performance/rockchip/rk3588-npu-rkllm-benchmarks-2026-05-22.md`) — RKLLM runs ~15 tok/s for 1.5B models, which is faster than CPU-only Ollama on the same board

The presence of `:cloud` models alongside `OLLAMA_HOST=127.0.0.1:11434` is a misleading signal — local-looking, cloud-routed in fact.

## Listing cloud-eligible models

`https://ollama.com/search?c=cloud` enumerates the supported cloud catalogue.
