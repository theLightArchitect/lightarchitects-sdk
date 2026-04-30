<script lang="ts">
  const MAX_BYTES = 8 * 1024;

  interface Props {
    task?: string;
    dry?: boolean;
    disabled?: boolean;
    onSubmit?: (task: string, dry: boolean) => void;
    onTaskChange?: (task: string) => void;
  }

  let {
    task = $bindable(''),
    dry = $bindable(false),
    disabled = false,
    onSubmit,
    onTaskChange,
  }: Props = $props();

  const byteCount = $derived(new TextEncoder().encode(task).length);
  const overLimit = $derived(byteCount > MAX_BYTES);
  const canSubmit = $derived(task.trim().length > 0 && !overLimit && !disabled);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && canSubmit) {
      e.preventDefault();
      onSubmit?.(task.trim(), dry);
    }
  }

  function handleInput() {
    onTaskChange?.(task);
  }

  function submit() {
    if (canSubmit) onSubmit?.(task.trim(), dry);
  }
</script>

<div class="flex flex-col gap-2">
  <div class="relative">
    <textarea
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

  <div class="flex items-center justify-between gap-2">
    <label class="flex items-center gap-1.5 cursor-pointer select-none">
      <input
        type="checkbox"
        bind:checked={dry}
        {disabled}
        class="accent-[#f59e0b] w-3 h-3"
      />
      <span class="text-[10px] text-[#94a3b8]">Dry run</span>
      <span class="text-[9px] text-[#475569]">(no writes)</span>
    </label>

    <button
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

  {#if overLimit}
    <p class="text-[9px] text-[#ef4444]">Task exceeds 8 KB limit — trim before dispatching.</p>
  {/if}
</div>
