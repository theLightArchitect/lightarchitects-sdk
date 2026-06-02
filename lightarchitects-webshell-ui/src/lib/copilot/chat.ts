/**
 * Shared copilot event handling and send logic.
 *
 * Both CopilotDrawer (compact view) and CopilotSurface (immersive view) are
 * views of the same conversation. This module is the single source of truth
 * for writing to that shared state so both views always show identical messages.
 *
 * Message format for system events uses structured prefixes so any view can
 * detect and render them appropriately:
 *   TOOL:<name>\n<input json>
 *   DONE:<tool_id>\n<ok|err> · <ms>ms\n<result>
 *   STATUS:<text>
 *   ERR:<message>
 */
import { get } from 'svelte/store';
import {
  copilotMessages, copilotLoading, copilotGrounding,
  snapshotContextForCopilot,
} from '$lib/stores';
import type { AgentEvent, CopilotMessage, SiblingId } from '$lib/types';
import * as sessionMgr from '$lib/copilot/session';
import { api } from '$lib/api';

export function addCopilotMessage(
  role: CopilotMessage['role'],
  content: string,
  sibling?: SiblingId,
  kind?: CopilotMessage['kind'],
): CopilotMessage {
  const msg: CopilotMessage = {
    id: crypto.randomUUID(), role, content, sibling,
    timestamp: new Date().toISOString(), kind,
  };
  copilotMessages.update(m => [...m, msg]);
  return msg;
}

/**
 * Process a streaming agent event into the shared copilotMessages store.
 *
 * @param ev         - The agent event from the SSE stream.
 * @param onComplete - Optional callback fired when the turn completes (e.g.
 *                     particle burst in Surface, voice playback in Drawer).
 */
export function handleCopilotEvent(ev: AgentEvent, onComplete?: () => void): void {
  switch (ev.type) {
    case 'text':
      if (!get(copilotLoading)) copilotLoading.set(true);
      copilotMessages.update((msgs) => {
        const upd = [...msgs];
        const last = upd[upd.length - 1];
        if (last?.role === 'assistant' && get(copilotLoading)) {
          upd[upd.length - 1] = { ...last, content: last.content + ev.chunk };
        } else {
          upd.push({
            id: crypto.randomUUID(), role: 'assistant',
            content: ev.chunk, timestamp: new Date().toISOString(),
          });
        }
        return upd;
      });
      break;
    case 'thinking':
      addCopilotMessage('system', ev.content, undefined, 'thinking');
      break;
    case 'tool_start':
      addCopilotMessage('system', `TOOL:${ev.name}\n${JSON.stringify(ev.input, null, 2)}`);
      break;
    case 'tool_complete':
      addCopilotMessage(
        'system',
        `DONE:${ev.id}\n${ev.success ? 'ok' : 'err'} · ${ev.duration_ms}ms${ev.result ? '\n' + ev.result.slice(0, 400) : ''}`,
      );
      break;
    case 'status_update':
      addCopilotMessage('system', `STATUS:${ev.text}`);
      break;
    case 'error':
      addCopilotMessage('system', `ERR:${ev.message}`);
      copilotLoading.set(false);
      break;
    case 'complete':
      copilotLoading.set(false);
      onComplete?.();
      break;
    case 'token_usage':
    case 'heartbeat':
      break;
    default:
      break;
  }
}

/**
 * Send a message via the native SSE path. Intended for views that don't have
 * their own command parsing or WS bridge (i.e. CopilotSurface). CopilotDrawer
 * uses this indirectly via handleCopilotEvent only — its send path stays local
 * because it handles /commands, WS bridge, and legacy HTTP fallback.
 *
 * @param text       - The user message text (already trimmed).
 * @param cwd        - Working directory for the session.
 * @param onComplete - Optional callback for turn-complete side effects.
 */
export async function sendCopilotNative(
  text: string,
  cwd: string,
  onComplete?: () => void,
): Promise<void> {
  addCopilotMessage('user', text);
  copilotLoading.set(true);
  try {
    await sessionMgr.withSession(cwd, async (bid) => {
      const ctx = snapshotContextForCopilot();
      const { grounding } = await api.copilotChatNative(
        bid, text,
        (ev) => handleCopilotEvent(ev as AgentEvent, onComplete),
        { recentEvents: ctx.recentEvents, uiContext: ctx.uiContext },
      );
      if (grounding !== null) copilotGrounding.set(grounding);
    });
  } catch (err) {
    handleCopilotEvent({
      type: 'error',
      message: err instanceof Error ? err.message : 'Send failed',
    } as AgentEvent);
  }
}
