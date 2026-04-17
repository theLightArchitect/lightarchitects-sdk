/**
 * useBrowserStateReporter — periodically reports current browser UI state
 * to the server via POST /api/browser-state.
 *
 * This allows external processes (e.g. Claude Code) to read the browser's
 * viewport size, panel layout, zoom level, etc. via GET /api/browser-state.
 *
 * Reports every 5 seconds. Stops on unmount.
 */
import { useEffect, useRef } from 'react';
import { useSceneStore } from '../store/sceneState';
import { resolveToken } from '../lib/auth';

const REPORT_INTERVAL_MS = 5_000;

export function useBrowserStateReporter(): void {
  const panelSizes = useSceneStore((s) => s.panelSizes);
  const helixZoom = useSceneStore((s) => s.helixZoom);
  const activePanel = useSceneStore((s) => s.activePanel);
  const steps = useSceneStore((s) => s.steps);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;

    let timeoutId: ReturnType<typeof setTimeout>;

    async function report(): Promise<void> {
      if (!mountedRef.current) return;

      const token = resolveToken();
      if (!token) {
        // No token yet — retry later.
        timeoutId = setTimeout(report, REPORT_INTERVAL_MS);
        return;
      }

      const body = {
        viewport_width: window.innerWidth,
        viewport_height: window.innerHeight,
        terminal_size_percent: panelSizes.terminal,
        helix_size_percent: panelSizes.helix,
        active_panel: activePanel,
        helix_zoom: helixZoom,
        helix_step_count: steps.length,
      };

      try {
        await fetch('/api/browser-state', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Authorization: `Bearer ${token}`,
          },
          body: JSON.stringify(body),
        });
      } catch {
        // Silently swallow — reporting is best-effort.
      }

      if (mountedRef.current) {
        timeoutId = setTimeout(report, REPORT_INTERVAL_MS);
      }
    }

    // Initial report after a short delay (let auth resolve first).
    timeoutId = setTimeout(report, 2_000);

    return () => {
      mountedRef.current = false;
      clearTimeout(timeoutId);
    };
  }, [panelSizes, helixZoom, activePanel, steps]);
}