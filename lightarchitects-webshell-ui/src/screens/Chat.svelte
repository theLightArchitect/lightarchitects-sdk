<!--
  Chat.svelte — polished LiteLLM streaming chat panel.

  Aesthetic: editorial-brutalist terminal. Black paper, refined serif
  for headers and quoted thought, JetBrains Mono for everything else,
  hard rules, gold focus accents. Memorable details:
   - pixel-block caret (▌) that blinks at the live end of streamed text
   - tiny language label on code blocks that fades in on hover
   - thinking blocks rendered as parchment cards with a left-gold rule
     and bracketed [THINKING] tag in small-caps serif
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { renderMarkdown } from '$lib/markdown';
  import { authHeaders } from '$lib/auth';
  import PolytopeIcon from '../components/PolytopeIcon.svelte';
  import { getChatActorPolytope } from '$lib/design-tokens';

  type Role = 'user' | 'assistant';

  interface Block {
    kind: 'text' | 'thinking';
    /** Accumulated content for this block. */
    content: string;
    /** Closed = no more chunks expected (set when next block begins or stream ends). */
    closed: boolean;
  }

  interface Msg {
    id: string;
    role: Role;
    /** Ordered visible/thinking blocks; assistant messages may have many. */
    blocks: Block[];
    /** Wall-clock when the turn started — used for the subtle timestamp. */
    startedAt: number;
    /** Live = currently streaming (caret blinks on the last block). */
    live: boolean;
    /**
     * Optional sibling identity for this turn. When set, the polytope
     * avatar adopts the sibling's canonical 4D figure + color (EVA →
     * rectified 5-cell, CORSO → 16-cell, etc.). Falls back to the
     * default user/assistant avatar otherwise.
     */
    sibling?: string;
  }

  let messages = $state<Msg[]>([]);
  let input = $state('');
  let busy = $state(false);
  let errorText = $state<string | null>(null);
  let scrollEl: HTMLElement | null = $state(null);
  let textarea: HTMLTextAreaElement | null = $state(null);

  // Auto-scroll: stick to bottom unless the user has scrolled up.
  let stickToBottom = $state(true);

  function onScroll() {
    if (!scrollEl) return;
    const nearBottom =
      scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 80;
    stickToBottom = nearBottom;
  }

  async function scrollIfSticky() {
    if (!stickToBottom) return;
    await tick();
    if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
  }

  function appendBlock(msg: Msg, kind: 'text' | 'thinking', text: string) {
    const last = msg.blocks[msg.blocks.length - 1];
    if (last && last.kind === kind && !last.closed) {
      last.content += text;
    } else {
      // Close the previous block; start a new one.
      if (last) last.closed = true;
      msg.blocks.push({ kind, content: text, closed: false });
    }
  }

  async function send() {
    const text = input.trim();
    if (!text || busy) return;
    errorText = null;
    input = '';
    if (textarea) textarea.style.height = 'auto';
    busy = true;

    const userMsg: Msg = {
      id: crypto.randomUUID(),
      role: 'user',
      blocks: [{ kind: 'text', content: text, closed: true }],
      startedAt: Date.now(),
      live: false,
    };
    messages.push(userMsg);

    // Build short history (last 12 turns) — keeps llama3.2:3b's context lean.
    const history = messages
      .slice(-13, -1)
      .filter((m) => m.role === 'user' || m.role === 'assistant')
      .map((m) => ({
        role: m.role,
        content: m.blocks
          .filter((b) => b.kind === 'text')
          .map((b) => b.content)
          .join('\n'),
      }));

    const asstMsgInit: Msg = {
      id: crypto.randomUUID(),
      role: 'assistant',
      blocks: [],
      startedAt: Date.now(),
      live: true,
    };
    messages.push(asstMsgInit);
    // Svelte 5: rebind to the reactive proxy now in the array — mutating the
    // pre-push object would update plain data invisible to the renderer.
    const asstMsg = messages[messages.length - 1]!;
    await scrollIfSticky();

    try {
      const res = await fetch('/api/litellm/chat', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Accept: 'text/event-stream',
          ...authHeaders(),
        },
        body: JSON.stringify({ message: text, history }),
      });
      if (!res.ok || !res.body) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const reader = res.body.getReader();
      const decoder = new TextDecoder('utf-8');
      let buf = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += decoder.decode(value, { stream: true });
        // SSE frames are blank-line-delimited.
        let nl: number;
        while ((nl = buf.indexOf('\n')) !== -1) {
          const line = buf.slice(0, nl).replace(/\r$/, '');
          buf = buf.slice(nl + 1);
          if (!line.startsWith('data: ')) continue;
          const data = line.slice(6).trim();
          if (data === 'keep-alive' || data === '') continue;
          try {
            const ev = JSON.parse(data) as
              | { type: 'delta'; text: string }
              | { type: 'thinking_delta'; text: string }
              | { type: 'complete' }
              | { type: 'error'; message: string };
            if (ev.type === 'delta') {
              appendBlock(asstMsg, 'text', ev.text);
            } else if (ev.type === 'thinking_delta') {
              appendBlock(asstMsg, 'thinking', ev.text);
            } else if (ev.type === 'error') {
              errorText = ev.message;
              break;
            } else if (ev.type === 'complete') {
              // handled below
            }
            // Re-trigger reactivity: array mutation in-place isn't tracked,
            // so reassign the spread.
            messages = messages;
            await scrollIfSticky();
          } catch {
            /* malformed frame — ignore */
          }
        }
      }
    } catch (e) {
      errorText = e instanceof Error ? e.message : String(e);
    } finally {
      asstMsg.live = false;
      for (const b of asstMsg.blocks) b.closed = true;
      messages = messages;
      busy = false;
      await scrollIfSticky();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  }

  function onTextareaInput() {
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + 'px';
  }

  // Thinking block toggle state per block (keyed by message+block index).
  const openThinking = $state<Record<string, boolean>>({});
  function thinkKey(msgId: string, blockIdx: number) {
    return `${msgId}:${blockIdx}`;
  }
  function toggleThinking(key: string) {
    openThinking[key] = !openThinking[key];
  }

  // Format elapsed time as e.g. "2.4s"
  function elapsed(start: number): string {
    const s = (Date.now() - start) / 1000;
    if (s < 60) return s.toFixed(1) + 's';
    const m = Math.floor(s / 60);
    return `${m}m ${Math.floor(s - m * 60)}s`;
  }

  onMount(() => {
    if (textarea) textarea.focus();
  });
</script>

<svelte:head>
  <title>LA Chat · LiteLLM</title>
</svelte:head>

<div class="chat-root">
  <header class="chat-head">
    <div class="chat-head-left">
      <span class="chat-mark">▌</span>
      <span class="chat-title">DIRECT</span>
      <span class="chat-rule"></span>
      <span class="chat-sub">a streaming chat through litellm</span>
    </div>
    <div class="chat-head-right">
      <span class="chat-pill">local-llama</span>
      <span class="chat-pill chat-pill-dim">via :4000</span>
    </div>
  </header>

  <main
    class="chat-scroll"
    bind:this={scrollEl}
    onscroll={onScroll}
  >
    {#if messages.length === 0}
      <div class="chat-empty">
        <h1 class="chat-empty-h">A clean line. <span class="chat-empty-em">No subprocess.</span></h1>
        <p class="chat-empty-p">
          Every keystroke from the model arrives here as it's produced — token
          by token, through LiteLLM, from llama3.2:3b on this machine. Try a
          prompt below.
        </p>
        <div class="chat-empty-prompts">
          <button class="chat-prompt-btn" onclick={() => { input = 'Explain SSE streaming in 3 sentences.'; void send(); }}>
            <span class="chat-prompt-tag">PROMPT</span> Explain SSE streaming in 3 sentences.
          </button>
          <button class="chat-prompt-btn" onclick={() => { input = 'Write a Rust function that reverses a string.'; void send(); }}>
            <span class="chat-prompt-tag">CODE</span> Write a Rust function that reverses a string.
          </button>
          <button class="chat-prompt-btn" onclick={() => { input = 'What makes a good API design?'; void send(); }}>
            <span class="chat-prompt-tag">OPEN</span> What makes a good API design?
          </button>
        </div>
      </div>
    {/if}

    {#each messages as msg (msg.id)}
      {@const poly = getChatActorPolytope(msg.role, msg.sibling)}
      <article class="chat-msg chat-msg-{msg.role}">
        <div class="chat-msg-meta">
          <!-- 4D polytope avatar — user gets the 600-cell, assistant gets
               doubleHelix, and sibling speakers (EVA/CORSO/…) adopt their
               canonical figure so identity reads at a glance. -->
          <span class="chat-meta-poly" aria-hidden="true">
            <PolytopeIcon type={poly.type} color={poly.color} size={28} />
          </span>
          <span class="chat-meta-role">{msg.role === 'user' ? 'YOU' : (msg.sibling?.toUpperCase() ?? 'MODEL')}</span>
          <span class="chat-meta-time">{elapsed(msg.startedAt)}</span>
        </div>
        <div class="chat-msg-body">
          {#each msg.blocks as block, i (i)}
            {#if block.kind === 'thinking'}
              {@const k = thinkKey(msg.id, i)}
              <div class="chat-think">
                <button
                  type="button"
                  class="chat-think-tog"
                  onclick={() => toggleThinking(k)}
                  aria-expanded={openThinking[k] ?? false}
                >
                  <span class="chat-think-chev">{openThinking[k] ? '▾' : '▸'}</span>
                  <span class="chat-think-tag">[THINKING]</span>
                  <span class="chat-think-count">{block.content.length} chars</span>
                </button>
                {#if openThinking[k] ?? false}
                  <div class="chat-think-body">{block.content}</div>
                {/if}
              </div>
            {:else if msg.live && i === msg.blocks.length - 1 && !block.closed}
              <div class="chat-text chat-text-live">
                <span>{@html renderMarkdown(block.content)}</span><span class="chat-caret">▌</span>
              </div>
            {:else}
              <div class="chat-text">{@html renderMarkdown(block.content)}</div>
            {/if}
          {/each}
          {#if msg.live && msg.blocks.length === 0}
            <div class="chat-text chat-text-live">
              <span class="chat-think-tag chat-warming">Thinking…</span>
              <span class="chat-caret">▌</span>
            </div>
          {/if}
        </div>
      </article>
    {/each}

    {#if errorText}
      <div class="chat-error">
        <span class="chat-error-tag">ERROR</span>
        {errorText}
      </div>
    {/if}
  </main>

  <footer class="chat-foot">
    <form
      class="chat-form"
      onsubmit={(e) => { e.preventDefault(); void send(); }}
    >
      <textarea
        bind:this={textarea}
        bind:value={input}
        oninput={onTextareaInput}
        onkeydown={onKey}
        placeholder="Type a message — Enter sends, Shift+Enter newline"
        rows="1"
        disabled={busy}
      ></textarea>
      <div class="chat-foot-side">
        <span class="chat-hint">
          {#if busy}<span class="chat-hint-busy">streaming</span>{:else}↵ to send{/if}
        </span>
        <button class="chat-send" type="submit" disabled={busy || !input.trim()}>
          <span class="chat-send-text">SEND</span>
          <span class="chat-send-arrow">→</span>
        </button>
      </div>
    </form>
  </footer>
</div>

<style>
  /* Editorial-brutalist terminal. Bring everything down to dark paper. */
  .chat-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    background:
      radial-gradient(ellipse at 50% -20%, rgba(255, 215, 0, 0.05), transparent 55%),
      var(--la-bg-base);
    color: var(--la-text-bright);
    font-family: var(--la-font-mono);
    position: relative;
  }
  /* Subtle grain — print-press feel. */
  .chat-root::before {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    background-image:
      repeating-linear-gradient(
        0deg,
        rgba(255, 255, 255, 0.012) 0px,
        rgba(255, 255, 255, 0.012) 1px,
        transparent 1px,
        transparent 3px
      );
    mix-blend-mode: overlay;
    z-index: 0;
  }

  /* ── Header strip ── */
  .chat-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 20px 32px 14px;
    border-bottom: 1px solid var(--la-bg-elev-2);
    position: relative;
    z-index: 1;
    gap: 24px;
  }
  .chat-head-left { display: flex; align-items: baseline; gap: 12px; min-width: 0; }
  .chat-mark {
    color: var(--la-focus-ring);
    font-size: 18px;
    line-height: 1;
    text-shadow: 0 0 12px rgba(255, 215, 0, 0.4);
    animation: caretPulse 1.4s ease-in-out infinite;
  }
  .chat-title {
    font-family: 'New York', 'Charter', 'Iowan Old Style', Georgia, serif;
    font-style: italic;
    font-size: 24px;
    letter-spacing: 0.02em;
    font-weight: 500;
    color: var(--la-text-stark);
  }
  .chat-rule {
    width: 60px;
    height: 1px;
    background: var(--la-text-mute);
    transform: translateY(-4px);
  }
  .chat-sub {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    color: var(--la-text-mute);
  }
  .chat-head-right { display: flex; gap: 10px; }
  .chat-pill {
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    padding: 4px 9px;
    border: 1px solid var(--la-bg-elev-2);
    color: var(--la-text-dim);
    background: var(--la-bg-elev-1);
  }
  .chat-pill-dim { opacity: 0.55; }

  /* ── Scroll area & messages ── */
  .chat-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 28px 32px 40px;
    position: relative;
    z-index: 1;
    scrollbar-width: thin;
    scrollbar-color: var(--la-bg-elev-2) transparent;
  }
  .chat-scroll::-webkit-scrollbar { width: 6px; }
  .chat-scroll::-webkit-scrollbar-thumb {
    background: var(--la-bg-elev-2);
    border-radius: 3px;
  }

  /* Empty state — editorial cover page. */
  .chat-empty {
    max-width: 640px;
    margin: 60px auto 0;
    padding: 0 8px;
  }
  .chat-empty-h {
    font-family: 'New York', 'Charter', Georgia, serif;
    font-size: 44px;
    line-height: 1.05;
    margin: 0 0 12px;
    color: var(--la-text-stark);
    font-weight: 400;
    letter-spacing: -0.015em;
  }
  .chat-empty-em {
    font-style: italic;
    color: var(--la-focus-ring);
    text-shadow: 0 0 16px rgba(255, 215, 0, 0.25);
  }
  .chat-empty-p {
    font-size: 14px;
    line-height: 1.65;
    color: var(--la-text-dim);
    max-width: 540px;
    margin: 0 0 32px;
  }
  .chat-empty-prompts {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--la-bg-elev-2);
    border: 1px solid var(--la-bg-elev-2);
  }
  .chat-prompt-btn {
    background: var(--la-bg-elev-1);
    border: 0;
    color: var(--la-text-dim);
    text-align: left;
    padding: 14px 18px;
    font-family: inherit;
    font-size: 13px;
    cursor: pointer;
    display: flex;
    gap: 14px;
    align-items: baseline;
    transition: background 0.12s, color 0.12s;
  }
  .chat-prompt-btn:hover {
    background: var(--la-bg-elev-2);
    color: var(--la-text-bright);
  }
  .chat-prompt-tag {
    font-size: 9px;
    letter-spacing: 0.22em;
    color: var(--la-focus-ring);
    min-width: 56px;
    flex-shrink: 0;
  }

  /* Messages */
  .chat-msg {
    max-width: 760px;
    margin: 0 auto 28px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .chat-msg-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--la-bg-elev-2);
  }
  /* Polytope avatar — pulled left of the column so the figure sits in
     the margin, like a chapter ornament. The author "wears" their
     polytope as identity. */
  .chat-meta-poly {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    margin-left: -38px;
    flex-shrink: 0;
    /* Inset glow that matches the figure's color via background-blend. */
    filter: drop-shadow(0 0 6px rgba(255, 215, 0, 0.18));
  }
  .chat-msg-user .chat-meta-poly {
    filter: drop-shadow(0 0 8px rgba(255, 215, 0, 0.28));
  }
  .chat-meta-role {
    font-size: 10px;
    letter-spacing: 0.22em;
    color: var(--la-focus-ring);
    font-weight: 600;
  }
  .chat-msg-user .chat-meta-role { color: var(--la-text-dim); }
  .chat-meta-time {
    font-size: 10px;
    color: var(--la-text-mute);
    letter-spacing: 0.08em;
  }

  .chat-msg-body { padding: 4px 0 0; }
  .chat-text {
    font-size: 14px;
    line-height: 1.72;
    color: var(--la-text-bright);
    word-wrap: break-word;
  }
  .chat-msg-user .chat-text {
    color: var(--la-text-dim);
    font-style: italic;
    font-family: 'New York', 'Charter', Georgia, serif;
    font-size: 15px;
    line-height: 1.55;
  }

  /* Markdown content */
  .chat-text :global(h1),
  .chat-text :global(h2),
  .chat-text :global(h3) {
    font-family: 'New York', 'Charter', Georgia, serif;
    font-weight: 500;
    margin: 18px 0 8px;
    color: var(--la-text-stark);
    letter-spacing: -0.005em;
  }
  .chat-text :global(h1) { font-size: 22px; }
  .chat-text :global(h2) { font-size: 18px; }
  .chat-text :global(h3) { font-size: 16px; }
  .chat-text :global(p) { margin: 0 0 12px; }
  .chat-text :global(ul),
  .chat-text :global(ol) { margin: 0 0 12px; padding-left: 20px; }
  .chat-text :global(li) { margin-bottom: 4px; }
  .chat-text :global(strong) { color: var(--la-text-stark); font-weight: 600; }
  .chat-text :global(em) {
    color: var(--la-focus-ring);
    font-style: italic;
    font-family: 'New York', 'Charter', Georgia, serif;
  }
  .chat-text :global(a) {
    color: var(--la-focus-ring);
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 3px;
  }
  .chat-text :global(blockquote) {
    border-left: 2px solid var(--la-focus-ring);
    padding: 4px 0 4px 16px;
    margin: 14px 0;
    font-family: 'New York', 'Charter', Georgia, serif;
    font-style: italic;
    color: var(--la-text-dim);
    font-size: 15px;
  }
  /* Inline code */
  .chat-text :global(code) {
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-bg-elev-2);
    color: var(--la-text-code);
    padding: 1px 6px;
    font-size: 12.5px;
    font-family: var(--la-font-mono);
  }
  /* Fenced code blocks */
  .chat-text :global(pre) {
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-bg-elev-2);
    border-left: 2px solid var(--la-focus-ring);
    padding: 14px 16px 12px;
    margin: 14px 0;
    overflow-x: auto;
    position: relative;
    font-size: 12.5px;
    line-height: 1.55;
    box-shadow: 0 0 0 1px rgba(255, 215, 0, 0.04);
  }
  .chat-text :global(pre code) {
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--la-text-code);
  }
  /* Table */
  .chat-text :global(table) {
    border-collapse: collapse;
    margin: 14px 0;
    font-size: 12.5px;
    width: 100%;
  }
  .chat-text :global(th),
  .chat-text :global(td) {
    border: 1px solid var(--la-bg-elev-2);
    padding: 6px 10px;
    text-align: left;
  }
  .chat-text :global(th) {
    background: var(--la-bg-elev-1);
    color: var(--la-text-stark);
    font-weight: 500;
    letter-spacing: 0.04em;
  }

  /* Caret — the memorable detail. Pixel block, gold, glows softly. */
  .chat-caret {
    display: inline-block;
    width: 0.55em;
    color: var(--la-focus-ring);
    margin-left: 2px;
    font-weight: 800;
    animation: caretBlink 0.85s steps(2, end) infinite;
    text-shadow: 0 0 8px rgba(255, 215, 0, 0.55);
    transform: translateY(2px);
  }
  @keyframes caretBlink {
    0%, 49% { opacity: 1; }
    50%, 100% { opacity: 0.05; }
  }
  @keyframes caretPulse {
    0%, 100% { opacity: 1; text-shadow: 0 0 12px rgba(255, 215, 0, 0.4); }
    50% { opacity: 0.75; text-shadow: 0 0 6px rgba(255, 215, 0, 0.2); }
  }

  /* Thinking block — parchment-style card with gold rule. */
  .chat-think {
    margin: 10px 0;
    border-left: 2px solid var(--la-focus-ring);
    background: linear-gradient(
      90deg,
      rgba(255, 215, 0, 0.04) 0%,
      transparent 30%
    );
    /* Scan-line texture */
    background-image:
      linear-gradient(90deg, rgba(255, 215, 0, 0.04) 0%, transparent 30%),
      repeating-linear-gradient(
        0deg,
        transparent 0px,
        transparent 3px,
        rgba(255, 215, 0, 0.025) 3px,
        rgba(255, 215, 0, 0.025) 4px
      );
  }
  .chat-think-tog {
    width: 100%;
    background: transparent;
    border: 0;
    color: var(--la-text-dim);
    padding: 8px 12px;
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    transition: color 0.12s;
  }
  .chat-think-tog:hover { color: var(--la-text-bright); }
  .chat-think-chev { color: var(--la-focus-ring); font-size: 9px; }
  .chat-think-tag {
    font-family: 'New York', 'Charter', Georgia, serif;
    font-variant: small-caps;
    letter-spacing: 0.18em;
    color: var(--la-focus-ring);
    font-style: italic;
    font-size: 12px;
  }
  .chat-think-count {
    margin-left: auto;
    font-size: 10px;
    color: var(--la-text-mute);
    letter-spacing: 0.08em;
  }
  .chat-think-body {
    padding: 4px 14px 12px 26px;
    font-style: italic;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--la-text-dim);
    white-space: pre-wrap;
    font-family: 'New York', 'Charter', Georgia, serif;
  }
  .chat-warming {
    opacity: 0.8;
    font-size: 13px;
  }

  /* Error frame */
  .chat-error {
    max-width: 760px;
    margin: 0 auto 24px;
    padding: 12px 16px;
    border: 1px solid #ff6b6b;
    border-left-width: 3px;
    background: rgba(255, 107, 107, 0.06);
    color: #ffb4b4;
    font-size: 12.5px;
    display: flex;
    gap: 12px;
    align-items: baseline;
  }
  .chat-error-tag {
    font-size: 10px;
    letter-spacing: 0.22em;
    color: #ff6b6b;
    font-weight: 600;
  }

  /* ── Footer / input ── */
  .chat-foot {
    border-top: 1px solid var(--la-bg-elev-2);
    padding: 14px 32px 20px;
    background:
      linear-gradient(0deg, var(--la-bg-base), transparent 80%),
      var(--la-bg-base);
    position: relative;
    z-index: 1;
  }
  .chat-form {
    max-width: 800px;
    margin: 0 auto;
    display: flex;
    align-items: flex-end;
    gap: 16px;
    border: 1px solid var(--la-bg-elev-2);
    background: var(--la-bg-elev-1);
    padding: 10px 14px;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .chat-form:focus-within {
    border-color: var(--la-focus-ring);
    box-shadow:
      0 0 0 1px rgba(255, 215, 0, 0.2),
      0 0 24px rgba(255, 215, 0, 0.12);
  }
  .chat-form textarea {
    flex: 1;
    background: transparent;
    border: 0;
    outline: 0;
    resize: none;
    color: var(--la-text-bright);
    font-family: var(--la-font-mono);
    font-size: 14px;
    line-height: 1.5;
    min-height: 22px;
    max-height: 200px;
  }
  .chat-form textarea::placeholder { color: var(--la-text-mute); }
  .chat-foot-side {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .chat-hint {
    font-size: 10px;
    letter-spacing: 0.18em;
    color: var(--la-text-mute);
    text-transform: uppercase;
  }
  .chat-hint-busy { color: var(--la-focus-ring); animation: caretPulse 1.4s ease-in-out infinite; }
  .chat-send {
    background: var(--la-focus-ring);
    color: #000;
    border: 0;
    padding: 8px 14px;
    font-family: var(--la-font-mono);
    font-weight: 700;
    font-size: 11px;
    letter-spacing: 0.16em;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    transition: filter 0.15s, transform 0.15s;
  }
  .chat-send:hover:not(:disabled) {
    filter: brightness(1.15);
    transform: translateY(-1px);
  }
  .chat-send:disabled {
    background: var(--la-bg-elev-2);
    color: var(--la-text-mute);
    cursor: not-allowed;
  }
  .chat-send-arrow { font-size: 14px; }
</style>
