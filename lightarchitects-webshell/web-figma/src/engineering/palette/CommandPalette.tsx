// ============================================================================
// File: web-figma/src/engineering/palette/CommandPalette.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Cmd-K command palette — full Phase 5 implementation.
// Security: all commands are a static whitelist; args sanitized per-verb;
//           no runtime eval, no dynamic code path, no unsanitized DOM write.
// ============================================================================

import React, { useEffect, useState, useCallback } from 'react';
import { Command } from 'cmdk';
import { useWebshellData } from '../store/EngineeringProvider';
import { SIBLINGS } from '../store/sceneState';
import { useSceneStore } from '../../app/store';

// ── Argument sanitizers ──────────────────────────────────────────────────────

/** Accepts only exact sibling names. */
function sanitizeSibling(raw: string): string | null {
  const s = raw.toLowerCase().trim();
  return (SIBLINGS as readonly string[]).includes(s) ? s : null;
}

/** Strips everything except alphanumeric, spaces, hyphens, underscores, dots.
 *  Max 200 chars. Prevents XSS and injection through query strings. */
function sanitizeQuery(raw: string): string | null {
  const s = raw.replace(/[^a-zA-Z0-9 ._\-]/g, '').slice(0, 200).trim();
  return s.length > 0 ? s : null;
}

// ── Command definitions ──────────────────────────────────────────────────────

interface CmdDef {
  id: string;
  label: string;
  group: string;
  hint?: string;
  /** Returns false if arg is invalid/unsafe. */
  run: (arg: string, ctx: ExecCtx) => boolean;
}

interface ExecCtx {
  setFocusedSibling: (s: string | null) => void;
}

const COMMANDS: CmdDef[] = [
  // ── NAVIGATION ──────────────────────────────────────────────────────────
  ...SIBLINGS.map((s) => ({
    id: `focus-${s}`,
    label: `focus ${s}`,
    group: 'Navigation',
    run: (_arg: string, ctx: ExecCtx) => { ctx.setFocusedSibling(s); return true; },
  })),
  {
    id: 'focus-clear',
    label: 'focus clear',
    group: 'Navigation',
    hint: 'clear focused sibling',
    run: (_arg: string, ctx: ExecCtx) => { ctx.setFocusedSibling(null); return true; },
  },

  // ── RETRIEVAL ───────────────────────────────────────────────────────────
  {
    id: 'query',
    label: 'query …',
    group: 'Retrieval',
    hint: 'helix query — spawns retrieval orb',
    // Arg is the query pattern; sanitized before use.
    run: (arg: string, _ctx: ExecCtx) => {
      const safe = sanitizeQuery(arg);
      if (!safe) return false;
      // Trigger retrieval orb in the 3D scene. Full SOUL helix API call lives
      // on the backend SSE path; this is the client-side visual signal.
      useSceneStore.getState().spawnOrb();
      console.debug('[palette] query:', safe);
      return true;
    },
  },

  // ── SYSTEM ──────────────────────────────────────────────────────────────
  {
    id: 'clear',
    label: 'clear',
    group: 'System',
    hint: 'clear focused sibling (alias)',
    run: (_arg: string, ctx: ExecCtx) => { ctx.setFocusedSibling(null); return true; },
  },
  {
    id: 'pty-restart',
    label: 'pty restart',
    group: 'System',
    hint: 'signal PTY session restart',
    run: (_arg: string, _ctx: ExecCtx) => {
      // PTY restart is signalled by closing and re-connecting the WebSocket.
      // The backend drops the PTY process on WS close; the frontend re-establishes
      // on the next keystroke. Dispatching a storage event lets xterm.js pick it up.
      window.dispatchEvent(new StorageEvent('storage', { key: 'pty:restart' }));
      console.debug('[palette] pty restart requested');
      return true;
    },
  },
];

// Group → sorted command list.
const GROUPS = Array.from(
  COMMANDS.reduce((m, c) => {
    (m.get(c.group) ?? m.set(c.group, []).get(c.group)!).push(c);
    return m;
  }, new Map<string, CmdDef[]>()),
);

// ── Top-level verb → CmdDef matcher ─────────────────────────────────────────

function resolveCommand(input: string): { cmd: CmdDef; arg: string } | null {
  const trimmed = input.toLowerCase().trim();
  if (!trimmed) return null;

  // Exact match first (e.g. "clear", "pty restart").
  const exact = COMMANDS.find((c) => c.label === trimmed || c.label === trimmed.replace(/\s+.*$/, '…'));
  if (exact) return { cmd: exact, arg: '' };

  // Prefix match for parameterised commands: "focus soul", "query test helix".
  const prefixed = COMMANDS.find((c) => !c.label.endsWith('…') && trimmed.startsWith(c.label + ' '));
  if (prefixed) return { cmd: prefixed, arg: trimmed.slice(prefixed.label.length + 1) };

  // "query <anything>" — arg is the rest.
  if (trimmed.startsWith('query ')) {
    const queryCmd = COMMANDS.find((c) => c.id === 'query');
    if (queryCmd) return { cmd: queryCmd, arg: trimmed.slice(6) };
  }

  return null;
}

// ── Component ────────────────────────────────────────────────────────────────

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [input, setInput] = useState('');
  const [feedback, setFeedback] = useState<string | null>(null);
  const { setFocusedSibling } = useWebshellData();

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setOpen((o) => !o);
        setInput('');
        setFeedback(null);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const close = useCallback(() => { setOpen(false); setInput(''); setFeedback(null); }, []);

  function execute(raw: string) {
    const resolved = resolveCommand(raw);
    if (!resolved) {
      setFeedback('unknown command');
      return;
    }
    const ok = resolved.cmd.run(resolved.arg, { setFocusedSibling });
    if (!ok) {
      setFeedback('invalid argument');
      return;
    }
    close();
  }

  function selectItem(value: string) {
    // cmdk passes the item's value on select; execute it directly.
    execute(value);
  }

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 100, display: 'flex', alignItems: 'flex-start', justifyContent: 'center', paddingTop: 80, background: 'rgba(0,0,0,0.55)' }}
      onClick={close}
    >
      <div
        style={{ width: 520, background: '#0a0a0f', border: '1px solid #1e293b', borderRadius: 10, overflow: 'hidden', boxShadow: '0 24px 64px rgba(0,0,0,0.8)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <Command>
          {/* Input row */}
          <div style={{ display: 'flex', alignItems: 'center', borderBottom: '1px solid #1e293b', padding: '0 16px' }}>
            <span style={{ color: '#475569', fontFamily: 'monospace', fontSize: 13, marginRight: 8 }}>›</span>
            <Command.Input
              value={input}
              onValueChange={(v) => { setInput(v); setFeedback(null); }}
              placeholder="focus soul · query helix · clear · pty restart"
              onKeyDown={(e) => {
                if (e.key === 'Enter') execute(input);
                if (e.key === 'Escape') close();
              }}
              style={{ flex: 1, padding: '14px 0', background: 'transparent', border: 'none', outline: 'none', color: '#e2e8f0', fontFamily: 'monospace', fontSize: 13 }}
            />
            <span style={{ color: '#334155', fontSize: 10, fontFamily: 'monospace' }}>⌘K</span>
          </div>

          {/* Feedback banner */}
          {feedback && (
            <div style={{ padding: '6px 16px', background: 'rgba(239,68,68,0.12)', color: '#f87171', fontSize: 11, fontFamily: 'monospace' }}>
              {feedback}
            </div>
          )}

          {/* Command list */}
          <Command.List style={{ maxHeight: 340, overflowY: 'auto', padding: '8px 0' }}>
            <Command.Empty style={{ padding: '12px 16px', color: '#475569', fontFamily: 'monospace', fontSize: 12 }}>
              No matching command.
            </Command.Empty>

            {GROUPS.map(([group, cmds]) => (
              <Command.Group key={group} heading={group} style={{ padding: 0 }}>
                {cmds.map((c) => (
                  <Command.Item
                    key={c.id}
                    value={c.label}
                    onSelect={() => selectItem(c.label)}
                    style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '7px 16px', cursor: 'pointer', color: '#94a3b8', fontFamily: 'monospace', fontSize: 12, borderRadius: 0 }}
                  >
                    <span>{c.label}</span>
                    {c.hint && <span style={{ color: '#334155', fontSize: 10 }}>{c.hint}</span>}
                  </Command.Item>
                ))}
              </Command.Group>
            ))}
          </Command.List>

          {/* Footer */}
          <div style={{ borderTop: '1px solid #0f172a', padding: '6px 16px', display: 'flex', gap: 16, color: '#334155', fontSize: 10, fontFamily: 'monospace' }}>
            <span>↵ execute</span>
            <span>↑↓ navigate</span>
            <span>esc close</span>
          </div>
        </Command>
      </div>
    </div>
  );
}
