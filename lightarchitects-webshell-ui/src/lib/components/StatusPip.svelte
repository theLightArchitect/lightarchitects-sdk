<script lang="ts">
  let {
    color,
    state,
    shape = 'filled',
    ariaLabel,
  }: {
    color: string;
    state: 'idle' | 'active' | 'complete' | 'failed';
    shape?: 'filled' | 'outlined' | 'x';
    ariaLabel: string;
  } = $props();
</script>

<span
  class="status-pip"
  class:pip-active={state === 'active'}
  class:pip-complete={state === 'complete'}
  class:pip-failed={state === 'failed'}
  data-shape={shape}
  role="img"
  aria-label={ariaLabel}
  style:--pip-color={color}
>
  {#if shape === 'x'}<span class="pip-x" aria-hidden="true">×</span>{/if}
</span>

<style>
  .status-pip {
    display: inline-block;
    width: 8px;
    height: 8px;
    background: var(--pip-color, var(--la-text-mute));
    flex-shrink: 0;
    transition: background var(--la-t-snap) var(--la-ease-mech),
                box-shadow var(--la-t-snap) var(--la-ease-mech);
    position: relative;
  }

  /* outlined = hollow; filled = default solid */
  .status-pip[data-shape="outlined"] {
    background: transparent;
    border: 1px solid var(--pip-color, var(--la-text-mute));
  }

  /* x = failure indicator — shows × glyph */
  .status-pip[data-shape="x"] {
    background: transparent;
    border: none;
  }
  .pip-x {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    line-height: 1;
    color: var(--pip-color, var(--la-agent-security));
    font-weight: 700;
  }

  /* Active — blinking animation */
  .pip-active {
    animation: pip-square 1s steps(2) infinite;
    box-shadow: 0 0 6px var(--pip-color);
  }

  /* Complete — static glow */
  .pip-complete {
    box-shadow: 0 0 4px color-mix(in srgb, var(--pip-color) 40%, transparent);
  }

  /* Failed — fast blink */
  .pip-failed {
    background: var(--la-agent-security);
    animation: pip-square 0.4s steps(2) infinite;
    box-shadow: 0 0 8px var(--la-agent-security);
  }

  @keyframes pip-square {
    0%, 49%   { opacity: 1; }
    50%, 100% { opacity: 0.2; }
  }

  /* color-mix fallback for older Safari */
  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .pip-complete {
      box-shadow: 0 0 4px rgba(255, 255, 255, 0.15);
    }
  }
</style>
