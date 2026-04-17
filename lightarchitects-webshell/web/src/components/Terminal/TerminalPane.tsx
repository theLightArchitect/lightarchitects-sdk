/**
 * TerminalPane — xterm.js terminal mounted in a resizable container.
 *
 * Lifecycle:
 *   1. On mount: creates Terminal + addons, opens into the container div.
 *   2. Attempts WebGL renderer; falls back to Canvas on context loss.
 *   3. FitAddon sizes the PTY to the container — a ResizeObserver re-fits
 *      whenever the panel is dragged, which triggers `terminal.onResize`
 *      and forwards the new dimensions to the WS via useTerminalSocket.
 *   4. On unmount: disposes all addons and the terminal.
 */
import { useEffect, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import { CanvasAddon } from '@xterm/addon-canvas';
import '@xterm/xterm/css/xterm.css';
import { useTerminalSocket } from './useTerminalSocket';

const TERMINAL_THEME = {
  background:    '#0a0a0f',
  foreground:    '#e2e8f0',
  cursor:        '#00f5ff',
  cursorAccent:  '#0a0a0f',
  black:         '#1e293b',
  red:           '#ef4444',
  green:         '#22c55e',
  yellow:        '#f59e0b',
  blue:          '#3b82f6',
  magenta:       '#FF1493',
  cyan:          '#00f5ff',
  white:         '#e2e8f0',
  brightBlack:   '#334155',
  brightRed:     '#fca5a5',
  brightGreen:   '#86efac',
  brightYellow:  '#fcd34d',
  brightBlue:    '#93c5fd',
  brightMagenta: '#f9a8d4',
  brightCyan:    '#67e8f9',
  brightWhite:   '#f8fafc',
} as const;

interface TerminalPaneProps {
  style?: CSSProperties;
}

export function TerminalPane({ style }: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [terminal, setTerminal] = useState<Terminal | null>(null);

  // Wire the WebSocket connection — called at component level so it's not
  // conditional, but the hook handles terminal === null gracefully.
  useTerminalSocket(terminal);

  // Create the terminal instance once the container is mounted.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const term = new Terminal({
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      fontSize: 14,
      lineHeight: 1.2,
      theme: TERMINAL_THEME,
      cursorBlink: true,
      scrollback: 5_000,
      allowProposedApi: true, // required for WebglAddon
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());

    // Try GPU-accelerated renderer; fall back to canvas on WebGL context loss.
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl.dispose();
        term.loadAddon(new CanvasAddon());
      });
      term.loadAddon(webgl);
    } catch {
      term.loadAddon(new CanvasAddon());
    }

    term.open(el);
    fit.fit();

    fitRef.current = fit;
    setTerminal(term);

    return () => {
      term.dispose();
      fitRef.current = null;
      setTerminal(null);
    };
  }, []);

  // Re-fit whenever the container size changes (panel drag, window resize).
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const obs = new ResizeObserver(() => {
      fitRef.current?.fit();
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  return (
    <div
      ref={containerRef}
      style={{
        width: '100%',
        height: '100%',
        overflow: 'hidden',
        background: '#0a0a0f',
        ...style,
      }}
    />
  );
}
