# SEQ-1 — Wave Dispatch with Real Ollama Worker

> Canon XLI: Architect-authored design input. Phase 1 deliverable.

```mermaid
sequenceDiagram
  autonumber
  participant PROG as Program::run()
  participant WD as WaveDispatcher
  participant WM as WorktreeManager
  participant S3 as OllamaCloudCodingProvider\n(SLOT 3)
  participant ORV as OllamaResponseValidator
  participant OC as Ollama Cloud\n(qwen3-coder:480b)
  participant MA as MergeAgent
  participant RG as ReviewGate

  PROG->>WD: dispatch_wave(wave_tasks)
  WD->>WM: create_task_worktrees(tasks)
  WM-->>WD: Vec<WorktreePath>

  par For each task in wave (JoinSet concurrent)
    WD->>S3: spawn(task, worktree, system_prompt_file)
    Note over S3: Injects §66 context bundle:\n  - plan section\n  - file_ownership\n  - wave siblings\n  - pre_dispatch_sha
    S3->>OC: POST /api/chat {\n  model: "qwen3-coder:480b-cloud",\n  messages: [{role:"system",...},{role:"user",task_prompt}],\n  stream: true\n}\nAuthorization: Bearer {OLLAMA_API_KEY}
    OC-->>S3: NDJSON stream\n{"message":{"content":"..."}, "done":false}\n...
    S3->>S3: accumulate_stream() → raw_diff
    S3->>ORV: validate(raw_diff, task.file_ownership)
    alt Valid diff
      ORV-->>S3: ValidationResult::Ok { diff_bytes }
      S3->>S3: git apply + commit in task worktree
      S3-->>WD: TaskResult::Complete(task_id, commit_sha)
    else Validation violation
      ORV-->>S3: ValidationResult::Violation { kind, detail }
      S3-->>WD: TaskResult::Failed(task_id, ValidationError)
      Note over WD: FixAgent dispatched (max 3 retries)
    end
  end

  WD->>MA: merge_completed(task_branches → feat/<codename>)
  Note over MA: Arc<Mutex> serialized; jittered exp backoff on conflict
  MA-->>PROG: MergeResult

  PROG->>RG: evaluate_wave(wave_artifacts)
  RG-->>PROG: GateResult::Pass
```
