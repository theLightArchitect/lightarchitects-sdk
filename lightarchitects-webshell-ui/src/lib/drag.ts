import { writable } from 'svelte/store';
import type { PanelId } from './types';

/** The panelId currently being drag-relocated (null when no drag in progress). */
export const draggingPanelId = writable<PanelId | null>(null);
