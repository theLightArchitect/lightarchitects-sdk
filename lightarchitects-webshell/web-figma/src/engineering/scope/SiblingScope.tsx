// ============================================================================
// File: web-figma/src/engineering/scope/SiblingScope.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Single sibling oscilloscope row — stub (Phase 4 implements waveform)
// ============================================================================

import React from 'react';

interface SiblingScopeProps {
  sibling: string;
  /** Amplitude samples 0–1, max BUF_LEN entries. */
  samples: number[];
  focused: boolean;
}

export function SiblingScope({ sibling, samples, focused }: SiblingScopeProps) {
  const latest = samples[samples.length - 1] ?? 0;
  const color = focused ? '#00f5ff' : '#475569';

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        padding: '3px 0',
        opacity: focused ? 1 : 0.7,
      }}
    >
      <span
        style={{
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
          fontSize: 10,
          color,
          width: 56,
          textTransform: 'uppercase',
          letterSpacing: 1,
        }}
      >
        {sibling}
      </span>
      {/* Phase 4: replace with SiblingWave canvas. */}
      <div
        style={{
          flex: 1,
          height: 2,
          background: `rgba(0,245,255,${(latest * 0.8 + 0.05).toFixed(2)})`,
          borderRadius: 1,
        }}
      />
    </div>
  );
}
