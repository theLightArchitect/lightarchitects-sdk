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
      .replace(/^### (.+)$/gm, '<h3 class="text-sm font-semibold text-[#e2e8f0]">$1</h3>')
      .replace(/^## (.+)$/gm, '<h2 class="text-sm font-semibold text-[#e2e8f0] mt-2">$1</h2>')
      .replace(/^# (.+)$/gm, '<h1 class="text-base font-bold text-[#e2e8f0]">$1</h1>')
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.+?)\*/g, '<em>$1</em>')
      .replace(/`([^`]+)`/g, '<code class="bg-[#1e293b] px-1 rounded text-[#e2e8f0] text-[10px]">$1</code>')
      .replace(/^- \[ \] (.+)$/gm, '<div class="flex items-center gap-1"><span class="text-[#475569]">☐</span><span>$1</span></div>')
      .replace(/^- \[x\] (.+)$/gm, '<div class="flex items-center gap-1"><span class="text-[#22c55e]">☑</span><span class="line-through text-[#475569]">$1</span></div>')
      .replace(/^- (.+)$/gm, '<div class="flex items-start gap-1"><span class="text-[#FFD700]">•</span><span>$1</span></div>')
      .replace(/\n\n/g, '</p><p class="text-[11px] text-[#94a3b8]">')
      .replace(/\n/g, '<br>');
  }
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#94a3b8]">BUILD NOTES</h3>
    <div class="flex items-center gap-2">
      {#if isEditing}
        <button
          onclick={saveEdit}
          class="text-[10px] px-2 py-0.5 rounded bg-[#22c55e]/10 text-[#22c55e] hover:bg-[#22c55e]/20 transition-colors"
        >
          Save
        </button>
        <button
          onclick={cancelEdit}
          class="text-[10px] px-2 py-0.5 rounded bg-[#ef4444]/10 text-[#ef4444] hover:bg-[#ef4444]/20 transition-colors"
        >
          Cancel
        </button>
      {:else}
        <button
          onclick={startEdit}
          class="text-[10px] px-2 py-0.5 rounded bg-[#FFD700]/10 text-[#FFD700] hover:bg-[#FFD700]/20 transition-colors"
        >
          Edit
        </button>
        <button
          onclick={() => previewMode = !previewMode}
          class="text-[10px] px-2 py-0.5 rounded bg-[#1e293b] text-[#64748b] hover:text-[#94a3b8] transition-colors"
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
        class="w-full h-48 bg-[#0a0a0a] border border-[#1e293b] rounded px-3 py-2 text-xs text-[#e2e8f0] font-mono placeholder-[#475569] outline-none focus:border-[#FFD700] resize-y"
        placeholder="Write notes in Markdown…&#10;&#10;# Heading&#10;## Subheading&#10;- List item&#10;- [ ] Todo&#10;**bold** *italic* `code`"
      ></textarea>
    </div>
  {:else if content}
    {#if previewMode}
      <div class="p-3 prose-invert text-[11px] text-[#94a3b8]">
        {@html renderMarkdown(content)}
      </div>
    {:else}
      <div class="p-3">
        <pre class="text-[11px] text-[#94a3b8] whitespace-pre-wrap font-mono">{content}</pre>
      </div>
    {/if}
  {:else}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[#475569]">No notes yet</p>
      <button
        onclick={startEdit}
        class="text-[10px] text-[#FFD700] hover:text-[#9F67FF] mt-1 transition-colors"
      >
        Add build notes
      </button>
    </div>
  {/if}
</div>