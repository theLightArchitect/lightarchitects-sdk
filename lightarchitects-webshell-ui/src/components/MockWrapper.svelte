<!--
@component
MockWrapper — wraps any card with grayscale desaturation and a corner MOCK badge.
Use when the component renders mock data because its backend endpoint is not yet wired.

Props:
- `label`          — short badge label (≤6 chars). Default "MOCK".
- `detail`         — longer badge detail.
- `intensity`      — grayscale intensity 0–1. Default 0.55.
- `inertChildren`  — applies `inert` to suppress keyboard focus into mock subtree.
                     Default true. Set false only if mock content has interactive
                     elements operators should be able to focus.

Note: `filter: grayscale()` cascades to all descendants including icons/canvas.
This is intentional for the "preview" aesthetic — see DESIGN-LANGUAGE pattern.
Reduced-motion is auto-honored by the global @media block in tokens.css.
-->
<script lang="ts">
  import MockBadge from '$lib/../components/MockBadge.svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    label?: string;
    detail?: string;
    intensity?: number;
    inertChildren?: boolean;
    children: Snippet;
  }
  let {
    label = 'MOCK',
    detail,
    intensity = 0.55,
    inertChildren = true,
    children,
  }: Props = $props();
</script>

<div
  class="mock-wrapper"
  style="--mock-gray: {intensity}"
  inert={inertChildren ? true : undefined}
>
  <MockBadge {label} {detail} />
  {@render children()}
</div>

<style>
  .mock-wrapper {
    position: relative;
    filter: grayscale(calc(var(--mock-gray) * 100%)) opacity(0.82);
    transition: filter 0.3s ease;
    animation: mock-fade-in 220ms ease-out;
  }
  .mock-wrapper:hover {
    filter: grayscale(calc(var(--mock-gray) * 60%)) opacity(0.92);
  }
  @keyframes mock-fade-in {
    from { opacity: 0; transform: translateY(2px); }
    to   { opacity: 1; transform: translateY(0); }
  }
</style>
