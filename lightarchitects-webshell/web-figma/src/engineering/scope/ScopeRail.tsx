// ============================================================================
// File: web-figma/src/engineering/scope/ScopeRail.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Floating oscilloscope overlay top-right 240×384. Reads SiblingWave
//          refs from context; canvas rendering is zero-React via rAF in SiblingScope.
// ============================================================================

import React from 'react';
import { useWebshellData } from '../store/EngineeringProvider';
import { SiblingScope } from './SiblingScope';
import { SIBLINGS } from '../store/sceneState';

// Sibling → hex colour (mirrors EngineeringProvider ACTOR_COLORS).
const SCOPE_COLORS: Record<string, string> = {
  soul:    '#7C3AED',
  eva:     '#FF1493',
  corso:   '#00BFFF',
  quantum: '#B44AFF',
  seraph:  '#FF0040',
  larc:    '#F59E0B',
  ayin:    '#FF6D00',
};

export function ScopeRail() {
  const { waves, focusedSibling } = useWebshellData();

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
          fontFamily: 'monospace',
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
          wave={waves[s]}
          color={SCOPE_COLORS[s] ?? '#94a3b8'}
          label={s}
          focused={focusedSibling === s}
        />
      ))}
    </div>
  );
}
