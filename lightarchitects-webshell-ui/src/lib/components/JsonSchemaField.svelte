<script lang="ts">
  // Recursive form field renderer for JSON Schema common-subset.
  // Supported types: string, integer, number, boolean, object (recursive, depth ≤ MAX),
  // array (comma-separated string → split on submit), enum (select).
  // Unknown types fall back to a plain text input that passes through as-is.

  import { MAX_SCHEMA_DEPTH, type FieldDescriptor } from '$lib/mcp-schema';

  interface Props {
    field: FieldDescriptor;
    value: unknown;
    depth?: number;
    onchange: (key: string, value: unknown) => void;
  }

  let { field, value, depth = 0, onchange }: Props = $props();

  let childValues = $state<Map<string, unknown>>(new Map());

  function handleInput(e: Event) {
    const raw = (e.target as HTMLInputElement).value;
    if (field.type === 'integer') {
      const n = parseInt(raw, 10);
      onchange(field.key, isNaN(n) ? raw : n);
    } else if (field.type === 'number') {
      const n = parseFloat(raw);
      onchange(field.key, isNaN(n) ? raw : n);
    } else {
      onchange(field.key, raw);
    }
  }

  function handleCheck(e: Event) {
    onchange(field.key, (e.target as HTMLInputElement).checked);
  }

  function handleSelect(e: Event) {
    onchange(field.key, (e.target as HTMLSelectElement).value);
  }

  function handleChild(childKey: string, childValue: unknown) {
    childValues.set(childKey, childValue);
    // Propagate assembled child object upward
    const assembled: Record<string, unknown> = {};
    for (const [k, v] of childValues) assembled[k] = v;
    onchange(field.key, assembled);
  }

  const inputId = `field-${field.key}-${depth}`;
  const descId = field.description ? `desc-${field.key}-${depth}` : undefined;
</script>

<div class="field" class:nested={depth > 0}>
  {#if field.type === 'boolean'}
    <label class="bool-label" for={inputId}>
      <input
        id={inputId}
        type="checkbox"
        checked={value === true || field.default === true}
        aria-describedby={descId}
        onchange={handleCheck}
      />
      <span class="field-name">{field.key}</span>
      {#if field.required}<span class="required" aria-hidden="true">*</span>{/if}
    </label>

  {:else if field.type === 'enum'}
    <label class="field-label" for={inputId}>
      <span class="field-name">{field.key}</span>
      {#if field.required}<span class="required" aria-hidden="true">*</span>{/if}
    </label>
    <select
      id={inputId}
      class="field-select"
      aria-describedby={descId}
      aria-required={field.required}
      value={typeof value === 'string' ? value : (field.default ?? field.enumValues[0] ?? '')}
      onchange={handleSelect}
    >
      {#if !field.required}
        <option value="">— optional —</option>
      {/if}
      {#each field.enumValues as opt (opt)}
        <option value={opt}>{opt}</option>
      {/each}
    </select>

  {:else if field.type === 'object' && depth < MAX_SCHEMA_DEPTH}
    <fieldset class="nested-fieldset">
      <legend class="field-name">
        {field.key}
        {#if field.required}<span class="required" aria-hidden="true">*</span>{/if}
      </legend>
      {#each field.properties as child (child.key)}
        <svelte:self
          field={child}
          value={childValues.get(child.key) ?? child.default}
          depth={depth + 1}
          onchange={handleChild}
        />
      {/each}
    </fieldset>

  {:else if field.type === 'array'}
    <!-- Arrays rendered as comma-separated text; caller splits on submit -->
    <label class="field-label" for={inputId}>
      <span class="field-name">{field.key}</span>
      {#if field.required}<span class="required" aria-hidden="true">*</span>{/if}
      <span class="type-hint">comma-separated {field.itemType}s</span>
    </label>
    <input
      id={inputId}
      type="text"
      class="field-input"
      placeholder="item1, item2, ..."
      value={Array.isArray(value) ? value.join(', ') : (value ?? '')}
      aria-describedby={descId}
      aria-required={field.required}
      oninput={handleInput}
    />

  {:else}
    <!-- string, integer, number, unknown -->
    <label class="field-label" for={inputId}>
      <span class="field-name">{field.key}</span>
      {#if field.required}<span class="required" aria-hidden="true">*</span>{/if}
      {#if field.type === 'integer' || field.type === 'number'}
        <span class="type-hint">{field.type}</span>
      {/if}
    </label>
    <input
      id={inputId}
      type={field.type === 'integer' || field.type === 'number' ? 'number' : 'text'}
      step={field.type === 'integer' ? '1' : 'any'}
      class="field-input"
      placeholder={String(field.default ?? '')}
      value={value ?? field.default ?? ''}
      aria-describedby={descId}
      aria-required={field.required}
      oninput={handleInput}
    />
  {/if}

  {#if field.description}
    <p id={descId} class="field-desc">{field.description}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-bottom: 10px;
  }

  .field.nested {
    padding-left: 12px;
    border-left: 2px solid var(--border-subtle, #222);
    margin-left: 4px;
  }

  .field-label,
  .bool-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.78rem;
    color: var(--text-muted, #888);
  }

  .field-name {
    color: var(--text, #e4e4e7);
    font-weight: 500;
  }

  .required {
    color: var(--danger, #ef4444);
    font-size: 0.85em;
  }

  .type-hint {
    font-size: 0.68rem;
    color: var(--text-muted, #888);
    font-style: italic;
  }

  .field-input,
  .field-select {
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 0.8rem;
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    color: inherit;
    padding: 5px 8px;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }

  .field-input:focus,
  .field-select:focus {
    border-color: var(--accent, #818cf8);
  }

  .nested-fieldset {
    border: 1px solid var(--border-subtle, #222);
    border-radius: 4px;
    padding: 8px 12px;
    margin: 0;
  }

  .nested-fieldset legend {
    font-size: 0.78rem;
    color: var(--text-muted, #888);
    padding: 0 4px;
  }

  .field-desc {
    font-size: 0.7rem;
    color: var(--text-muted, #888);
    margin: 0;
    line-height: 1.4;
  }
</style>
