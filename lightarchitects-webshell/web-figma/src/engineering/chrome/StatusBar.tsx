// ============================================================================
// File: web-figma/src/engineering/chrome/StatusBar.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Status pills [AYIN ●] [HELIX ●] [BUILD ●] [PTY ●]
// ============================================================================

import React from 'react';
import { useWebshellData } from '../store/EngineeringProvider';
import type { AyinConnStatus } from '../store/sceneState';

const STATUS_COLORS: Record<AyinConnStatus, string> = {
  connected:   '#22c55e',
  reconnecting: '#f59e0b',
  offline:     '#ef4444',
};

interface PillProps {
  label: string;
  color: string;
}

function Pill({ label, color }: PillProps) {
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        padding: '2px 8px',
        borderRadius: 4,
        background: 'rgba(17,24,39,0.85)',
        border: '1px solid rgba(30,41,59,0.8)',
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        fontSize: 11,
        color: '#94a3b8',
        backdropFilter: 'blur(4px)',
      }}
    >
      <span style={{ width: 7, height: 7, borderRadius: '50%', background: color, boxShadow: `0 0 4px ${color}`, display: 'inline-block' }} />
      {label}
    </span>
  );
}

export function StatusBar() {
  const { ayinStatus } = useWebshellData();
  const ayinColor = STATUS_COLORS[ayinStatus];

  return (
    <div
      style={{
        position: 'fixed',
        bottom: 12,
        left: 12,
        display: 'flex',
        gap: 6,
        zIndex: 50,
        pointerEvents: 'none',
      }}
    >
      <Pill label="AYIN" color={ayinColor} />
      <Pill label="HELIX" color="#f59e0b" />
      <Pill label="BUILD" color="#f59e0b" />
      <Pill label="PTY" color="#f59e0b" />
    </div>
  );
}
