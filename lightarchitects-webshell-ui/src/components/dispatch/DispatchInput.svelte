<script lang="ts">
  import {
    type FileAttachment,
    MAX_ATTACHMENT_BYTES,
    MAX_TOTAL_BYTES,
  } from '$lib/dispatch';

  const MAX_BYTES = 8 * 1024;

  interface Props {
    task?: string;
    dry?: boolean;
    disabled?: boolean;
    attachments?: FileAttachment[];
    onSubmit?: (task: string, dry: boolean, attachments: FileAttachment[]) => void;
    onTaskChange?: (task: string) => void;
  }

  let {
    task = $bindable(''),
    dry = $bindable(false),
    disabled = false,
    attachments = $bindable<FileAttachment[]>([]),
    onSubmit,
    onTaskChange,
  }: Props = $props();

  let fileInput: HTMLInputElement | null = null;
  let folderInput: HTMLInputElement | null = null;
  let attachError = $state<string | null>(null);

  const byteCount = $derived(new TextEncoder().encode(task).length);
  const overLimit = $derived(byteCount > MAX_BYTES);
  const canSubmit = $derived(task.trim().length > 0 && !overLimit && !disabled);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && canSubmit) {
      e.preventDefault();
      onSubmit?.(task.trim(), dry, attachments);
    }
  }

  function handleInput() {
    onTaskChange?.(task);
  }

  function submit() {
    if (canSubmit) onSubmit?.(task.trim(), dry, attachments);
  }

  async function readFiles(files: FileList) {
    attachError = null;
    const incoming: FileAttachment[] = [];
    let totalBytes = attachments.reduce((s, a) => s + new TextEncoder().encode(a.content).length, 0);

    for (const file of Array.from(files)) {
      if (!file.type.startsWith('text/') && !file.name.match(/\.(ts|tsx|js|jsx|svelte|rs|toml|json|md|html|css|scss|yaml|yml|sh|py|go|txt|env|lock|conf|cfg)$/i)) {
        continue;
      }
      const bytes = file.size;
      if (bytes > MAX_ATTACHMENT_BYTES) {
        attachError = `${file.name} exceeds 50 KB — skipped.`;
        continue;
      }
      if (totalBytes + bytes > MAX_TOTAL_BYTES) {
        attachError = 'Total attachment size exceeds 300 KB — some files skipped.';
        break;
      }
      const content = await file.text();
      const path = (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name;
      incoming.push({ name: file.name, path, content });
      totalBytes += bytes;
    }

    attachments = [...attachments, ...incoming];
  }

  function onFilePick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    if (input.files?.length) {
      void readFiles(input.files);
      input.value = '';
    }
  }

  function removeAttachment(idx: number) {
    attachments = attachments.filter((_, i) => i !== idx);
  }
</script>

<div class="flex flex-col gap-2" data-testid="dispatch-input">
  <div class="relative">
    <textarea
      data-testid="dispatch-task-input"
      bind:value={task}
      {disabled}
      placeholder="Describe the task for the squad… (⌘↩ to dispatch)"
      rows={4}
      class="w-full bg-[#0f172a] border rounded px-3 py-2 text-[11px] text-[#e2e8f0]
             placeholder-[#475569] outline-none resize-none font-mono leading-relaxed
             transition-colors
             {overLimit
               ? 'border-[#ef4444]'
               : disabled
                 ? 'border-[#1e293b] opacity-50 cursor-not-allowed'
                 : 'border-[#1e293b] focus:border-[#3b82f6]'}"
      oninput={handleInput}
      onkeydown={handleKeydown}
    ></textarea>
    <span
      class="absolute bottom-1.5 right-2 text-[9px] tabular-nums
             {overLimit ? 'text-[#ef4444]' : 'text-[#475569]'}"
    >
      {byteCount} / {MAX_BYTES.toLocaleString()}B
    </span>
  </div>

  <!-- File attachment chips -->
  {#if attachments.length > 0}
    <div class="flex flex-wrap gap-1" data-testid="attachment-chips">
      {#each attachments as att, i}
        <span class="flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px]
                     bg-[#1e293b] border border-[#334155] text-[#94a3b8] font-mono">
          {att.name}
          {#if !disabled}
            <button
              type="button"
              onclick={() => removeAttachment(i)}
              class="text-[#475569] hover:text-[#ef4444] leading-none"
              aria-label="Remove {att.name}"
            >×</button>
          {/if}
        </span>
      {/each}
    </div>
  {/if}

  {#if attachError}
    <p class="text-[9px] text-[#f59e0b]">{attachError}</p>
  {/if}

  <!-- Hidden file inputs -->
  <input
    bind:this={fileInput}
    type="file"
    multiple
    accept=".ts,.tsx,.js,.jsx,.svelte,.rs,.toml,.json,.md,.html,.css,.scss,.yaml,.yml,.sh,.py,.go,.txt,.env,.lock,.conf,.cfg,text/*"
    class="hidden"
    onchange={onFilePick}
    data-testid="file-input"
  />
  <input
    bind:this={folderInput}
    type="file"
    webkitdirectory
    class="hidden"
    onchange={onFilePick}
    data-testid="folder-input"
  />

  <div class="flex items-center justify-between gap-2">
    <label class="flex items-center gap-1.5 cursor-pointer select-none">
      <input
        data-testid="dispatch-dry-toggle"
        type="checkbox"
        bind:checked={dry}
        {disabled}
        class="accent-[#f59e0b] w-3 h-3"
      />
      <span class="text-[10px] text-[#94a3b8]">Dry run</span>
      <span class="text-[9px] text-[#475569]">(no writes)</span>
    </label>

    <div class="flex items-center gap-1">
      <!-- Attach files button -->
      <button
        type="button"
        onclick={() => fileInput?.click()}
        {disabled}
        title="Attach files"
        class="px-2 py-1 text-[10px] rounded border border-[#1e293b]
               text-[#475569] hover:border-[#334155] hover:text-[#94a3b8]
               transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      >
        + Files
      </button>
      <!-- Attach folder button -->
      <button
        type="button"
        onclick={() => folderInput?.click()}
        {disabled}
        title="Attach folder"
        class="px-2 py-1 text-[10px] rounded border border-[#1e293b]
               text-[#475569] hover:border-[#334155] hover:text-[#94a3b8]
               transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      >
        + Folder
      </button>

      <button
        data-testid="dispatch-submit"
        onclick={submit}
        disabled={!canSubmit}
        class="px-3 py-1 text-[10px] font-medium rounded transition-colors
               {canSubmit
                 ? 'bg-[#3b82f6] text-white hover:bg-[#2563eb]'
                 : 'bg-[#1e293b] text-[#475569] cursor-not-allowed'}"
      >
        {disabled ? 'Dispatching…' : 'Dispatch'}
      </button>
    </div>
  </div>

  {#if overLimit}
    <p class="text-[9px] text-[#ef4444]">Task exceeds 8 KB limit — trim before dispatching.</p>
  {/if}
</div>
