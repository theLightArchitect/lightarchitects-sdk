<!--
  @component Artifact — shipped file card.
  @reads content as { path?: string; mime?: string; commit_sha?: string }
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  type D = { path?: string; mime?: string; commit_sha?: string };
  const d = $derived((data as D | null) ?? {});
</script>
<div class="ls-art-wrap">
  <div class="ls-art-path">{d.path ?? '—'}</div>
  <div class="ls-art-meta">
    {#if d.mime}<span class="ls-art-badge">{d.mime}</span>{/if}
    {#if d.commit_sha}<span class="ls-art-sha">{d.commit_sha.slice(0, 7)}</span>{/if}
  </div>
</div>
<style>
.ls-art-wrap { display: flex; flex-direction: column; gap: 5px; }
.ls-art-path { font-size: 10px; font-family: var(--ls-font-code); color: var(--ls-text); word-break: break-all; }
.ls-art-meta { display: flex; gap: 4px; flex-wrap: wrap; }
.ls-art-badge { font-size: 8px; padding: 1px 4px; background: var(--ls-sunken); border: 1px solid var(--ls-border); color: var(--ls-text-dim); border-radius: 2px; }
.ls-art-sha   { font-size: 8px; color: var(--ls-acc); font-family: var(--ls-font-code); }
</style>
