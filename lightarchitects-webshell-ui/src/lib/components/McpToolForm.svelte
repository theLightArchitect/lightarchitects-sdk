<script lang="ts">
  // Form wrapper for a single MCP tool invocation.
  // Generates a form from the tool's input_schema (common subset) or falls back
  // to a raw-JSON textarea (R-C, R-L mitigations).
  // On submit: validates, assembles args, calls /api/mcp/invoke.

  import JsonSchemaField from './JsonSchemaField.svelte';
  import SchemaRawJsonFallback from './SchemaRawJsonFallback.svelte';
  import {
    schemaToFields,
    assembleArgs,
    needsRawFallback,
    type FieldDescriptor,
  } from '$lib/mcp-schema';
  import { invokeMcpTool } from '$lib/mcp-client';

  interface Props {
    server: string;
    toolName: string;
    description: string;
    /** Raw JSON Schema for the tool's input — may be undefined if the server didn't provide one. */
    inputSchema: unknown;
    oncancel: () => void;
    onsuccess: (output: unknown) => void;
  }

  let { server, toolName, description, inputSchema, oncancel, onsuccess }: Props = $props();

  const useRaw = $derived(needsRawFallback(inputSchema));
  const fields: FieldDescriptor[] = $derived(useRaw ? [] : schemaToFields(inputSchema));

  let fieldValues = $state<Map<string, unknown>>(new Map());
  let rawJson = $state('{}');
  let rawValid = $state(true);
  let rawParsed = $state<Record<string, unknown> | null>({});
  let submitting = $state(false);
  let error = $state<string | null>(null);

  function handleFieldChange(key: string, value: unknown) {
    fieldValues.set(key, value);
  }

  function handleRawChange(parsed: Record<string, unknown> | null, raw: string, valid: boolean) {
    rawJson = raw;
    rawParsed = parsed;
    rawValid = valid;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (useRaw && !rawValid) return;

    submitting = true;
    error = null;

    try {
      const input: Record<string, unknown> = useRaw
        ? (rawParsed ?? {})
        : assembleArgs(fields, fieldValues);

      const output = await invokeMcpTool({ server, tool: toolName, input });
      onsuccess(output);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="tool-form" role="dialog" aria-modal="true" aria-label="Invoke {toolName}">
  <header class="form-header">
    <div class="tool-meta">
      <span class="server-tag">{server}</span>
      <h3 class="tool-name">{toolName}</h3>
    </div>
    <button class="close-btn" aria-label="Cancel" onclick={oncancel}>✕</button>
  </header>

  {#if description}
    <p class="tool-desc">{description}</p>
  {/if}

  <form class="form-body" onsubmit={submit}>
    {#if useRaw}
      <SchemaRawJsonFallback
        value={rawJson}
        onchange={handleRawChange}
        label="Tool arguments"
      />
    {:else if fields.length === 0}
      <p class="no-args">This tool takes no arguments.</p>
    {:else}
      {#each fields as field (field.key)}
        <JsonSchemaField
          {field}
          value={fieldValues.get(field.key) ?? field.default}
          onchange={handleFieldChange}
        />
      {/each}
    {/if}

    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}

    <div class="form-actions">
      <button type="button" class="btn-cancel" onclick={oncancel} disabled={submitting}>
        Cancel
      </button>
      <button
        type="submit"
        class="btn-invoke"
        disabled={submitting || (useRaw && !rawValid)}
        aria-busy={submitting}
      >
        {submitting ? 'Invoking…' : 'Invoke'}
      </button>
    </div>
  </form>
</div>

<style>
  .tool-form {
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 16px;
    max-width: 520px;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .form-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
  }

  .tool-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .server-tag {
    font-size: 0.68rem;
    color: var(--accent, #818cf8);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .tool-name {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--text, #e4e4e7);
  }

  .close-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted, #888);
    font-size: 0.9rem;
    padding: 2px 6px;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: var(--text, #e4e4e7);
    background: var(--surface-2, #262626);
  }

  .tool-desc {
    font-size: 0.78rem;
    color: var(--text-muted, #888);
    margin: 0;
    line-height: 1.5;
  }

  .form-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .no-args {
    font-size: 0.78rem;
    color: var(--text-muted, #888);
    margin: 0;
    font-style: italic;
  }

  .form-error {
    font-size: 0.75rem;
    color: var(--danger, #ef4444);
    margin: 0;
    padding: 6px 10px;
    background: rgba(239, 68, 68, 0.08);
    border-radius: 4px;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }

  .btn-cancel,
  .btn-invoke {
    font-size: 0.8rem;
    padding: 6px 14px;
    border-radius: 5px;
    border: 1px solid transparent;
    cursor: pointer;
    font-family: inherit;
  }

  .btn-cancel {
    background: transparent;
    border-color: var(--border, #333);
    color: var(--text-muted, #888);
  }

  .btn-cancel:hover:not(:disabled) {
    color: var(--text, #e4e4e7);
  }

  .btn-invoke {
    background: var(--accent, #818cf8);
    color: #fff;
    font-weight: 600;
  }

  .btn-invoke:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-invoke:disabled,
  .btn-cancel:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
