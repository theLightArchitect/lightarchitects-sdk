/**
 * useEventSource — connects to /api/events over SSE via fetch() (not EventSource,
 * which doesn't support the Authorization header).
 *
 * Token resolution order:
 *   1. URL hash fragment:  #token=<value>  (set by the Rust binary on launch)
 *   2. sessionStorage:     webshell_token
 *
 * On connection failure the hook respects the backend's synthetic lag / status
 * events and updates the Zustand store accordingly.
 */
import { useEffect } from 'react';
import { useSceneStore } from '../store/sceneState';
import { resolveToken } from '../lib/auth';

/** Parses one SSE `data:` line and dispatches into the scene store. */
function dispatchLine(
  line: string,
  addStep: (id: string, actor: string, action: string) => void,
  setAyinStatus: ReturnType<typeof useSceneStore.getState>['setAyinStatus'],
  spawnOrb: (queryId: string, hitStepIds: string[]) => void,
  focusPanel: ReturnType<typeof useSceneStore.getState>['focusPanel'],
  setPanelVisibility: ReturnType<typeof useSceneStore.getState>['setPanelVisibility'],
  resizePanels: ReturnType<typeof useSceneStore.getState>['resizePanels'],
  setHelixZoom: ReturnType<typeof useSceneStore.getState>['setHelixZoom'],
  pushNotification: ReturnType<typeof useSceneStore.getState>['pushNotification'],
) {
  const data = line.startsWith('data: ') ? line.slice(6) : null;
  if (!data) return;

  let msg: Record<string, unknown>;
  try {
    msg = JSON.parse(data) as Record<string, unknown>;
  } catch {
    return;
  }

  const type = msg['type'];

  if (type === 'ayin_span') {
    const span = msg as { id?: string; actor?: string; action?: string };
    const id = String(span.id ?? crypto.randomUUID());
    const actor = String(span.actor ?? 'unknown');
    const action = String(span.action ?? '');
    addStep(id, actor, action);
  } else if (type === 'ayin_status') {
    const inner = msg['status'] as Record<string, unknown> | undefined;
    if (inner?.['connected'] === true) {
      setAyinStatus({ connected: true, reconnecting: false, attempt: 0 });
    } else if (inner?.['reconnecting'] !== undefined) {
      const attempt = Number((inner['reconnecting'] as Record<string, unknown>)?.['attempt'] ?? 0);
      setAyinStatus({ connected: false, reconnecting: true, attempt });
    } else {
      setAyinStatus({ connected: false, reconnecting: false, attempt: 0 });
    }
  } else if (type === 'helix_entry') {
    // A helix retrieval event: spawn an orb toward the matching steps.
    const retrieval = msg as { query_id?: string; hit_step_ids?: string[] };
    const queryId = String(retrieval.query_id ?? crypto.randomUUID());
    const hitStepIds = Array.isArray(retrieval.hit_step_ids)
      ? retrieval.hit_step_ids.map(String)
      : [];
    spawnOrb(queryId, hitStepIds);
  } else if (type === 'control') {
    // Control command from an external process (e.g. Claude Code).
    const cmd = msg as {
      command?: string;
      panel?: string;
      visible?: boolean;
      terminal?: number;
      helix?: number;
      level?: number;
      message?: string;
    };
    const command = cmd.command;
    if (command === 'focus_panel') {
      focusPanel(String(cmd.panel ?? 'terminal'));
    } else if (command === 'set_panel_visibility') {
      setPanelVisibility(String(cmd.panel ?? 'terminal'), cmd.visible !== false);
    } else if (command === 'resize_panels') {
      resizePanels(Number(cmd.terminal ?? 50), Number(cmd.helix ?? 50));
    } else if (command === 'set_helix_zoom') {
      setHelixZoom(Number(cmd.level ?? 5));
    } else if (command === 'notify') {
      pushNotification(String(cmd.message ?? ''), String((msg as Record<string, unknown>)['level'] ?? 'info'));
    }
  } else if (type === 'lag') {
    // Synthetic lag event from the backend — not actionable at UI layer.
    console.warn('[sse] lag event received, skipped =', msg['skipped']);
  }
}

export function useEventSource(): void {
  const addStep = useSceneStore((s) => s.addStep);
  const setAyinStatus = useSceneStore((s) => s.setAyinStatus);
  const spawnOrb = useSceneStore((s) => s.spawnOrb);
  const focusPanel = useSceneStore((s) => s.focusPanel);
  const setPanelVisibility = useSceneStore((s) => s.setPanelVisibility);
  const resizePanels = useSceneStore((s) => s.resizePanels);
  const setHelixZoom = useSceneStore((s) => s.setHelixZoom);
  const pushNotification = useSceneStore((s) => s.pushNotification);

  useEffect(() => {
    let aborted = false;
    const controller = new AbortController();

    async function connect(attempt: number): Promise<void> {
      if (aborted) return;

      const token = resolveToken();
      setAyinStatus({ connected: false, reconnecting: attempt > 0, attempt });

      let response: Response;
      try {
        response = await fetch('/api/events', {
          headers: token ? { Authorization: `Bearer ${token}` } : {},
          signal: controller.signal,
        });
      } catch (err) {
        if (aborted) return;
        const delay = Math.min(1000 * (1 << Math.min(attempt, 5)), 30_000);
        console.warn(`[sse] connect failed (attempt ${attempt}), retry in ${delay}ms:`, err);
        await new Promise((r) => setTimeout(r, delay));
        return connect(attempt + 1);
      }

      if (!response.ok || !response.body) {
        const delay = Math.min(1000 * (1 << Math.min(attempt, 5)), 30_000);
        console.warn(`[sse] HTTP ${response.status}, retry in ${delay}ms`);
        await new Promise((r) => setTimeout(r, delay));
        return connect(attempt + 1);
      }

      setAyinStatus({ connected: true, reconnecting: false, attempt: 0 });

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buf = '';

      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done || aborted) break;
          buf += decoder.decode(value, { stream: true });
          // SSE events are delimited by double-newline.
          let boundary: number;
          while ((boundary = buf.indexOf('\n\n')) !== -1) {
            const block = buf.slice(0, boundary);
            buf = buf.slice(boundary + 2);
            for (const line of block.split('\n')) {
              dispatchLine(line, addStep, setAyinStatus, spawnOrb, focusPanel, setPanelVisibility, resizePanels, setHelixZoom, pushNotification);
            }
          }
        }
      } catch (err) {
        if (aborted) return;
        console.warn('[sse] stream error:', err);
      }

      if (!aborted) {
        setAyinStatus({ connected: false, reconnecting: true, attempt: attempt + 1 });
        const delay = Math.min(1000 * (1 << Math.min(attempt, 5)), 30_000);
        await new Promise((r) => setTimeout(r, delay));
        return connect(attempt + 1);
      }
    }

    void connect(0);

    return () => {
      aborted = true;
      controller.abort();
    };
  }, [addStep, setAyinStatus, spawnOrb, focusPanel, setPanelVisibility, resizePanels, setHelixZoom, pushNotification]);
}
