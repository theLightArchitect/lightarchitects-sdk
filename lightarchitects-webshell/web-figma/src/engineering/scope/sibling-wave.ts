// ============================================================================
// File: web-figma/src/engineering/scope/sibling-wave.ts
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Per-sibling oscilloscope state — TypeScript port of
//          Projects/lÆx0-cli/src/tui/oscilloscope.rs::SiblingWave
// Parity: constants and tick/spike semantics identical to Rust reference
// Complexity: O(1) per spike, O(1) per tick (ring-buffer shift + push)
// Security: pure function state, no external input
// ============================================================================

/** Number of samples in each sibling's ring buffer. */
export const BUF_LEN = 56;

/** Amplitude decay per tick (40 fps → ~0.3s ring-down from full spike). */
export const DECAY = 0.88;

/** Phase advance (radians per tick) while active — waveform frequency. */
export const PHASE_STEP = 0.38;

/** Amplitude scale applied before stacking (keeps waveforms from overlapping). */
export const AMP_SCALE = 0.55;

/** Absolute-value threshold above which peak overlay applies. */
export const PEAK_THRESHOLD = 0.7;

/** Below this activity → zero out (prevents float drift). */
const ACTIVITY_EPS = 0.004;

/** Active-display threshold (samples drawn even below this — isActive is UI hint). */
const IS_ACTIVE_EPS = 0.01;

/**
 * A single sibling's waveform state.
 *
 * Mirrors the Rust `SiblingWave` struct:
 *   - `spike()` sets `activity = 1.0` (any incoming event → full spike)
 *   - `tick()` decays activity, advances phase, appends new sample
 *   - `samples` is a ring buffer of `BUF_LEN` amplitude values in approximately
 *     `[-AMP_SCALE, +AMP_SCALE]` (or `[-AMP_SCALE * ttsBoost, +AMP_SCALE * ttsBoost]`
 *     when TTS boost is applied — not used by webshell yet).
 */
export class SiblingWave {
  activity = 0;
  phase = 0;
  /** Ring buffer of length BUF_LEN. Oldest sample at index 0. */
  samples: number[];
  /** TTS boost multiplier — 1.0 default; 2.0 during speech (unused in webshell). */
  ttsBoost = 1.0;

  constructor() {
    this.samples = new Array(BUF_LEN).fill(0);
  }

  /** Trigger a spike — sets activity to 1.0 (full envelope start). */
  spike(): void {
    this.activity = 1.0;
  }

  /**
   * Advance one frame: decay activity, accumulate phase (only while active),
   * compute new sample via sin(phase) * AMP_SCALE, append to ring buffer.
   */
  tick(): void {
    this.activity = Math.max(this.activity * DECAY, 0);
    if (this.activity < ACTIVITY_EPS) this.activity = 0;

    // Phase only advances while active — idle siblings render a flat line.
    if (this.activity > 0) this.phase += PHASE_STEP;

    const effective = this.activity * this.ttsBoost;
    const sample = effective * Math.sin(this.phase) * AMP_SCALE;

    // Ring-buffer: shift oldest, push newest.
    this.samples.shift();
    this.samples.push(sample);
  }

  /** True if this wave is currently producing a visible signal (UI hint). */
  isActive(): boolean {
    return this.activity > IS_ACTIVE_EPS;
  }
}
