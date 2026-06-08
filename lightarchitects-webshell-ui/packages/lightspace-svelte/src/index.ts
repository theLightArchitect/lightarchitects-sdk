// @lightarchitects/lightspace-svelte — barrel export (TS/JS only).
// Svelte components are imported directly by path:
//   import Canvas from '@lightarchitects/lightspace-svelte/Canvas.svelte'
//   import { subscribeSession } from '@lightarchitects/lightspace-svelte'

export * from './types';
export * from './stores';
export { subscribeSession } from './sse';
export { MaterializeEngine } from './materialize';
export type { PhaseEvent } from './materialize';
export { formatAge } from './components/HitlQueue.svelte';
