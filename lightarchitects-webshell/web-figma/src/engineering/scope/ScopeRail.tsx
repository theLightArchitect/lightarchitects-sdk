// ============================================================================
// File: web-figma/src/engineering/scope/ScopeRail.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Floating oscilloscope overlay top-right 240×384 (Phase 4 full impl)
// ============================================================================

import React from 'react';
import { useWebshellData } from '../store/EngineeringProvider';
import { SiblingScope } from './SiblingScope';
import { SIBLINGS } from '../store/sceneState';

export function ScopeRail() {
  const { strandWaves, focusedSibling } = useWebshellData();

  return (
    <div
      style={{
        position: 'fixed',
        top: 12,
        right: 12,
        width: 240,
        background: 'rgba(10,10,15,0.82)',
        border: '1px solid rgba(30,41,59,0.8)',
        borderRadius: 8,
        padding: '10px 12px',
        backdropFilter: 'blur(8px)',
        zIndex: 50,
        pointerEvents: 'none',
      }}
    >
      <div
        style={{
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
          fontSize: 9,
          color: '#475569',
          letterSpacing: 2,
          marginBottom: 8,
          textTransform: 'uppercase',
        }}
      >
        SIBLING ACTIVATIONS
      </div>
      {SIBLINGS.map((s) => (
        <SiblingScope
          key={s}
          sibling={s}
          samples={strandWaves[s]?.samples ?? []}
          focused={focusedSibling === s}
        />
      ))}
    </div>
  );
}
