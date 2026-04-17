// ============================================================================
// File: web-figma/src/engineering/palette/CommandPalette.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Cmd-K command palette stub (Phase 5 adds full command set)
// Security: commands are a static whitelist — no dynamic eval
// ============================================================================

import React, { useEffect, useState } from 'react';
import { Command } from 'cmdk';
import { useWebshellData } from '../store/EngineeringProvider';
import { SIBLINGS } from '../store/sceneState';

const SAFE_COMMANDS = ['focus', 'query', 'clear', 'pty restart'] as const;

function isSafeCommand(cmd: string): boolean {
  const lower = cmd.toLowerCase().trim();
  return SAFE_COMMANDS.some((safe) => lower === safe || lower.startsWith(`${safe} `));
}

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState('');
  const { setFocusedSibling } = useWebshellData();

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setOpen((o) => !o);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!open) return null;

  function execute(cmd: string) {
    if (!isSafeCommand(cmd)) return;
    const [verb, arg] = cmd.toLowerCase().trim().split(' ');
    if (verb === 'focus' && arg && SIBLINGS.includes(arg as never)) {
      setFocusedSibling(arg);
    } else if (verb === 'focus' && !arg) {
      setFocusedSibling(null);
    }
    setValue('');
    setOpen(false);
  }

  return (
    <div style={{ position: 'fixed', inset: 0, zIndex: 100, display: 'flex', alignItems: 'flex-start', justifyContent: 'center', paddingTop: 80, background: 'rgba(0,0,0,0.4)' }} onClick={() => setOpen(false)}>
      <div style={{ width: 480, background: '#0a0a0f', border: '1px solid #1e293b', borderRadius: 8, overflow: 'hidden' }} onClick={(e) => e.stopPropagation()}>
        <Command>
          <Command.Input
            value={value}
            onValueChange={setValue}
            placeholder="Type a command…"
            onKeyDown={(e) => { if (e.key === 'Enter') execute(value); if (e.key === 'Escape') setOpen(false); }}
            style={{ width: '100%', padding: '12px 16px', background: 'transparent', border: 'none', outline: 'none', color: '#e2e8f0', fontFamily: "'JetBrains Mono', monospace", fontSize: 14 }}
          />
          <Command.List style={{ padding: 8 }}>
            {SIBLINGS.map((s) => (
              <Command.Item key={s} value={`focus ${s}`} onSelect={() => execute(`focus ${s}`)} style={{ padding: '6px 12px', borderRadius: 4, cursor: 'pointer', color: '#94a3b8', fontFamily: "'JetBrains Mono', monospace", fontSize: 12 }}>
                focus {s}
              </Command.Item>
            ))}
          </Command.List>
        </Command>
      </div>
    </div>
  );
}
