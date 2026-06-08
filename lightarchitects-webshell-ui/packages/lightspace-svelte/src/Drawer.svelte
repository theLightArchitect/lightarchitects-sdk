<!--
  @component Drawer — file list + HITL escalation queue.
  @reads drawerStore (Map<string, DrawerFileData>) from ./stores
         hitlStore via HitlQueue component
  @contract HitlQueue renders amber age-pills for pending escalations (visual layer #7).
-->
<script lang="ts">
  import { drawerStore } from './stores';
  import HitlQueue from './components/HitlQueue.svelte';

  const files = $derived([...$drawerStore.values()]);

  const MIME_ICON: Record<string, string> = {
    'text/markdown':   '📄',
    'image/svg+xml':   '🖼',
    'application/json':'{}',
    'text/plain':      '📝',
    'text/x-rust':     '🦀',
    'text/typescript': 'TS',
  };
</script>

<aside class="ls-drawer">
  <HitlQueue />

  <div class="ls-drawer-section">
    <div class="ls-drawer-section-hd">files ({files.length})</div>
    <ul class="ls-drawer-files">
      {#each files as f (f.id)}
        <li class="ls-drawer-file" title="{f.content_uri}">
          <span class="ls-drawer-icon" aria-hidden="true">{MIME_ICON[f.mime_type] ?? '📁'}</span>
          <span class="ls-drawer-uri">{f.content_uri.split('/').at(-1) ?? f.id}</span>
          <span class="ls-drawer-mime">{f.mime_type.split('/').at(-1)}</span>
        </li>
      {/each}
      {#if files.length === 0}
        <li class="ls-drawer-empty">no files</li>
      {/if}
    </ul>
  </div>
</aside>

<style>
.ls-drawer { display: flex; flex-direction: column; gap: 8px; width: 220px; min-height: 0; overflow-y: auto; padding: 8px; border-left: 1px solid var(--ls-border); background: var(--ls-card); }
.ls-drawer-section { display: flex; flex-direction: column; gap: 4px; }
.ls-drawer-section-hd { font-size: 8px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--ls-text-mute); padding: 2px 0; border-bottom: 1px solid var(--ls-border); }
.ls-drawer-files { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 3px; }
.ls-drawer-file  { display: flex; align-items: center; gap: 5px; padding: 3px 4px; border-radius: 3px; cursor: default; }
.ls-drawer-file:hover { background: var(--ls-sunken); }
.ls-drawer-icon  { font-size: 10px; }
.ls-drawer-uri   { font-size: 9px; color: var(--ls-text); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--ls-font-code); }
.ls-drawer-mime  { font-size: 7px; color: var(--ls-text-ghost); }
.ls-drawer-empty { font-size: 9px; color: var(--ls-text-ghost); padding: 4px; }
</style>
