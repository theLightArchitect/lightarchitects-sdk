# C1 — System Context: Lightspace in the Platform

```mermaid
graph LR
  Operator((Operator\nbrowser))
  Webshell[Webshell UI\nSvelte 5 / SvelteKit]
  Gateway[lightarchitects\ngateway binary]
  Lightspace[lightarchitects-lightspace\nRust SDK crate]
  Webshell_BE[lightarchitects-webshell\nbinary]
  TUI[lightshell TUI\nratatui]
  AYIN[(AYIN\n:3742)]
  Helix[(SOUL helix\nvault)]
  Disk[(~/.lightarchitects/\nlightspace/)]
  Siblings[CORSO/EVA/SOUL/\nQUANTUM/SERAPH/AYIN/LÆX]

  Operator -->|types intent| Webshell
  Webshell -->|SSE events| Operator
  Webshell -->|MCP| Gateway
  Gateway -->|dispatch| Siblings
  Webshell -->|HTTP API| Webshell_BE
  Webshell_BE -->|reduce(state,event)| Lightspace
  Webshell_BE -->|write NDJSON| Disk
  Webshell_BE -->|spans| AYIN
  TUI -->|reduce(state,event)| Lightspace
  TUI -->|read replay| Disk
  Lightspace -.->|no I/O, pure fn| Lightspace
```

**Scope**: The `lightarchitects-lightspace` crate is a **pure reducer** — it has zero I/O, zero async, no database calls. It receives `(CanvasState, CanvasEvent) → Result<CanvasState, ReducerError>`. All persistence, SSE, and network is handled by the consumers (webshell binary, lightshell TUI).
