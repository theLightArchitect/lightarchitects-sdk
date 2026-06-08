<!--
  @component ConfidencePill — shows confidence value + evidence tier.
  @props value  0.0–1.0 float (displayed as percentage)
         tier   EvidenceTier ("HIGH" | "MEDIUM" | "LOW")
-->
<script lang="ts">
  import type { EvidenceTier } from '../types';
  let { value, tier }: { value: number; tier: EvidenceTier } = $props();

  const TIER_COLOR: Record<EvidenceTier, string> = {
    HIGH:   'var(--ls-acc-green)',
    MEDIUM: 'var(--ls-acc-amber)',
    LOW:    'var(--ls-text-ghost)',
  };
  const color = $derived(TIER_COLOR[tier] ?? 'var(--ls-text-ghost)');
  const pct   = $derived(Math.round(value * 100));
</script>
<span class="ls-conf-pill" style="color: {color}; border-color: {color}" aria-label="{pct}% {tier}">
  {pct}%&nbsp;<span class="ls-conf-tier">{tier}</span>
</span>
<style>
.ls-conf-pill { display: inline-flex; align-items: center; gap: 3px; font-size: 8px; padding: 1px 5px; border: 1px solid; border-radius: 10px; font-family: var(--ls-font-code); }
.ls-conf-tier { opacity: 0.7; }
</style>
