// ============================================================================
// Mosaic Panel Layout — store, presets, and tree mutation utilities
// ============================================================================

import { writable, get } from 'svelte/store';
import type { PanelTree, PanelId, LayoutPreset, PanelContext, PanelNavRequest } from './types';

// ── Default preset trees ─────────────────────────────────────────────────────

/** Monitor: what are agents doing right now
 *  git-forest 35% | agent-console 40% | build-status 25% */
const OPS_PRESET: PanelTree = {
  type: 'axis',
  direction: 'row',
  children: [
    { type: 'leaf', panelId: 'git-forest' },
    { type: 'leaf', panelId: 'agent-console' },
    { type: 'leaf', panelId: 'build-status' },
  ],
  flexes: [1.05, 1.2, 0.75],
};

/** Workspace: review what agents changed, run tests to verify
 *  file-explorer 18% | file-diff 52% | terminal 30% */
const IDE_PRESET: PanelTree = {
  type: 'axis',
  direction: 'row',
  children: [
    { type: 'leaf', panelId: 'file-explorer' },
    { type: 'leaf', panelId: 'file-diff' },
    { type: 'leaf', panelId: 'terminal' },
  ],
  flexes: [0.54, 1.56, 0.9],
};

/** Debug: reproduce the failure, see what broke, run the fix
 *  agent-console 40% | findings 35% | terminal 25% */
const DEBUG_PRESET: PanelTree = {
  type: 'axis',
  direction: 'row',
  children: [
    { type: 'leaf', panelId: 'agent-console' },
    { type: 'leaf', panelId: 'findings' },
    { type: 'leaf', panelId: 'terminal' },
  ],
  flexes: [1.2, 1.05, 0.75],
};

/** Ship: review the diff, confirm tests pass, push
 *  file-diff 50% | terminal 30% | build-status 20% */
const PR_REVIEW_PRESET: PanelTree = {
  type: 'axis',
  direction: 'row',
  children: [
    { type: 'leaf', panelId: 'file-diff' },
    { type: 'leaf', panelId: 'terminal' },
    { type: 'leaf', panelId: 'build-status' },
  ],
  flexes: [1.5, 0.9, 0.6],
};

/** Agent: single agent run, full attention */
const FOCUS_PRESET: PanelTree = { type: 'leaf', panelId: 'agent-console' };

/** Observe: live AYIN traces with agent activity context
 *  agent-console 40% | ayin-traces 60% */
const OBSERVE_PRESET: PanelTree = {
  type: 'axis',
  direction: 'row',
  children: [
    { type: 'leaf', panelId: 'agent-console' },
    { type: 'leaf', panelId: 'ayin-traces' },
  ],
  flexes: [1.2, 1.8],
};

export const PRESETS: Record<LayoutPreset, PanelTree> = {
  ops: OPS_PRESET,
  ide: IDE_PRESET,
  debug: DEBUG_PRESET,
  'pr-review': PR_REVIEW_PRESET,
  focus: FOCUS_PRESET,
  observe: OBSERVE_PRESET,
};

// ── Flex-ratio validation ─────────────────────────────────────────────────────

/** Zed invariant: sum(flexes) should ≈ children.length ± 0.001. Reset if invalid. */
function validateFlex(tree: PanelTree): PanelTree {
  if (tree.type !== 'axis') return tree;
  const n = tree.children.length;
  const sum = tree.flexes.reduce((a, b) => a + b, 0);
  const validFlex = tree.flexes.length === n && Math.abs(sum - n) < 0.001;
  return {
    ...tree,
    flexes: validFlex ? tree.flexes : Array(n).fill(1),
    children: tree.children.map(validateFlex),
  };
}

// ── Layout store ─────────────────────────────────────────────────────────────

const LS_LAYOUT_KEY = 'la_layout_ops';
const LS_PRESET_KEY = 'la_layout_preset';
const ORPHANED_KEY  = 'la.helixPanelWidth'; // stale key to delete on first load

function loadLayout(): { tree: PanelTree; preset: LayoutPreset } {
  // Remove the orphaned helix panel width key from before the layout system
  try { localStorage.removeItem(ORPHANED_KEY); } catch { /* ignore */ }

  try {
    const rawPreset = localStorage.getItem(LS_PRESET_KEY);
    const preset = (rawPreset && rawPreset in PRESETS ? rawPreset : 'ops') as LayoutPreset;
    const rawTree = localStorage.getItem(LS_LAYOUT_KEY);
    if (rawTree) {
      const parsed = JSON.parse(rawTree) as PanelTree;
      return { tree: validateFlex(parsed), preset };
    }
  } catch { /* ignore parse errors */ }
  return { tree: OPS_PRESET, preset: 'ops' };
}

const initial = loadLayout();

/** The current layout tree. Mutate via setLayout / applyPreset / updateFlex. */
export const layoutTree = writable<PanelTree>(initial.tree);

/** The active named preset (may diverge from tree once user customizes). */
export const activePreset = writable<LayoutPreset>(initial.preset);

let saveTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    try {
      localStorage.setItem(LS_LAYOUT_KEY, JSON.stringify(get(layoutTree)));
      localStorage.setItem(LS_PRESET_KEY, get(activePreset));
    } catch { /* storage quota exceeded — ignore */ }
  }, 500);
}

/** Replace the entire tree (used by preset switcher and drag-to-split). */
export function setLayout(tree: PanelTree, preset?: LayoutPreset) {
  layoutTree.set(validateFlex(tree));
  if (preset) activePreset.set(preset);
  scheduleSave();
}

/** Switch to a named preset — replaces the tree. */
export function applyPreset(preset: LayoutPreset) {
  setLayout(PRESETS[preset], preset);
}

/** Update flex ratios for a specific axis node identified by path. */
export function updateFlex(path: number[], newFlexes: number[]) {
  layoutTree.update(root => {
    const updated = setAtPath(root, path, node => {
      if (node.type !== 'axis') return node;
      return { ...node, flexes: newFlexes };
    });
    scheduleSave();
    return updated;
  });
}

// ── Tree path utilities ───────────────────────────────────────────────────────

/** Walk a path of child indices to reach a node, apply transform, return new root. */
export function setAtPath(
  tree: PanelTree,
  path: number[],
  transform: (node: PanelTree) => PanelTree,
): PanelTree {
  if (path.length === 0) return transform(tree);
  if (tree.type !== 'axis') return tree;
  const [head, ...tail] = path;
  const newChildren = tree.children.map((child, i) =>
    i === head ? setAtPath(child, tail, transform) : child,
  );
  return { ...tree, children: newChildren };
}

/** Collect all leaf panelIds in the tree (for panel visibility management). */
export function collectPanelIds(tree: PanelTree): Set<PanelId> {
  const ids = new Set<PanelId>();
  function walk(node: PanelTree) {
    if (node.type === 'leaf') { ids.add(node.panelId); return; }
    if (node.type === 'tabgroup') { node.tabs.forEach(id => ids.add(id)); return; }
    node.children.forEach(walk);
  }
  walk(tree);
  return ids;
}

// ── Custom presets ────────────────────────────────────────────────────────────

export interface CustomPreset {
  id: string;
  name: string;
  tree: PanelTree;
}

const LS_CUSTOM_PRESETS_KEY = 'la_custom_presets';

const VALID_PANEL_IDS = new Set<string>([
  'copilot', 'terminal', 'git-forest', 'agent-console',
  'file-diff', 'file-explorer', 'build-status', 'findings', 'helix',
  'ayin-traces',
]);

function isValidPanelTree(node: unknown): boolean {
  if (typeof node !== 'object' || node === null) return false;
  const n = node as Record<string, unknown>;
  if (n['type'] === 'leaf') return typeof n['panelId'] === 'string' && VALID_PANEL_IDS.has(n['panelId'] as string);
  if (n['type'] === 'tabgroup') return Array.isArray(n['tabs']) && (n['tabs'] as unknown[]).every(t => typeof t === 'string' && VALID_PANEL_IDS.has(t as string));
  if (n['type'] === 'axis') return Array.isArray(n['children']) && (n['children'] as unknown[]).every(isValidPanelTree);
  return false;
}

function loadCustomPresets(): CustomPreset[] {
  try {
    const raw = localStorage.getItem(LS_CUSTOM_PRESETS_KEY);
    if (raw) {
      const parsed: unknown = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        return (parsed as unknown[]).filter(
          (p): p is CustomPreset =>
            typeof p === 'object' && p !== null &&
            typeof (p as Record<string, unknown>)['id'] === 'string' &&
            typeof (p as Record<string, unknown>)['name'] === 'string' &&
            isValidPanelTree((p as Record<string, unknown>)['tree']),
        );
      }
    }
  } catch { /* ignore */ }
  return [];
}

export const customPresets = writable<CustomPreset[]>(loadCustomPresets());

function persistCustomPresets(presets: CustomPreset[]) {
  try { localStorage.setItem(LS_CUSTOM_PRESETS_KEY, JSON.stringify(presets)); } catch { /* ignore */ }
}

/** Save the current layout tree as a named custom preset (upsert by name). */
export function saveCustomPreset(name: string): void {
  const trimmed = name.trim();
  if (!trimmed) return;
  const tree = get(layoutTree);
  customPresets.update(presets => {
    const existing = presets.findIndex(p => p.name === trimmed);
    const entry: CustomPreset = {
      id: existing >= 0 ? presets[existing].id : crypto.randomUUID(),
      name: trimmed,
      tree,
    };
    const updated = existing >= 0
      ? presets.map((p, i) => (i === existing ? entry : p))
      : [...presets, entry];
    persistCustomPresets(updated);
    return updated;
  });
}

export function deleteCustomPreset(id: string): void {
  customPresets.update(presets => {
    const updated = presets.filter(p => p.id !== id);
    persistCustomPresets(updated);
    return updated;
  });
}

export function applyCustomPreset(preset: CustomPreset): void {
  setLayout(preset.tree);
}

/**
 * Append a new leaf panel to the root of the current layout.
 * Zed invariant: sum(flexes) ≈ children.length. Appending flex=1 preserves
 * this because the existing sum already equals old child count.
 */
export function addPanel(panelId: PanelId): void {
  layoutTree.update(tree => {
    if (tree.type === 'leaf') {
      return { type: 'axis', direction: 'row', children: [tree, { type: 'leaf', panelId }], flexes: [1, 1] };
    }
    if (tree.type === 'tabgroup') {
      return { type: 'axis', direction: 'row', children: [tree, { type: 'leaf', panelId }], flexes: [1, 1] };
    }
    return { ...tree, children: [...tree.children, { type: 'leaf', panelId }], flexes: [...tree.flexes, 1] };
  });
  scheduleSave();
}

// ── Focus bus ─────────────────────────────────────────────────────────────────

/** Context written by the focused panel; read by CopilotDrawer. */
export const activePanelContext = writable<PanelContext | null>(null);

/** Cross-panel navigation request (e.g. findings → file-diff). */
export const panelNavRequest = writable<PanelNavRequest | null>(null);

/** ID of the currently maximized panel (null = normal layout). */
export const maximizedPanelId = writable<PanelId | null>(null);

/** ID of the panel whose header is currently being dragged (null = not dragging). */
export const draggingPanelId = writable<PanelId | null>(null);

/** Whether the layout is in edit mode — gates panel action buttons (close, maximize). */
export const editMode = writable(false);

// ── Drag-to-split tree mutations ──────────────────────────────────────────────

function prunePanel(tree: PanelTree, panelId: PanelId): PanelTree | null {
  if (tree.type === 'leaf') return tree.panelId === panelId ? null : tree;
  if (tree.type === 'tabgroup') {
    const tabs = tree.tabs.filter(t => t !== panelId);
    if (tabs.length === 0) return null;
    return { ...tree, tabs, activeIndex: Math.min(tree.activeIndex, tabs.length - 1) };
  }
  const newChildren: PanelTree[] = [];
  const newFlexes: number[] = [];
  for (let i = 0; i < tree.children.length; i++) {
    const pruned = prunePanel(tree.children[i], panelId);
    if (pruned !== null) { newChildren.push(pruned); newFlexes.push(tree.flexes[i]); }
  }
  if (newChildren.length === 0) return null;
  if (newChildren.length === 1) return newChildren[0];
  return { ...tree, children: newChildren, flexes: newFlexes };
}

function replaceLeaf(tree: PanelTree, targetId: PanelId, replacement: PanelTree): PanelTree {
  if (tree.type === 'leaf') return tree.panelId === targetId ? replacement : tree;
  if (tree.type === 'tabgroup') return tree;
  return { ...tree, children: tree.children.map(c => replaceLeaf(c, targetId, replacement)) };
}

/**
 * Split the target leaf into an axis containing [source, target] or [target, source]
 * based on the drop edge, then remove the source from its original position.
 */
export function splitLeaf(
  targetId: PanelId,
  sourceId: PanelId,
  edge: 'left' | 'right' | 'top' | 'bottom',
) {
  if (targetId === sourceId) return;
  const direction: 'row' | 'column' = (edge === 'left' || edge === 'right') ? 'row' : 'column';
  const insertBefore = edge === 'left' || edge === 'top';

  const pruned = prunePanel(get(layoutTree), sourceId);
  if (!pruned) return;

  const newAxis: PanelTree = {
    type: 'axis',
    direction,
    children: insertBefore
      ? [{ type: 'leaf', panelId: sourceId }, { type: 'leaf', panelId: targetId }]
      : [{ type: 'leaf', panelId: targetId }, { type: 'leaf', panelId: sourceId }],
    flexes: [1, 1],
  };

  setLayout(replaceLeaf(pruned, targetId, newAxis));
}
