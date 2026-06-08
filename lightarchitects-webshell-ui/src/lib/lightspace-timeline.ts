/**
 * Lightspace TIMELINE engine — rAF-based playback for demo mode.
 *
 * Accepts an array of timed events `{t: ms, fn: (stores) => void}[]` and
 * fires them using requestAnimationFrame + performance.now().
 * Exposes play(), pause(), reset(), setSpeed(n) controls.
 *
 * @integration src/lib/lightspace-stores.ts — stores passed to each event fn
 * @integration src/lib/lightspace-demo-timeline.ts — the 235-event array
 */

export interface TimelineEvent {
  t: number;              // timestamp in ms from start
  fn: () => void;         // mutation to fire (no store args — closures capture stores)
}

export interface TimelineState {
  playing: boolean;
  speed: number;          // 0.5 | 1 | 2 | 4
  currentMs: number;      // current playback position
  totalMs: number;
}

export class TimelineEngine {
  private events: TimelineEvent[];
  private state: TimelineState;
  private rafId: number | null = null;
  private wallStart: number | null = null;  // real-time anchor for current play run
  private posAtStart: number = 0;            // timeline position when play() was last called
  private nextEventIdx: number = 0;

  private onStateChange?: (s: TimelineState) => void;

  constructor(events: TimelineEvent[], onStateChange?: (s: TimelineState) => void) {
    this.events = events.slice().sort((a, b) => a.t - b.t);
    const totalMs = events.length > 0 ? Math.max(...events.map(e => e.t)) + 200 : 0;
    this.state = { playing: false, speed: 1, currentMs: 0, totalMs };
    this.onStateChange = onStateChange;
  }

  play() {
    if (this.state.playing) return;
    this.state.playing = true;
    this.wallStart = performance.now();
    this.posAtStart = this.state.currentMs;
    this.notify();
    this.tick();
  }

  pause() {
    if (!this.state.playing) return;
    this.state.playing = false;
    if (this.rafId !== null) { cancelAnimationFrame(this.rafId); this.rafId = null; }
    this.wallStart = null;
    this.notify();
  }

  reset() {
    this.pause();
    this.state.currentMs = 0;
    this.nextEventIdx = 0;
    this.notify();
  }

  setSpeed(s: 0.5 | 1 | 2 | 4) {
    const wasPlaying = this.state.playing;
    if (wasPlaying) this.pause();
    this.state.speed = s;
    if (wasPlaying) this.play();
  }

  getState(): Readonly<TimelineState> {
    return { ...this.state };
  }

  private tick() {
    if (!this.state.playing || this.wallStart === null) return;
    const wallElapsed = performance.now() - this.wallStart;
    const timelinePos = this.posAtStart + wallElapsed * this.state.speed;
    this.state.currentMs = Math.min(timelinePos, this.state.totalMs);

    // Fire all pending events up to current position
    while (
      this.nextEventIdx < this.events.length &&
      this.events[this.nextEventIdx].t <= this.state.currentMs
    ) {
      try { this.events[this.nextEventIdx].fn(); } catch { /* event error is non-fatal */ }
      this.nextEventIdx++;
    }

    this.notify();

    if (this.state.currentMs >= this.state.totalMs) {
      this.state.playing = false;
      this.wallStart = null;
      this.notify();
      return;
    }

    this.rafId = requestAnimationFrame(() => this.tick());
  }

  private notify() {
    this.onStateChange?.({ ...this.state });
  }
}
