<script lang="ts">
  type Provider   = 'github' | 'gitlab';
  type StorageTier = 'keychain' | 'session' | 'none';

  interface Props {
    open?: boolean;
    onclose?: () => void;
  }

  let { open = $bindable(false), onclose }: Props = $props();

  // ─── Provider catalogue ──────────────────────────────────────────────────

  const PROVIDERS = {
    github: {
      label:       'GITHUB',
      placeholder: 'ghp_xxxxxxxxxxxxxxxxxxxx',
      scopes:      ['repo', 'read:org'],
      steps: [
        'github.com / Settings / Developer settings',
        'Personal access tokens / Tokens (classic)',
        'Generate new token / select scopes below',
        'Paste token here — shown only once by GitHub',
      ],
      validate: (t: string) =>
        t.startsWith('ghp_') || t.startsWith('github_pat_') || t.length >= 40,
    },
    gitlab: {
      label:       'GITLAB',
      placeholder: 'glpat-xxxxxxxxxxxxxxxxxxxx',
      scopes:      ['read_repository', 'read_api'],
      steps: [
        'gitlab.com / Profile / Preferences / Access Tokens',
        'Or: your-instance/-/profile/personal_access_tokens',
        'Set expiry > 1 year and add scopes below',
        'Paste token here — shown only once by GitLab',
      ],
      validate: (t: string) => t.startsWith('glpat-') || t.length >= 20,
    },
  } as const;

  // ─── State ───────────────────────────────────────────────────────────────

  let activeProvider  = $state<Provider>('github');
  let tokenInput      = $state('');
  let saving          = $state(false);
  let showSteps       = $state(false);
  let feedback        = $state<{ ok: boolean; text: string } | null>(null);
  let gatewayOnline   = $state(false);
  let githubTier      = $state<StorageTier>('none');
  let gitlabTier      = $state<StorageTier>('none');

  // ─── Derived ─────────────────────────────────────────────────────────────

  let info       = $derived(PROVIDERS[activeProvider]);
  let tier       = $derived(activeProvider === 'github' ? githubTier : gitlabTier);
  let inputValid = $derived(tokenInput.length === 0 || info.validate(tokenInput));

  function setTier(p: Provider, t: StorageTier) {
    if (p === 'github') githubTier = t; else gitlabTier = t;
  }

  // ─── Bootstrap on open ───────────────────────────────────────────────────

  $effect(() => {
    if (!open) { tokenInput = ''; feedback = null; showSteps = false; return; }
    void loadStatus();
  });

  async function loadStatus() {
    // Check gateway liveness
    try {
      const r = await fetch('/v1/platform/health', { signal: AbortSignal.timeout(1500) });
      gatewayOnline = r.ok;
    } catch { gatewayOnline = false; }

    if (gatewayOnline) {
      try {
        const r = await fetch('/v1/platform/tokens/status');
        if (r.ok) {
          const d = await r.json() as { github?: StorageTier; gitlab?: StorageTier };
          githubTier = d.github ?? 'none';
          gitlabTier = d.gitlab ?? 'none';
          return;
        }
      } catch { /* endpoint not yet implemented in gateway */ }
    }

    githubTier = sessionStorage.getItem('la_token_github') ? 'session' : 'none';
    gitlabTier = sessionStorage.getItem('la_token_gitlab') ? 'session' : 'none';
  }

  // ─── Actions ─────────────────────────────────────────────────────────────

  async function saveToken() {
    const raw = tokenInput.trim();
    if (!raw || !inputValid) return;

    saving  = true;
    feedback = null;
    tokenInput = '';  // remove from JS state immediately — never keep longer than needed

    try {
      if (gatewayOnline) {
        try {
          const r = await fetch(`/v1/platform/tokens/${activeProvider}`, {
            method:  'POST',
            headers: { 'Content-Type': 'application/json' },
            body:    JSON.stringify({ token: raw }),
          });
          if (r.ok) {
            const d = await r.json() as { stored_in?: StorageTier };
            const t: StorageTier = d.stored_in ?? 'keychain';
            setTier(activeProvider, t);
            feedback = {
              ok:   true,
              text: t === 'keychain'
                ? 'Stored in OS Keychain — token never touches disk unencrypted'
                : 'Stored for this session',
            };
            return;
          }
        } catch { /* gateway endpoint not implemented yet — fall through */ }
      }

      // Session fallback — labelled clearly
      sessionStorage.setItem(`la_token_${activeProvider}`, raw);
      setTier(activeProvider, 'session');
      feedback = {
        ok:   false,
        text: gatewayOnline
          ? 'Gateway token endpoint not ready — session storage used (clears on tab close)'
          : 'Gateway offline — session storage used (clears on tab close)',
      };
    } finally {
      saving = false;
    }
  }

  async function revoke(p: Provider) {
    if (gatewayOnline) {
      try { await fetch(`/v1/platform/tokens/${p}`, { method: 'DELETE' }); }
      catch { /* ignore */ }
    }
    sessionStorage.removeItem(`la_token_${p}`);
    setTier(p, 'none');
    feedback = { ok: true, text: `${PROVIDERS[p].label} token removed` };
  }

  // ─── Keyboard / close ────────────────────────────────────────────────────

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    if (e.key === 'Enter' && !e.shiftKey && tokenInput.length > 0 && inputValid && !saving) {
      void saveToken();
    }
  }

  function onBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) close();
  }

  function close() {
    open       = false;
    tokenInput = '';
    feedback   = null;
    showSteps  = false;
    onclose?.();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="vault-scrim" onclick={onBackdrop}>
    <div class="vault-panel" role="dialog" aria-modal="true" aria-label="Token Vault">

      <!-- ── Header ─────────────────────────────────────────────────────── -->
      <div class="vault-hd">
        <span class="vault-title">TOKEN VAULT</span>
        <span class="gw-pill" class:online={gatewayOnline}>
          <span class="gw-dot"></span>
          {gatewayOnline ? 'GATEWAY ONLINE' : 'GATEWAY OFFLINE'}
        </span>
        <button class="vault-x" onclick={close} aria-label="Close">×</button>
      </div>

      <!-- ── Security tier legend ───────────────────────────────────────── -->
      <div class="tier-legend">
        <span class="tier-node keychain">KEYCHAIN</span>
        <span class="tier-arrow">›</span>
        <span class="tier-node session">SESSION</span>
        <span class="tier-arrow">›</span>
        <span class="tier-node none">NONE</span>
        <span class="tier-desc">— most secure to least; gateway required for keychain</span>
      </div>

      <!-- ── Provider tabs ──────────────────────────────────────────────── -->
      <div class="provider-tabs">
        {#each (['github', 'gitlab'] as Provider[]) as p}
          {@const t = p === 'github' ? githubTier : gitlabTier}
          <button
            class="ptab"
            class:active={activeProvider === p}
            onclick={() => { activeProvider = p; feedback = null; }}
          >
            {PROVIDERS[p].label}
            <span class="ptab-badge tier-{t}">{t === 'none' ? '—' : t.toUpperCase()}</span>
          </button>
        {/each}
      </div>

      <!-- ── Body ───────────────────────────────────────────────────────── -->
      <div class="vault-body">

        <!-- Current status when configured -->
        {#if tier !== 'none'}
          <div class="status-card tier-{tier}">
            <div class="status-card-inner">
              <span class="status-dot"></span>
              <div>
                <div class="status-heading">
                  {tier === 'keychain' ? 'Stored in OS Keychain' : 'Stored in session'}
                </div>
                <div class="status-sub">
                  {#if tier === 'keychain'}
                    Encrypted by your operating system — most secure option
                  {:else}
                    Cleared when you close this tab — connect the gateway for keychain storage
                  {/if}
                </div>
              </div>
            </div>
            <button class="revoke-btn" onclick={() => revoke(activeProvider)}>REVOKE</button>
          </div>
        {/if}

        <!-- Token input section -->
        <div class="input-section">
          <div class="input-hd">
            <span class="input-label">
              {tier !== 'none' ? 'REPLACE TOKEN' : 'PERSONAL ACCESS TOKEN'}
            </span>
            <button
              class="steps-toggle"
              onclick={() => showSteps = !showSteps}
            >
              {showSteps ? 'HIDE STEPS' : 'HOW TO GENERATE'}
            </button>
          </div>

          {#if showSteps}
            <div class="steps-block">
              <ol class="step-list">
                {#each info.steps as step, i}
                  <li><span class="step-n">{i + 1}</span>{step}</li>
                {/each}
              </ol>
              <div class="scopes-row">
                <span class="scopes-lbl">Required scopes:</span>
                {#each info.scopes as scope}
                  <code class="scope-badge">{scope}</code>
                {/each}
              </div>
            </div>
          {/if}

          <div class="input-row">
            <input
              type="password"
              class="token-input"
              class:invalid={tokenInput.length > 0 && !inputValid}
              placeholder={info.placeholder}
              autocomplete="off"
              spellcheck="false"
              bind:value={tokenInput}
            />
            <button
              class="save-btn"
              class:keychain-mode={gatewayOnline}
              disabled={saving || tokenInput.length === 0 || !inputValid}
              onclick={saveToken}
            >
              {#if saving}SAVING…{:else if gatewayOnline}STORE IN KEYCHAIN{:else}SAVE FOR SESSION{/if}
            </button>
          </div>

          {#if tokenInput.length > 0 && !inputValid}
            <div class="hint err">Token format looks incorrect for {info.label}</div>
          {:else if !gatewayOnline && tokenInput.length > 0}
            <div class="hint warn">
              Gateway offline — token will be session-only.
              Run <code class="ic">lightarchitects gateway start</code> for keychain storage.
            </div>
          {/if}
        </div>

        <!-- Feedback -->
        {#if feedback}
          <div class="feedback" class:ok={feedback.ok} class:warn={!feedback.ok}>
            {feedback.text}
          </div>
        {/if}

        <!-- Gateway offline callout -->
        {#if !gatewayOnline}
          <div class="gw-callout">
            <span class="gw-callout-hd">KEYCHAIN UNAVAILABLE</span>
            The lightarchitects gateway is not running. Token will be stored in session
            memory only and cleared when you close this tab.
            <br>
            <code class="ic">lightarchitects gateway start</code>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Backdrop ─────────────────────────────────────────────────────────── */
  .vault-scrim {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal-scrim);
    background: var(--la-bg-overlay);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* ── Panel ────────────────────────────────────────────────────────────── */
  .vault-panel {
    z-index: var(--z-modal);
    width: min(480px, 94vw);
    background: var(--la-bg-panel);
    border: 1px solid var(--la-hair-strong);
    box-shadow: 0 0 0 1px rgba(0, 200, 255, 0.08),
                0 20px 60px rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
  }

  /* ── Header ───────────────────────────────────────────────────────────── */
  .vault-hd {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--la-bg-elev-1);
    border-bottom: 1px solid var(--la-hair-strong);
    flex-shrink: 0;
  }
  .vault-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-struct-primary);
    flex: 1;
  }
  .gw-pill {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-semantic-offline);
    border: 1px solid currentColor;
    padding: 2px 6px;
    opacity: 0.7;
  }
  .gw-pill.online { color: var(--la-semantic-ok); opacity: 1; }
  .gw-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }
  .vault-x {
    font-size: 16px;
    line-height: 1;
    color: var(--la-text-dim);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0 2px;
    transition: color var(--la-t-snap);
  }
  .vault-x:hover { color: var(--la-text-stark); }

  /* ── Tier legend ──────────────────────────────────────────────────────── */
  .tier-legend {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 12px;
    background: var(--la-bg-void);
    border-bottom: 1px solid var(--la-hair-base);
    font-size: 8px;
    flex-wrap: wrap;
  }
  .tier-node {
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 1px 5px;
    border: 1px solid currentColor;
  }
  .tier-node.keychain { color: var(--la-semantic-ok); }
  .tier-node.session  { color: var(--la-semantic-warn); }
  .tier-node.none     { color: var(--la-text-dim); }
  .tier-arrow { color: var(--la-text-mute); }
  .tier-desc  { color: var(--la-text-dim); font-size: 7px; }

  /* ── Provider tabs ────────────────────────────────────────────────────── */
  .provider-tabs {
    display: flex;
    border-bottom: 1px solid var(--la-hair-strong);
    flex-shrink: 0;
  }
  .ptab {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding: 8px 12px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-dim);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    transition: color var(--la-t-snap), border-color var(--la-t-snap);
  }
  .ptab.active {
    color: var(--la-text-bright);
    border-bottom-color: var(--la-struct-primary);
  }
  .ptab:hover:not(.active) { color: var(--la-text-base); }
  .ptab-badge {
    font-size: 7px;
    padding: 1px 4px;
    border: 1px solid currentColor;
    font-weight: 700;
    letter-spacing: 0.06em;
  }
  .tier-keychain { color: var(--la-semantic-ok); }
  .tier-session  { color: var(--la-semantic-warn); }
  .tier-none     { color: var(--la-text-mute); }

  /* ── Body ─────────────────────────────────────────────────────────────── */
  .vault-body {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ── Status card ──────────────────────────────────────────────────────── */
  .status-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid;
  }
  .status-card.tier-keychain {
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.06);
  }
  .status-card.tier-session {
    border-color: rgba(245, 158, 11, 0.35);
    background: rgba(245, 158, 11, 0.06);
  }
  .status-card-inner {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    margin-top: 3px;
    flex-shrink: 0;
  }
  .tier-keychain .status-dot { background: var(--la-semantic-ok); box-shadow: var(--la-semantic-ok-glow); }
  .tier-session  .status-dot { background: var(--la-semantic-warn); box-shadow: var(--la-semantic-warn-glow); }
  .status-heading {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-bright);
    margin-bottom: 2px;
  }
  .status-sub { font-size: 8px; color: var(--la-text-dim); }
  .revoke-btn {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-semantic-error);
    background: none;
    border: 1px solid var(--la-semantic-error);
    padding: 3px 8px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--la-t-snap), box-shadow var(--la-t-snap);
  }
  .revoke-btn:hover {
    background: rgba(239, 68, 68, 0.1);
    box-shadow: var(--la-semantic-error-glow);
  }

  /* ── Input section ────────────────────────────────────────────────────── */
  .input-section { display: flex; flex-direction: column; gap: 6px; }
  .input-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .input-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-dim);
  }
  .steps-toggle {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-struct-primary);
    background: none;
    border: none;
    cursor: pointer;
    opacity: 0.7;
    transition: opacity var(--la-t-snap);
  }
  .steps-toggle:hover { opacity: 1; }

  /* ── Steps / instructions ─────────────────────────────────────────────── */
  .steps-block {
    background: var(--la-bg-card);
    border: 1px solid var(--la-hair-base);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .step-list {
    list-style: none;
    padding: 0; margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .step-list li {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    font-size: 8px;
    color: var(--la-text-base);
    line-height: 1.4;
  }
  .step-n {
    font-size: 7px;
    font-weight: 700;
    color: var(--la-struct-primary);
    background: rgba(0, 200, 255, 0.1);
    border: 1px solid rgba(0, 200, 255, 0.25);
    padding: 0 4px;
    flex-shrink: 0;
    line-height: 1.6;
  }
  .scopes-row {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }
  .scopes-lbl { font-size: 7px; color: var(--la-text-dim); }
  .scope-badge {
    font-size: 7px;
    color: var(--la-semantic-warn);
    background: rgba(245, 158, 11, 0.08);
    border: 1px solid rgba(245, 158, 11, 0.3);
    padding: 1px 5px;
  }

  /* ── Token input row ──────────────────────────────────────────────────── */
  .input-row {
    display: flex;
    gap: 6px;
  }
  .token-input {
    flex: 1;
    font-family: var(--la-font-mono);
    font-size: 9px;
    color: var(--la-text-bright);
    background: var(--la-bg-card);
    border: 1px solid var(--la-hair-strong);
    padding: 6px 8px;
    outline: none;
    transition: border-color var(--la-t-snap);
  }
  .token-input::placeholder { color: var(--la-text-mute); }
  .token-input:focus { border-color: var(--la-focus-ring); }
  .token-input.invalid { border-color: var(--la-semantic-error); }

  .save-btn {
    font-family: var(--la-font-mono);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-bright);
    background: var(--la-bg-elevated);
    border: 1px solid var(--la-hair-strong);
    padding: 6px 10px;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--la-t-snap), border-color var(--la-t-snap), box-shadow var(--la-t-snap);
  }
  .save-btn:hover:not(:disabled) {
    border-color: var(--la-struct-primary);
    box-shadow: 0 0 0 1px rgba(0, 200, 255, 0.15);
  }
  .save-btn.keychain-mode:not(:disabled) {
    color: var(--la-semantic-ok);
    border-color: rgba(34, 197, 94, 0.4);
  }
  .save-btn.keychain-mode:hover:not(:disabled) {
    background: rgba(34, 197, 94, 0.08);
    box-shadow: var(--la-semantic-ok-glow);
  }
  .save-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ── Inline hints ─────────────────────────────────────────────────────── */
  .hint { font-size: 8px; padding: 4px 0; }
  .hint.err  { color: var(--la-semantic-error); }
  .hint.warn { color: var(--la-semantic-warn); }
  .ic {
    font-family: var(--la-font-mono);
    font-size: 8px;
    color: var(--la-struct-primary);
    background: rgba(0, 200, 255, 0.08);
    border: 1px solid rgba(0, 200, 255, 0.2);
    padding: 1px 4px;
  }

  /* ── Feedback ─────────────────────────────────────────────────────────── */
  .feedback {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.06em;
    padding: 5px 8px;
    border: 1px solid;
  }
  .feedback.ok {
    color: var(--la-semantic-ok);
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.06);
  }
  .feedback.warn {
    color: var(--la-semantic-warn);
    border-color: rgba(245, 158, 11, 0.35);
    background: rgba(245, 158, 11, 0.06);
  }

  /* ── Gateway offline callout ──────────────────────────────────────────── */
  .gw-callout {
    font-size: 8px;
    color: var(--la-text-dim);
    border: 1px solid var(--la-hair-base);
    padding: 7px 9px;
    line-height: 1.6;
  }
  .gw-callout-hd {
    display: block;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-semantic-warn);
    margin-bottom: 4px;
  }
</style>
