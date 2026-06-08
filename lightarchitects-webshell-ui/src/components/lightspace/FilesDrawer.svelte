<!--
  @component FilesDrawer
  @description "Files & Diagrams" collapsible subdrawer. Auto-opens on first file graduation.
  @contract EventType 'impl_complete' (commit_sha → infer files) → ImplCompleteEvent
  @reads lightspaceFilesStore.files, lightspaceUiStore.filesDrawerOpen
  @mutates lightspaceUiStore.filesDrawerOpen (toggle), lightspaceFilesStore.activeFileId (FileRow click)
  @api GET /api/builds/:id/files (for file list); GET /api/builds/:id/files/:fid (for content)
  @mockup-ref arch/lightspace-mockup.html → #la-subdrawer, .la-file-row, renderFiles()
-->
<script lang="ts">
  import { lightspaceFilesStore, lightspaceUiStore, filesCount } from '$lib/lightspace-stores';
  import FileRow from './FileRow.svelte';
</script>

<div class="ls-subdrawer" class:ls-subdrawer-open={$lightspaceUiStore.filesDrawerOpen}>
  <button
    class="ls-subdrawer-head"
    onclick={() => lightspaceUiStore.update(s => ({ ...s, filesDrawerOpen: !s.filesDrawerOpen }))}
  >
    <span class="ls-subdrawer-chev">{$lightspaceUiStore.filesDrawerOpen ? '▾' : '▸'}</span>
    <span>Files &amp; Diagrams</span>
    <span class="ls-subdrawer-count">{$filesCount} file{$filesCount === 1 ? '' : 's'}</span>
  </button>

  {#if $lightspaceUiStore.filesDrawerOpen && $lightspaceFilesStore.files.length > 0}
    <div class="ls-subdrawer-body">
      {#each $lightspaceFilesStore.files as file (file.id)}
        <FileRow {file} active={$lightspaceFilesStore.activeFileId === file.id} />
      {/each}
    </div>
  {/if}
</div>

<style>
.ls-subdrawer { border-top: 1px solid var(--ls-border-base); }
.ls-subdrawer-head {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px;
  font-size: 10px; letter-spacing: var(--ls-tk-mid); text-transform: uppercase;
  color: var(--ls-text-dim); cursor: pointer; width: 100%;
  background: transparent; border: 0; text-align: left;
  font-family: var(--ls-font-code); transition: color var(--ls-fast);
}
.ls-subdrawer-head:hover { color: var(--ls-text-bright); }
.ls-subdrawer-chev { font-size: 9px; color: var(--ls-text-mute); }
.ls-subdrawer-count { margin-left: auto; color: var(--ls-text-mute); font-size: 9px; }
.ls-subdrawer-body { padding: 4px 8px 8px; }
</style>
