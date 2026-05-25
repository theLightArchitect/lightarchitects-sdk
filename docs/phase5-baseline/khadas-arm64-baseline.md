# Phase 5 — ARM64 Embedding Baseline (Khadas RK3588)

**Date**: 2026-05-24  
**Host**: Khadas VIM3 (RK3588, aarch64-unknown-linux-gnu)  
**Rust**: 1.93.1  
**Service**: arena-platform.service (systemd user, port 8080)

## Configuration

| Field | Value |
|-------|-------|
| `LA_EMBEDDING_BACKEND` | `fastembed` |
| `LA_EMBEDDING_MODEL` | `nomic-embed-text-v1.5` |
| `LA_EMBEDDING_DIM` | `768` |
| Neo4j URI | `bolt://127.0.0.1:7687` (SSH tunnel → Mac M4) |
| Cache max | 64 MiB, TTL 300s |

## Dimension Lock Confirmation

Gateway startup log (first boot, model download ~210s):
```
"Embedding provider initialised — dimension lock confirmed",
"backend":"fastembed","model":"nomic-embed-text-v1.5","dim":768
```

## Latency Baseline

### Cold query (first-ever hit, fastembed embed + Neo4j HNSW)
- ~670ms per request (includes fastembed inference + Neo4j bolt round-trip)

### Warm-cache latency (TinyLFU hit, no embed/Neo4j)
| Query (truncated) | Trial 1 | Trial 2 | Trial 3 |
|---|---|---|---|
| embedding vector retrieval ARM | 25ms | 26ms | 26ms |
| fastembed nomic model initiali | 14ms | 26ms | 14ms |
| helix cache TinyLFU byte weigh | 14ms | 13ms | 14ms |
| Neo4j HNSW vector index query | 14ms | 26ms | 25ms |
| platform HTTP server gateway r | 13ms | 12ms | 25ms |

**Warm-cache p50**: ~14ms  
**Warm-cache p95**: ~26ms  
**Speedup (cold→warm)**: ~47× at p50

## Cache Behaviour

| Query # | `cache_hit_ratio` |
|---|---|
| 1 (cold) | 0.0 |
| 2+ (warm) | 1.0 |

Cache stats after 10 distinct queries:
```json
{"entry_count":10,"weighted_size_bytes":1887939}
```
~189 KB/entry avg weight (5 steps × ~37KB/step).

## Notes

- Neo4j is on Mac M4 at `10.129.155.41:7687`, accessed from Khadas via SSH
  reverse tunnel. Production deployment would use direct bolt connection.
- `nomic-embed-text-v1.5` requires 768-dim HNSW index — matches existing
  `step-embeddings` index in helix Neo4j.
- `all-minilm-l6-v2` (384-dim) also supported but requires separate HNSW
  index; would conflict with existing helix graph.
- Model cache path: `~/.cache/fastembed_cache/` (populated on first start).
