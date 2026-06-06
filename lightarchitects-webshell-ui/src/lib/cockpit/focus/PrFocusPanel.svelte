<script lang="ts">
  interface Props { owner: string; repo: string; number: number; }
  let { owner, repo, number }: Props = $props();

  const prTitle = $derived(`${owner}/${repo}#${number}`);
  const githubUrl = $derived(`https://github.com/${owner}/${repo}/pull/${number}`);
</script>

<div class="focus-panel" data-focus-kind="pr">
  <header class="focus-hdr">
    <span class="focus-kind">PULL REQUEST</span>
    <span class="focus-pr-num">#{number}</span>
  </header>
  <section class="focus-body">
    <div class="field-row">
      <span class="field-label">REPO</span>
      <span class="field-value field-mono">{owner}/{repo}</span>
    </div>
    <div class="field-row">
      <span class="field-label">PR</span>
      <span class="field-value">{prTitle}</span>
    </div>
    <div class="pr-actions">
      <a
        class="pr-link"
        href={githubUrl}
        target="_blank"
        rel="noopener noreferrer"
        aria-label="Open {prTitle} on GitHub"
      >OPEN ON GITHUB ↗</a>
    </div>
  </section>
</div>

<style>
  .focus-panel { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  .focus-hdr {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid var(--la-hair-base, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }
  .focus-kind { font-size: 8px; font-weight: 700; letter-spacing: 0.14em; color: var(--scope-accent, #4da6ff); opacity: 0.7; }
  .focus-pr-num { font-family: var(--font-mono, monospace); font-size: 11px; font-weight: 700; color: var(--scope-accent, #4da6ff); }
  .focus-body { flex: 1; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
  .field-row { display: flex; align-items: baseline; gap: 8px; }
  .field-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); min-width: 72px; flex-shrink: 0; }
  .field-value { font-size: 10px; color: var(--la-text-dim, #888); }
  .field-mono { font-family: var(--font-mono, monospace); font-size: 9px; }
  .pr-actions { margin-top: 4px; }
  .pr-link {
    font-size: 8px; font-weight: 700; letter-spacing: 0.1em;
    color: var(--scope-accent, #4da6ff); text-decoration: none;
    border-bottom: 1px solid var(--scope-accent, #4da6ff); opacity: 0.8;
  }
  .pr-link:hover { opacity: 1; }
</style>
