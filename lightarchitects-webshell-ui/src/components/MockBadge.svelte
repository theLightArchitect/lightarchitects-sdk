<!--
@component
MockBadge — absolute-positioned corner stamp for cards showing mock/placeholder data.
Parent must have `position: relative` (Tailwind: `relative`).
Renders top-right by default; pass `position="bottom-right"` for alternate placement.

Props:
- `label`    — short label (≤6 chars) always visible. Default "MOCK".
- `detail`   — longer detail text. Shown via title attribute + visible on viewports ≥360px.
- `position` — corner placement. Default "top-right".

a11y: `role="note"` (static annotation, not a live region — avoids SR re-announce noise).
-->
<script lang="ts">
  interface Props {
    label?: string;
    detail?: string;
    position?: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left';
  }
  let { label = 'MOCK', detail, position = 'top-right' }: Props = $props();

  const posClass: Record<NonNullable<Props['position']>, string> = {
    'top-right':    'top-2 right-2',
    'top-left':     'top-2 left-2',
    'bottom-right': 'bottom-2 right-2',
    'bottom-left':  'bottom-2 left-2',
  };
</script>

<span
  class="mock-badge {posClass[position]}"
  role="note"
  aria-label={detail ? `Mock data: ${detail}` : 'Mock data — backend not yet connected'}
  title={detail ?? 'Mock data — backend not yet connected'}
>
  <span class="mock-badge-label">{label}</span>
  {#if detail}
    <span class="mock-badge-detail">— {detail}</span>
  {/if}
</span>

<style>
  .mock-badge {
    position: absolute;
    z-index: 20;
    pointer-events: none;
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
    font-family: var(--la-font-mono);
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--la-warn-mock-fg);
    text-shadow: var(--la-warn-mock-glow);
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(2px);
    -webkit-backdrop-filter: blur(2px);
    border: 1px solid var(--la-warn-mock-edge);
    border-radius: 3px;
    padding: 2px 6px;
    max-width: 22ch;
  }
  .mock-badge-label {
    white-space: nowrap;
  }
  .mock-badge-detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  @media (max-width: 360px) {
    .mock-badge-detail {
      display: none;
    }
  }
</style>
