<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { codeStore } from '$lib/stores';

  interface Props {
    path: string;
    content: string;
    language?: string;
    onChange?: (value: string) => void;
    readonly?: boolean;
  }

  let { path, content, language = 'plaintext', onChange, readonly = false }: Props = $props();

  let containerEl = $state<HTMLDivElement | undefined>(undefined);
  let editor: any = null;
  let monacoModule: any = null;
  let loadError = $state<string | null>(null);

  function detectLanguage(p: string): string {
    const ext = p.split('.').pop()?.toLowerCase() ?? '';
    const map: Record<string, string> = {
      rs: 'rust', ts: 'typescript', tsx: 'typescript', js: 'javascript',
      jsx: 'javascript', svelte: 'html', json: 'json', toml: 'toml',
      yaml: 'yaml', yml: 'yaml', md: 'markdown', sh: 'shell',
      py: 'python', go: 'go', css: 'css', html: 'html',
    };
    return map[ext] ?? 'plaintext';
  }

  const resolvedLanguage = $derived(language !== 'plaintext' ? language : detectLanguage(path));

  onMount(async () => {
    try {
      if (!containerEl) return;
      monacoModule = await import('monaco-editor');
      const monaco = monacoModule;

      editor = monaco.editor.create(containerEl, {
        value: content,
        language: resolvedLanguage,
        theme: 'vs-dark',
        readOnly: readonly,
        minimap: { enabled: false },
        fontSize: 13,
        fontFamily: "'JetBrains Mono Variable', 'JetBrains Mono', monospace",
        lineNumbers: 'on',
        wordWrap: 'off',
        scrollBeyondLastLine: false,
        automaticLayout: true,
        tabSize: 2,
        renderWhitespace: 'selection',
      });
      // automaticLayout fires on resize events only — force a layout pass on the
      // next paint to handle containers that start with 0 computed height.
      requestAnimationFrame(() => { editor?.layout(); });

      editor.onDidChangeModelContent(() => {
        const val = editor.getValue();
        onChange?.(val);
        codeStore.update(buf => buf ? { ...buf, content: val } : buf);
      });
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'Failed to load editor';
    }
  });

  onDestroy(() => {
    editor?.dispose();
  });

  // Sync external content changes into the editor without resetting cursor.
  $effect(() => {
    if (!editor) return;
    const currentVal = editor.getValue();
    if (currentVal !== content) {
      const pos = editor.getPosition();
      editor.setValue(content);
      if (pos) editor.setPosition(pos);
    }
  });

  // Update language when path changes.
  $effect(() => {
    if (!editor || !monacoModule) return;
    const model = editor.getModel();
    if (model) {
      monacoModule.editor.setModelLanguage(model, resolvedLanguage);
    }
  });
</script>

<div class="editor-wrap" data-testid="code-editor">
  {#if loadError}
    <div class="editor-error">
      <span>Editor failed to load: {loadError}</span>
      <pre class="editor-fallback">{content}</pre>
    </div>
  {:else}
    <div bind:this={containerEl} class="editor-container"></div>
  {/if}
</div>

<style>
  .editor-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .editor-container {
    flex: 1;
    height: 100%;
    min-height: 0;
  }

  .editor-error {
    padding: 16px;
    color: var(--la-agent-security, #f55);
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 12px;
  }

  .editor-fallback {
    margin-top: 8px;
    white-space: pre-wrap;
    color: var(--la-text-dim, #888);
    font-size: 11px;
    overflow: auto;
  }
</style>
