<script lang="ts">
  // HITL card for the native LA `question` tool (webshell-hitl-bridge).
  //
  // The gateway long-polls POST /api/question; when the operator submits here,
  // POST /api/question/:id/answer unblocks the gateway and the card is removed
  // from `pendingQuestions` by the parent overlay.

  import { authHeaders } from '$lib/auth';
  import type { QuestionItemState } from '$lib/stores';

  interface Props {
    toolUseId: string;
    questions: QuestionItemState[];
    /** Called after the answer is successfully delivered to the gateway. */
    onAnswered: () => void;
  }

  let { toolUseId, questions, onAnswered }: Props = $props();

  // One string[] per question — empty until at least one option is selected.
  let selectedAnswers = $state<string[][]>(questions.map(() => []));
  let submitting = $state(false);
  let error = $state<string | null>(null);

  function handleSingle(qIdx: number, label: string) {
    selectedAnswers[qIdx] = [label];
  }

  function handleMulti(qIdx: number, label: string, checked: boolean) {
    const cur = selectedAnswers[qIdx];
    selectedAnswers[qIdx] = checked
      ? [...cur, label]
      : cur.filter(l => l !== label);
  }

  // Require at least one selection per question before enabling submit.
  const allAnswered = $derived(questions.every((_, i) => selectedAnswers[i].length > 0));

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!allAnswered || submitting) return;
    submitting = true;
    error = null;
    try {
      const resp = await fetch(`/api/question/${toolUseId}/answer`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...authHeaders(),
        },
        body: JSON.stringify({ answers: selectedAnswers }),
      });
      if (!resp.ok) {
        error = `Failed to submit answer (HTTP ${resp.status})`;
        return;
      }
      onAnswered();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }
</script>

<div
  class="question-card"
  role="dialog"
  aria-modal="true"
  aria-label="Agent question — response required"
  data-testid="question-card"
  data-tool-use-id={toolUseId}
>
  <header class="card-header">
    <span class="card-badge" aria-hidden="true">?</span>
    <h3 class="card-title">Agent Question</h3>
  </header>

  <form class="card-body" onsubmit={submit}>
    {#each questions as q, i (i)}
      <fieldset class="question-fieldset">
        <legend class="question-legend">
          <span class="q-header">{q.header}</span>
          <span class="q-text">{q.question}</span>
          {#if q.multiSelect}
            <span class="q-hint">Select all that apply</span>
          {/if}
        </legend>

        {#each q.options as opt (opt.label)}
          {@const isChecked = selectedAnswers[i].includes(opt.label)}
          <label
            class="option-label"
            class:selected={isChecked}
            data-testid="question-option"
            data-option-label={opt.label}
          >
            <input
              type={q.multiSelect ? 'checkbox' : 'radio'}
              name={`q-${toolUseId}-${i}`}
              value={opt.label}
              checked={isChecked}
              onchange={(e) => {
                if (q.multiSelect) {
                  handleMulti(i, opt.label, (e.target as HTMLInputElement).checked);
                } else {
                  handleSingle(i, opt.label);
                }
              }}
            />
            <span class="opt-label">{opt.label}</span>
            {#if opt.description}
              <span class="opt-desc">{opt.description}</span>
            {/if}
          </label>
        {/each}
      </fieldset>
    {/each}

    {#if error}
      <p class="card-error" role="alert">{error}</p>
    {/if}

    <div class="card-actions">
      <button
        type="submit"
        class="btn-submit"
        disabled={submitting || !allAnswered}
        aria-busy={submitting}
        data-testid="question-submit"
      >
        {submitting ? 'Submitting…' : 'Submit Answer'}
      </button>
    </div>
  </form>
</div>

<style>
  .question-card {
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--accent, #818cf8);
    border-radius: 8px;
    padding: 16px;
    width: 100%;
    max-width: 480px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.5);
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--accent, #818cf8);
    color: #fff;
    font-size: 0.7rem;
    font-weight: 700;
    flex-shrink: 0;
  }

  .card-title {
    margin: 0;
    font-size: 0.88rem;
    font-weight: 600;
    color: var(--text, #e4e4e7);
  }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .question-fieldset {
    border: 1px solid var(--border, #333);
    border-radius: 6px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .question-legend {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 4px;
  }

  .q-header {
    font-size: 0.65rem;
    color: var(--accent, #818cf8);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }

  .q-text {
    font-size: 0.85rem;
    color: var(--text, #e4e4e7);
    line-height: 1.4;
  }

  .q-hint {
    font-size: 0.68rem;
    color: var(--text-muted, #888);
    font-style: italic;
  }

  .option-label {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 5px;
    border: 1px solid var(--border, #333);
    cursor: pointer;
    background: var(--surface-2, #262626);
    transition: border-color 0.1s, background 0.1s;
  }

  .option-label:hover {
    border-color: var(--accent, #818cf8);
    background: rgba(129, 140, 248, 0.06);
  }

  .option-label.selected {
    border-color: var(--accent, #818cf8);
    background: rgba(129, 140, 248, 0.12);
  }

  .option-label input {
    margin-top: 2px;
    flex-shrink: 0;
    accent-color: var(--accent, #818cf8);
  }

  .opt-label {
    font-size: 0.82rem;
    font-weight: 500;
    color: var(--text, #e4e4e7);
    line-height: 1.3;
  }

  .opt-desc {
    font-size: 0.72rem;
    color: var(--text-muted, #888);
    line-height: 1.4;
    display: block;
    margin-top: 1px;
  }

  .card-error {
    font-size: 0.75rem;
    color: var(--danger, #ef4444);
    background: rgba(239, 68, 68, 0.08);
    border-radius: 4px;
    padding: 6px 10px;
    margin: 0;
  }

  .card-actions {
    display: flex;
    justify-content: flex-end;
  }

  .btn-submit {
    font-size: 0.8rem;
    font-family: inherit;
    font-weight: 600;
    padding: 7px 18px;
    border-radius: 5px;
    border: none;
    background: var(--accent, #818cf8);
    color: #fff;
    cursor: pointer;
    transition: filter 0.1s;
  }

  .btn-submit:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-submit:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
