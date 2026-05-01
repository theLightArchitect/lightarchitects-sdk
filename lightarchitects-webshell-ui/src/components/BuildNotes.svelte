<script lang="ts">
  import { buildNotes, notesEditing } from '$lib/stores';

  interface Props {
    buildId: string;
    onSave?: (content: string) => void;
  }

  let { buildId, onSave }: Props = $props();

  let content = $derived($buildNotes[buildId]?.content ?? '');
  let isEditing = $derived($notesEditing);
  let editContent = $state('');
  let previewMode = $state(false);

  function startEdit() {
    editContent = content;
    notesEditing.set(true);
  }

  function cancelEdit() {
    editContent = '';
    notesEditing.set(false);
  }

  function saveEdit() {
    buildNotes.update(notes => ({
      ...notes,
      [buildId]: {
        buildId,
        content: editContent,
        updatedAt: new Date().toISOString(),
      },
    }));
    onSave?.(editContent);
    notesEditing.set(false);
    editContent = '';
  }

  // Simple markdown-to-HTML for preview (bold, italic, headings, lists, links, code)
  function renderMarkdown(md: string): string {
    return md
      .replace(/^### (.+)$/gm, '<h3 class="text-sm font-semibold text-[var(--la-text-bright)]">$1</h3>')
      .replace(/^## (.+)$/gm, '<h2 class="text-sm font-semibold text-[var(--la-text-bright)] mt-2">$1</h2>')
      .replace(/^# (.+)$/gm, '<h1 class="text-base font-bold text-[var(--la-text-bright)]">$1</h1>')
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.+?)\*/g, '<em>$1</em>')
      .replace(/`([^`]+)`/g, '<code class="bg-[var(--la-drawer-border)] px-1 rounded text-[var(--la-text-bright)] text-[10px]">$1</code>')
      .replace(/^- \[ \] (.+)$/gm, '<div class="flex items-center gap-1"><span class="text-[var(--la-text-dim)]">☐</span><span>$1</span></div>')
      .replace(/^- \[x\] (.+)$/gm, '<div class="flex items-center gap-1"><span class="text-[var(--la-agent-researcher)]">☑</span><span class="line-through text-[var(--la-text-dim)]">$1</span></div>')
      .replace(/^- (.+)$/gm, '<div class="flex items-start gap-1"><span class="text-[var(--la-focus-ring)]">•</span><span>$1</span></div>')
      .replace(/\n\n/g, '</p><p class="text-[11px] text-[var(--la-text-label)]">')
      .replace(/\n/g, '<br>');
  }
</script>

<div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden">
  <div class="px-4 py-2 border-b border-[var(--la-drawer-border)] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[var(--la-text-label)]">BUILD NOTES</h3>
    <div class="flex items-center gap-2">
      {#if isEditing}
        <button
          onclick={saveEdit}
          class="text-[10px] px-2 py-0.5 rounded bg-[var(--la-agent-researcher)]/10 text-[var(--la-agent-researcher)] hover:bg-[var(--la-agent-researcher)]/20 transition-colors"
        >
          Save
        </button>
        <button
          onclick={cancelEdit}
          class="text-[10px] px-2 py-0.5 rounded bg-[var(--la-danger-stroke)]/10 text-[var(--la-danger-stroke)] hover:bg-[var(--la-danger-stroke)]/20 transition-colors"
        >
          Cancel
        </button>
      {:else}
        <button
          onclick={startEdit}
          class="text-[10px] px-2 py-0.5 rounded bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)] hover:bg-[var(--la-focus-ring)]/20 transition-colors"
        >
          Edit
        </button>
        <button
          onclick={() => previewMode = !previewMode}
          class="text-[10px] px-2 py-0.5 rounded bg-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:text-[var(--la-text-label)] transition-colors"
        >
          {previewMode ? 'Source' : 'Preview'}
        </button>
      {/if}
    </div>
  </div>

  {#if isEditing}
    <div class="p-3">
      <textarea
        bind:value={editContent}
        class="w-full h-48 bg-[var(--la-bg-void)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-xs text-[var(--la-text-bright)] font-mono placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)] resize-y"
        placeholder="Write notes in Markdown…&#10;&#10;# Heading&#10;## Subheading&#10;- List item&#10;- [ ] Todo&#10;**bold** *italic* `code`"
      ></textarea>
    </div>
  {:else if content}
    {#if previewMode}
      <div class="p-3 prose-invert text-[11px] text-[var(--la-text-label)]">
        {@html renderMarkdown(content)}
      </div>
    {:else}
      <div class="p-3">
        <pre class="text-[11px] text-[var(--la-text-label)] whitespace-pre-wrap font-mono">{content}</pre>
      </div>
    {/if}
  {:else}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[var(--la-text-dim)]">No notes yet</p>
      <button
        onclick={startEdit}
        class="text-[10px] text-[var(--la-focus-ring)] hover:text-[var(--la-agent-testing)] mt-1 transition-colors"
      >
        Add build notes
      </button>
    </div>
  {/if}
</div>