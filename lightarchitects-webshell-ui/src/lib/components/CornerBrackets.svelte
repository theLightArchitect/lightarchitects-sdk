<script lang="ts">
  let { dispatching = false }: { dispatching?: boolean } = $props();
</script>

<!-- Wrapper carries data-dispatching so the CSS selector in tokens.css fires -->
<div data-dispatching={dispatching ? 'true' : undefined} aria-hidden="true">
  <div class="corner-bracket tl"></div>
  <div class="corner-bracket tr"></div>
  <div class="corner-bracket bl"></div>
  <div class="corner-bracket br"></div>
</div>
