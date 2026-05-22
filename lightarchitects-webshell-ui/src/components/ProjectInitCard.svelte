<script lang="ts">
  import type { ProjectInitRequest } from '$lib/types';

  interface Props {
    slug: string;
    onInit: (req: ProjectInitRequest) => void;
    onCancel: () => void;
    loading?: boolean;
    error?: string;
  }

  let { slug, onInit, onCancel, loading = false, error }: Props = $props();

  let name = $state('');

  function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (loading) return;
    onInit({ slug, name: name.trim() || undefined });
  }
</script>

<div class="init-card" role="region" aria-label="Initialize project">
  <div class="init-card__icon">📁</div>
  <h3 class="init-card__heading">Project not initialized</h3>
  <p class="init-card__body">
    No <code>.lightarchitects/project.toml</code> found in
    <code>~/Projects/{slug}</code>. Initialize it to start tracking this project.
  </p>

  <form class="init-card__form" onsubmit={handleSubmit}>
    <label class="init-card__label" for="init-name">
      Display name <span class="init-card__optional">(optional — defaults to slug)</span>
    </label>
    <input
      id="init-name"
      class="init-card__input"
      type="text"
      bind:value={name}
      placeholder={slug}
      disabled={loading}
      autocomplete="off"
    />

    {#if error}
      <p class="init-card__error" role="alert">{error}</p>
    {/if}

    <div class="init-card__actions">
      <button
        type="button"
        class="init-card__btn init-card__btn--cancel"
        onclick={onCancel}
        disabled={loading}
      >
        Cancel
      </button>
      <button
        type="submit"
        class="init-card__btn init-card__btn--primary"
        disabled={loading}
        aria-busy={loading}
      >
        {loading ? 'Initializing…' : 'Initialize project'}
      </button>
    </div>
  </form>
</div>

<style>
  .init-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 2rem 1.5rem;
    background: var(--surface-2, #1a1a2e);
    border: 1px dashed var(--border, #3a3a5c);
    border-radius: 8px;
    text-align: center;
    max-width: 480px;
    margin: 2rem auto;
  }

  .init-card__icon {
    font-size: 2.5rem;
  }

  .init-card__heading {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-1, #e0e0ff);
  }

  .init-card__body {
    margin: 0;
    font-size: 0.875rem;
    color: var(--text-2, #8888aa);
    line-height: 1.5;
  }

  .init-card__form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
  }

  .init-card__label {
    font-size: 0.8125rem;
    color: var(--text-2, #8888aa);
  }

  .init-card__optional {
    color: var(--text-3, #555577);
  }

  .init-card__input {
    width: 100%;
    padding: 0.4rem 0.6rem;
    background: var(--surface-1, #0f0f1e);
    border: 1px solid var(--border, #3a3a5c);
    border-radius: 4px;
    color: var(--text-1, #e0e0ff);
    font-size: 0.875rem;
    box-sizing: border-box;
  }

  .init-card__input:focus {
    outline: 2px solid var(--accent, #6060cc);
    outline-offset: -1px;
  }

  .init-card__input:disabled {
    opacity: 0.5;
  }

  .init-card__error {
    margin: 0;
    font-size: 0.8125rem;
    color: var(--error, #ff6b6b);
  }

  .init-card__actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .init-card__btn {
    padding: 0.4rem 0.875rem;
    border-radius: 4px;
    font-size: 0.875rem;
    cursor: pointer;
    border: 1px solid transparent;
  }

  .init-card__btn--cancel {
    background: transparent;
    border-color: var(--border, #3a3a5c);
    color: var(--text-2, #8888aa);
  }

  .init-card__btn--cancel:hover:not(:disabled) {
    background: var(--surface-1, #0f0f1e);
  }

  .init-card__btn--primary {
    background: var(--accent, #6060cc);
    color: #fff;
  }

  .init-card__btn--primary:hover:not(:disabled) {
    background: var(--accent-hover, #7070dd);
  }

  .init-card__btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
