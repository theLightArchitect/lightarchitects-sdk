<!--
  @component FileRow
  @description Single file entry in FilesDrawer. Click = set activeFileId; double-click = open hero.
  @contract none — displays FileEntry; no SSE consumed directly
  @reads lightspaceFilesStore.files (via prop)
  @mutates lightspaceFilesStore.activeFileId (click), .heroFileId (double-click)
  @api GET /api/builds/:id/files/:fid — on hero open for file content
-->
<script lang="ts">
  import type { FileEntry } from '$lib/lightspace-types';
  import { lightspaceFilesStore } from '$lib/lightspace-stores';
  import { agentDomain } from '$lib/lightspace/vocab';

  interface Props { file: FileEntry; active: boolean; }
  let { file, active }: Props = $props();

  const MIME_COLOR: Partial<Record<string, string>> = {
    md: 'var(--ls-acc-purple)', rs: 'var(--ls-acc-red)', ts: 'var(--ls-acc)',
    svg: 'var(--ls-acc-amber)', yaml: 'var(--ls-acc-green)', json: 'var(--ls-acc)',
  };
</script>

<div
  class="ls-file-row"
  class:ls-file-active={active}
  onclick={() => lightspaceFilesStore.update(s => ({ ...s, activeFileId: file.id }))}
  ondblclick={() => lightspaceFilesStore.update(s => ({ ...s, heroFileId: file.id }))}
  role="button"
  tabindex="0"
  onkeydown={(e) => e.key === 'Enter' && lightspaceFilesStore.update(s => ({ ...s, activeFileId: file.id }))}
>
  <span class="ls-file-mime" style="color: {MIME_COLOR[file.mime] ?? 'var(--ls-text-mute)'}">
    {file.mime.toUpperCase()}
  </span>
  <div class="ls-file-info">
    <div class="ls-file-name">{file.name}</div>
    <div class="ls-file-meta">{agentDomain(file.prov.agent)} · {file.meta}</div>
  </div>
</div>

<style>
.ls-file-row {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 7px; cursor: pointer;
  transition: background var(--ls-fast);
}
.ls-file-row:hover, .ls-file-active { background: rgba(77,142,255,0.08); }
.ls-file-mime {
  font-family: var(--ls-font-display); font-weight: 700;
  font-size: 7px; letter-spacing: var(--ls-tk-loose);
  border: 1px solid currentColor; padding: 2px 4px; opacity: 0.7;
  white-space: nowrap; flex-shrink: 0;
}
.ls-file-info { flex: 1; min-width: 0; }
.ls-file-name { font-size: 10px; color: var(--ls-text-bright); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ls-file-meta { font-size: 9px; color: var(--ls-text-mute); margin-top: 1px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
