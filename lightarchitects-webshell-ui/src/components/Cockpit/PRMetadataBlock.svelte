<script lang="ts">
  import { authHeaders } from '$lib/auth';

  interface PrMeta {
    number: number;
    title: string;
    html_url: string;
    owner: string;
    repo: string;
    author: string;
    state: string;
    draft: boolean;
    head_sha: string;
    updated_at: string;
  }

  interface CommitMeta {
    sha: string;
    message: string;
    author_login: string;
    committed_at: string;
  }

  interface Props {
    owner: string;
    repo: string;
    prNumber: number;
    onHeadSha?: (sha: string) => void;
  }

  let { owner, repo, prNumber, onHeadSha }: Props = $props();

  let prMeta    = $state<PrMeta | null>(null);
  let commitMeta = $state<CommitMeta | null>(null);
  let loading   = $state(false);
  let error     = $state('');

  async function load(o: string, r: string, n: number) {
    loading = true;
    error = '';
    prMeta = null;
    commitMeta = null;
    try {
      const params = new URLSearchParams({ owner: o, repo: r, number: String(n) });
      const res = await fetch(`/api/gitforest/pr-metadata?${params}`, { headers: authHeaders() });
      if (!res.ok) { error = `PR metadata: ${res.status}`; return; }
      const meta: PrMeta = await res.json();
      prMeta = meta;
      onHeadSha?.(meta.head_sha);

      const cRes = await fetch(`/api/github-proxy/commits/${o}/${r}/${meta.head_sha}`, { headers: authHeaders() });
      if (cRes.ok) commitMeta = await cRes.json();
    } catch (e) {
      error = e instanceof Error ? e.message : 'fetch failed';
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void load(owner, repo, prNumber);
  });

  function ageLabel(updatedAt: string): string {
    const h = (Date.now() - new Date(updatedAt).getTime()) / 3_600_000;
    if (h < 1)  return `${Math.ceil(h * 60)}m ago`;
    if (h < 24) return `${Math.floor(h)}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  function shortSha(sha: string): string {
    return sha.slice(0, 8);
  }
</script>

{#if loading}
  <div class="meta-loading">loading PR…</div>
{:else if error}
  <div class="meta-error">{error}</div>
{:else if prMeta}
  <div class="meta-block">
    <div class="meta-title">
      {#if prMeta.draft}<span class="meta-draft">DRAFT</span>{/if}
      <span class="meta-pr-num">#{prMeta.number}</span>
      <span class="meta-pr-title">{prMeta.title}</span>
    </div>

    <div class="meta-row">
      <span class="meta-key">REPO</span>
      <span class="meta-val">{prMeta.owner}/{prMeta.repo}</span>
      <span class="meta-key">AUTHOR</span>
      <span class="meta-val">@{prMeta.author}</span>
      <span class="meta-key">UPDATED</span>
      <span class="meta-val">{ageLabel(prMeta.updated_at)}</span>
    </div>

    <div class="meta-row">
      <span class="meta-key">HEAD</span>
      <code class="meta-sha">{shortSha(prMeta.head_sha)}</code>
      {#if commitMeta}
        <span class="meta-key">MSG</span>
        <span class="meta-commit-msg">{commitMeta.message}</span>
        {#if commitMeta.author_login}
          <span class="meta-key">BY</span>
          <span class="meta-val">@{commitMeta.author_login}</span>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .meta-loading, .meta-error {
    font-size: 9px;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    padding: 4px 0;
  }

  .meta-error { color: var(--la-semantic-error); }

  .meta-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .meta-title {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }

  .meta-draft {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.07em;
    padding: 1px 4px;
    border: 1px solid var(--la-text-mute);
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
  }

  .meta-pr-num {
    font-size: 11px;
    font-weight: 700;
    color: var(--la-struct-primary);
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
  }

  .meta-pr-title {
    font-size: 11px;
    color: var(--la-text-bright);
    font-family: var(--la-font-mono, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
  }

  .meta-key {
    color: var(--la-text-mute);
    font-weight: 700;
    letter-spacing: 0.06em;
    flex-shrink: 0;
  }

  .meta-val {
    color: var(--la-text-base);
    flex-shrink: 0;
  }

  .meta-sha {
    color: var(--la-struct-primary);
    background: color-mix(in srgb, var(--la-struct-primary) 8%, transparent);
    padding: 1px 4px;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    flex-shrink: 0;
  }

  .meta-commit-msg {
    color: var(--la-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 240px;
  }
</style>
