<!--
@component
Form for launching an ironclaw autonomous build.

POSTs to POST /api/builds with { cwd, mode: "autonomous", waves, northstar_text? }.

`waves` is a JSON array of arrays of TaskSpec:
  [[{ id, prompt, depends_on?, file_ownership?, concurrency_safe? }, …], …]

Each inner array is one parallel wave; waves run sequentially.

Props:
- `onLaunched` — callback(buildId: string) after successful creation
-->
<script lang="ts">
  import { authHeaders } from '$lib/auth';

  let { onLaunched }: { onLaunched?: (buildId: string) => void } = $props();

  let cwd           = $state('');
  let northstar     = $state('');
  let wavesJson     = $state('');
  let submitting    = $state(false);
  let error         = $state('');
  let parseError    = $state('');

  const PLACEHOLDER = JSON.stringify([
    [{ id: 'phase-1', prompt: 'Implement feature X in src/lib.rs', file_ownership: ['src/lib.rs'] }],
    [{ id: 'phase-2', prompt: 'Add tests for feature X', depends_on: ['phase-1'] }],
  ], null, 2);

  function validateWaves(raw: string): unknown[] | null {
    try {
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) { parseError = 'waves must be a JSON array'; return null; }
      for (const wave of parsed) {
        if (!Array.isArray(wave)) { parseError = 'each wave must be an array of tasks'; return null; }
        for (const task of wave) {
          if (typeof task.id !== 'string' || !task.id) {
            parseError = `task missing "id" field`;
            return null;
          }
          if (typeof task.prompt !== 'string' || !task.prompt) {
            parseError = `task "${task.id}" missing "prompt" field`;
            return null;
          }
        }
      }
      parseError = '';
      return parsed;
    } catch {
      parseError = 'invalid JSON';
      return null;
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (submitting) return;

    const waves = validateWaves(wavesJson.trim() || '[]');
    if (waves === null) return;

    if (!cwd.trim()) { error = '"Working directory" is required'; return; }

    submitting = true;
    error = '';
    try {
      const body: Record<string, unknown> = {
        cwd:  cwd.trim(),
        mode: 'autonomous',
        waves,
      };
      if (northstar.trim()) body.northstar_text = northstar.trim();

      const res = await fetch('/api/builds', {
        method:  'POST',
        headers: { ...authHeaders(), 'Content-Type': 'application/json' },
        body:    JSON.stringify(body),
      });
      if (!res.ok) {
        error = `${res.status} — ${await res.text().catch(() => 'server error')}`;
        return;
      }
      const data = await res.json() as { build_id: string };
      onLaunched?.(data.build_id);
    } catch (e) {
      error = e instanceof Error ? e.message : 'launch failed';
    } finally {
      submitting = false;
    }
  }
</script>

<form class="sf" onsubmit={submit} data-testid="start-form" novalidate>
  <div class="sf-header">
    <span class="sf-label">START AUTONOMOUS BUILD</span>
  </div>

  <div class="sf-field">
    <label class="sf-field-label" for="sf-cwd">Working Directory</label>
    <input
      id="sf-cwd"
      class="sf-input"
      type="text"
      placeholder="/Users/kft/Projects/my-project"
      bind:value={cwd}
      disabled={submitting}
      required
      aria-label="Working directory"
    />
  </div>

  <div class="sf-field">
    <label class="sf-field-label" for="sf-northstar">Northstar (optional)</label>
    <input
      id="sf-northstar"
      class="sf-input"
      type="text"
      placeholder="One sentence describing the build goal"
      bind:value={northstar}
      disabled={submitting}
      aria-label="Northstar text"
    />
  </div>

  <div class="sf-field sf-field-grow">
    <label class="sf-field-label" for="sf-waves">
      Wave Plan (JSON)
      {#if parseError}
        <span class="sf-parse-error">{parseError}</span>
      {/if}
    </label>
    <textarea
      id="sf-waves"
      class="sf-textarea"
      rows="8"
      placeholder={PLACEHOLDER}
      bind:value={wavesJson}
      disabled={submitting}
      aria-label="Wave plan JSON"
      class:sf-textarea-error={!!parseError}
    ></textarea>
    <span class="sf-hint">
      Array of waves. Each wave = array of tasks ({'{'}id, prompt, depends_on?, file_ownership?{'}'}).
    </span>
  </div>

  {#if error}
    <p class="sf-error" role="alert">{error}</p>
  {/if}

  <div class="sf-footer">
    <button
      type="submit"
      class="sf-btn-launch"
      disabled={submitting || !!parseError}
    >
      {submitting ? 'LAUNCHING…' : 'LAUNCH BUILD'}
    </button>
  </div>
</form>

<style>
  .sf {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
  }

  .sf-header {
    display: flex;
    align-items: center;
  }

  .sf-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-label);
  }

  .sf-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .sf-field-grow {
    flex: 1;
  }

  .sf-field-label {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sf-parse-error {
    font-size: 8px;
    color: var(--la-semantic-error);
    font-weight: 500;
  }

  .sf-input {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-hair-base);
    border-radius: 3px;
    color: var(--la-text-base);
    padding: 6px 8px;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }

  .sf-input:focus {
    border-color: var(--la-focus-ring);
  }

  .sf-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .sf-textarea {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-hair-base);
    border-radius: 3px;
    color: var(--la-text-base);
    padding: 6px 8px;
    resize: vertical;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }

  .sf-textarea:focus {
    border-color: var(--la-focus-ring);
  }

  .sf-textarea.sf-textarea-error {
    border-color: var(--la-semantic-error);
  }

  .sf-textarea:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .sf-hint {
    font-size: 8px;
    color: var(--la-text-dim);
    font-style: italic;
  }

  .sf-error {
    font-size: 10px;
    color: var(--la-semantic-error);
    margin: 0;
  }

  .sf-footer {
    display: flex;
    justify-content: flex-end;
  }

  .sf-btn-launch {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 6px 22px;
    border: 1px solid var(--la-focus-ring);
    border-radius: 3px;
    color: var(--la-focus-ring);
    background: transparent;
    cursor: pointer;
    transition: background 150ms;
  }

  .sf-btn-launch:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-focus-ring) 14%, transparent);
  }

  .sf-btn-launch:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
