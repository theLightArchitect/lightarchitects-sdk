<script lang="ts">
  // Raw-JSON textarea fallback for MCP tool schemas with unsupported constructs
  // ($ref, oneOf, anyOf, allOf) or object schemas with no declared properties.
  // Validates JSON.parse() on blur; emits parsed value via onchange.

  interface Props {
    value: string;
    onchange: (parsed: Record<string, unknown> | null, raw: string, valid: boolean) => void;
    label?: string;
  }

  let { value = $bindable(''), onchange, label = 'Tool input (raw JSON)' }: Props = $props();

  let error = $state<string | null>(null);

  function validate(raw: string) {
    if (raw.trim() === '' || raw.trim() === '{}') {
      error = null;
      onchange({}, raw, true);
      return;
    }
    try {
      const parsed = JSON.parse(raw) as unknown;
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        error = 'Input must be a JSON object';
        onchange(null, raw, false);
        return;
      }
      error = null;
      onchange(parsed as Record<string, unknown>, raw, true);
    } catch {
      error = 'Invalid JSON';
      onchange(null, raw, false);
    }
  }

  function onblur() {
    validate(value);
  }

  function oninput(e: Event) {
    value = (e.target as HTMLTextAreaElement).value;
    // clear error on edit so user gets feedback only on blur
    if (error) error = null;
  }
</script>

<div class="raw-fallback">
  <label for="raw-json-input" class="raw-label">
    {label}
    <span class="badge">advanced schema — raw JSON</span>
  </label>
  <textarea
    id="raw-json-input"
    class="raw-textarea"
    class:invalid={error !== null}
    placeholder="&#123;&#125;"
    spellcheck="false"
    autocomplete="off"
    rows="6"
    aria-label={label}
    aria-describedby={error ? 'raw-json-error' : undefined}
    aria-invalid={error !== null}
    {value}
    {oninput}
    {onblur}
  ></textarea>
  {#if error}
    <p id="raw-json-error" class="raw-error" role="alert">{error}</p>
  {/if}
</div>

<style>
  .raw-fallback {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .raw-label {
    font-size: 0.75rem;
    color: var(--text-muted, #888);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .badge {
    font-size: 0.65rem;
    background: var(--warning-bg, rgba(245, 158, 11, 0.15));
    color: var(--warning, #f59e0b);
    border-radius: 3px;
    padding: 1px 5px;
  }

  .raw-textarea {
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 0.8rem;
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    color: inherit;
    padding: 8px;
    resize: vertical;
    width: 100%;
    box-sizing: border-box;
    outline: none;
  }

  .raw-textarea:focus {
    border-color: var(--accent, #818cf8);
  }

  .raw-textarea.invalid {
    border-color: var(--danger, #ef4444);
  }

  .raw-error {
    font-size: 0.72rem;
    color: var(--danger, #ef4444);
    margin: 0;
  }
</style>
