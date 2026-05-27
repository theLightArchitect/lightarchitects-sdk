<!--
@component
Fixed-position notification stack — mounts at z-index --z-overlay (200).
Sits above modals and tooltips per the z-index ladder in tokens.css.

Renders at bottom-right, stacks upward. HITL items always stay pinned
at the top of the stack (lowest in DOM = frontmost visually via flex-col-reverse).

Mounts the notification bridge on init so all SSE events wire automatically.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { notifications } from '$lib/notificationStore';
  import { mountNotificationBridge } from '$lib/notificationBridge';
  import NotificationToast from './NotificationToast.svelte';

  onMount(() => {
    return mountNotificationBridge();
  });

  /** HITL items first, then rest in push order (newest first). */
  const sorted = $derived(
    [
      ...$notifications.filter(n => n.severity === 'hitl'),
      ...$notifications.filter(n => n.severity !== 'hitl'),
    ]
  );
</script>

{#if $notifications.length > 0}
  <div class="notification-stack" aria-label="Notifications" role="region">
    {#each sorted as item (item.id)}
      <NotificationToast {item} />
    {/each}
  </div>
{/if}

<style>
  .notification-stack {
    position:        fixed;
    bottom:          20px;
    right:           20px;
    z-index:         200; /* --z-overlay */
    display:         flex;
    flex-direction:  column-reverse;
    align-items:     flex-end;
    gap:             0;
    pointer-events:  none;
    /* Individual toasts restore pointer-events */
  }

  .notification-stack :global(.toast) {
    pointer-events: auto;
  }
</style>
