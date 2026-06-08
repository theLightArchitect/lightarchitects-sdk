# State Machine — ConvSessionHandle Lifecycle

```mermaid
stateDiagram-v2
  [*] --> created: POST /api/conversation\n(ConvSessionHandle::new)

  created --> active: POST /api/conversation/{id}\n(first message sent; tokio::spawn dispatched)

  active --> idle: ConvSSEEvent::Done received\n(turn complete, no active spawn)

  idle --> active: POST /api/conversation/{id}\n(new message arrives; touch() updates last_active)

  idle --> evicted: TTL check — duration_since(last_active) > 1h\n(spawn_eviction_task retains false)

  active --> active: POST /api/conversation/{id}/interrupt\n(interrupt.store(true) — clears active_run;\nnext POST starts fresh turn)

  created --> evicted: DELETE /api/conversation/{id}\n(explicit session end)
  idle --> evicted: DELETE /api/conversation/{id}
  active --> evicted: DELETE /api/conversation/{id}

  evicted --> [*]: DashMap entry removed\nevent_tx dropped → all SSE subscribers receive RecvError::Closed

  note right of active
    inner: Arc<Mutex<ConvSessionInner>>
    active_run: Some(JoinHandle)
    event_tx: broadcast::Sender<ConvSSEEvent> (cap 256)
    interrupt: AtomicBool = false
  end note

  note right of idle
    active_run: None
    last_active: updated by touch()
    event_tx: still open for new subscriptions
  end note
```

## State invariants

| State | `active_run` | `interrupt` | `event_tx` |
|-------|-------------|-------------|-----------|
| `created` | None | false | open |
| `active` | Some(JoinHandle) | false (normally) | open |
| `active` (interrupted) | None (cleared) | true | open |
| `idle` | None | false | open |
| `evicted` | N/A | N/A | dropped |

## TTL eviction implementation note

`spawn_eviction_task` holds `Arc<ConvSessionStore>` and loops every 300s:
```rust
store.retain(|_, handle| {
    now.duration_since(*handle.last_active.lock().unwrap()) < ttl
});
```
Dropping the DashMap entry drops the `Arc<ConvSessionHandle>`. When the last Arc drops, `event_tx` is dropped → all `broadcast::Receiver`s receive `RecvError::Closed` → SSE stream handler returns `None` from `poll_next` → stream closes cleanly.
