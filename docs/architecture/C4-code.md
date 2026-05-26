# C4 — Code Diagram: Program::run() execution flow

> Canon XLI: Architect-authored design input. Phase 1 deliverable.

```mermaid
sequenceDiagram
  autonumber
  participant OP as Operator
  participant LB as lightsquad_bridge
  participant PROG as Program::run()
  participant PF as Preflight
  participant SUP as Supervisor (long-running)
  participant WD as WaveDispatcher
  participant SLOT3 as OllamaCloudCodingProvider (SLOT 3)
  participant ORV as OllamaResponseValidator
  participant OC as Ollama Cloud
  participant DP as DecisionPipeline
  participant RG as ReviewGate
  participant MA as MergeAgent
  participant DC as Decisions ledger
  participant SSE as Webshell SSE

  OP->>LB: launch_build(ProgramConfig)
  LB->>PROG: Program::new() + run()
  PROG->>PF: disk() + api() + canon() + dry_run()
  PF-->>PROG: PrefightOk

  PROG->>SUP: tokio::spawn(supervisor.run())
  Note over SUP: Long-running; polls ironclaw-hitl channel<br/>Loads canon + northstar + plan into prompt cache (~80K tokens)

  loop For each wave in program
    PROG->>WD: dispatch_wave(wave_tasks, worker_fn)

    par Per task (concurrent JoinSet)
      WD->>SLOT3: spawn_task(task, worktree_path)
      SLOT3->>OC: POST /api/chat {model, messages, stream:true}
      OC-->>SLOT3: NDJSON stream (token deltas)
      SLOT3->>SLOT3: parse_stream() → diff
      SLOT3->>ORV: validate(diff, task_scope)
      alt Validation PASS
        ORV-->>SLOT3: ValidationResult::Ok
        SLOT3-->>WD: TaskComplete(diff)
      else Validation FAIL (path traversal / denied file / diff ceiling)
        ORV-->>SLOT3: ValidationResult::Violation(reason)
        SLOT3-->>WD: TaskFail(ValidationError)
      end
    end

    WD->>MA: merge_task_branches(completed_tasks)
    MA->>MA: Arc<Mutex> serialized git ops
    MA-->>PROG: MergeOk

    PROG->>RG: evaluate_wave(wave_artifacts)
    RG-->>PROG: GateResult (pass | retry | fail)

    opt HITL escalation during wave
      SUP->>DP: apply_pipeline(decision_context)
      DP->>DP: Layer 1: canon_check()
      DP->>DP: Layer 2: northstar_check()
      alt Canon/Northstar resolves
        DP-->>SUP: Decision::Autonomous(verdict)
        SUP->>DC: append_hmac_entry(verdict)
      else Layer 3: LightArchitect consultation
        DP->>DP: light_architects.consult(gate_dim)
        DP-->>SUP: Decision::Autonomous(verdict)
        SUP->>DC: append_hmac_entry(verdict)
      else Layer 4: User escalation (CategoricalExclusion or genuinely novel)
        SUP->>SSE: send(IronclawHitlEscalationEvent{nonce, traceparent, ...})
        SSE-->>OP: GET /api/builds/:id/hitl-stream (EventSource)
        OP-->>SSE: POST /api/builds/:id/hitl-resolve {verdict, nonce}
        SSE-->>SUP: HitlResolution
        SUP->>DC: append_hmac_entry(user_verdict)
      end
    end
  end

  PROG-->>LB: BuildResult::Complete
  LB-->>OP: build complete
```
