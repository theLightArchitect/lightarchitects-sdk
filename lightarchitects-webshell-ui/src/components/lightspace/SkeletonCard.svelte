<!--
  @component SkeletonCard
  @description Pulsing placeholder that appears at grid_revealed before real cards arrive.
    Fades out when replaced by its matching kind card via canvasAddCard.

  @contract none — no SSE events; purely visual placeholder
  @reads none — all state via props
  @mutates none
  @api none

  @mockup-ref arch/lightspace-mockup.html → .la-card-skeleton, la-skel-in/la-skel-out animations
-->
<script lang="ts">
  import type { CardKind, CardSpan } from '$lib/lightspace-types';

  interface Props {
    kind: CardKind;
    span: CardSpan;
    tag?: string;
    animDelay?: number;
  }

  let { kind, span, tag = 'awaiting', animDelay = 0 }: Props = $props();

  const KIND_LABEL: Partial<Record<CardKind, string>> = {
    monitor:    'STATUS',
    instrument: 'METRICS',
    trace:      'ACTIVITY',
    agentspawn: 'AGENT',
    branchlane: 'PHASES',
  };
</script>

<section
  class="ls-card-skeleton ls-span-{span.replace('span-', '')} kind-{kind}"
  style="animation-delay: {animDelay}ms"
>
  <div class="ls-skel-head">
    <span class="ls-skel-kind">{KIND_LABEL[kind] ?? kind.toUpperCase()}</span>
    <span class="ls-skel-tag">{tag}</span>
  </div>
  <div class="ls-skel-shimmer"></div>
</section>

<style>
.ls-card-skeleton {
  background: var(--ls-card);
  border: 1px dashed color-mix(in oklab, var(--kind-color, var(--ls-border)) 40%, var(--ls-border));
  border-left: 3px solid var(--kind-color, var(--ls-border));
  position: relative;
  display: flex;
  flex-direction: column;
  min-height: 110px;
  opacity: 0;
  animation: ls-skel-in 0.45s ease both;
}

@keyframes ls-skel-in {
  0%   { opacity: 0; transform: translateY(6px) scale(0.985); }
  100% { opacity: 0.94; transform: translateY(0) scale(1); }
}

.ls-card-skeleton.kind-monitor    { --kind-color: var(--ls-kind-monitor); }
.ls-card-skeleton.kind-instrument { --kind-color: var(--ls-kind-instrument); }
.ls-card-skeleton.kind-trace      { --kind-color: var(--ls-kind-trace); }
.ls-card-skeleton.kind-thinking   { --kind-color: var(--ls-kind-thinking); }
.ls-card-skeleton.kind-toolcall   { --kind-color: var(--ls-kind-toolcall); }
.ls-card-skeleton.kind-bash       { --kind-color: var(--ls-kind-bash); }
.ls-card-skeleton.kind-agentspawn { --kind-color: var(--ls-kind-agentspawn); }
.ls-card-skeleton.kind-diff       { --kind-color: var(--ls-kind-diff); }
.ls-card-skeleton.kind-artifact   { --kind-color: var(--ls-kind-artifact); }
.ls-card-skeleton.kind-research   { --kind-color: var(--ls-kind-research); }
.ls-card-skeleton.kind-archgallery{ --kind-color: var(--ls-kind-archgallery); }
.ls-card-skeleton.kind-branchlane { --kind-color: var(--ls-kind-branchlane); }

.ls-skel-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 11px;
  border-bottom: 1px dashed var(--ls-border);
}

.ls-skel-kind {
  font-family: var(--ls-font-display);
  font-weight: 700;
  font-size: 9px;
  letter-spacing: var(--ls-tk-loose);
  color: var(--kind-color, var(--ls-text-mute));
  animation: ls-skel-kind-breath 2s ease-in-out infinite;
}

@keyframes ls-skel-kind-breath {
  0%, 100% { opacity: 0.7; }
  50%       { opacity: 1; }
}

.ls-skel-tag {
  font-size: 8px;
  letter-spacing: var(--ls-tk-mid);
  text-transform: uppercase;
  color: var(--ls-text-mute);
}

.ls-skel-shimmer {
  flex: 1;
  background:
    linear-gradient(90deg, transparent 0%, rgba(255,255,255,0.085) 50%, transparent 100%) repeat-x,
    repeating-linear-gradient(transparent 0 12px, rgba(255,255,255,0.05) 12px 13px);
  background-size: 200% 100%, auto;
  animation: ls-shimmer 2s linear infinite;
}

@keyframes ls-shimmer {
  0%   { background-position: -100% 0, 0 0; }
  100% { background-position: 200% 0, 0 0; }
}
</style>
