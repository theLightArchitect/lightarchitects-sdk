/**
 * useTerminalSocket — owns the WebSocket connection to /api/terminal/ws.
 *
 * ## Protocol (matches Rust session.rs exactly)
 *
 *   Server → browser:  Binary frames  — raw PTY stdout bytes
 *   Browser → server:  Binary frames  — keystrokes/paste (UTF-8 encoded)
 *   Browser → server:  Text frames    — JSON control:
 *                        {"type":"resize","cols":N,"rows":M}
 *                        {"type":"ping"}
 *
 * ## Auth
 *
 *   Sub-protocol: `bearer.<token>` — validated by ws_handler.rs before upgrade.
 *
 * ## Reconnect
 *
 *   Exponential backoff: 1s → 2s → 4s → 8s → 16s → 30s cap.
 *   On reconnect the current terminal dimensions are sent immediately so the
 *   PTY is correctly sized without waiting for the next resize event.
 */
import { useEffect } from 'react';
import type { Terminal } from '@xterm/xterm';
import { resolveToken } from '../../lib/auth';

export function useTerminalSocket(terminal: Terminal | null): void {
  useEffect(() => {
    if (!terminal) return;

    // Capture in a non-null local so closures below see a stable Terminal reference.
    const term = terminal;
    let aborted = false;
    let ws: WebSocket | null = null;
    const enc = new TextEncoder();

    // Forward terminal input as binary frames (raw UTF-8 bytes → PTY stdin).
    const onData = term.onData((data) => {
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(enc.encode(data));
      }
    });

    // Forward terminal resize as a JSON text control frame.
    const onResize = term.onResize(({ cols, rows }) => {
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'resize', cols, rows }));
      }
    });

    function connect(attempt: number): void {
      if (aborted) return;

      const token = resolveToken();
      const scheme = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const url = `${scheme}//${window.location.host}/api/terminal/ws`;
      const protocols = token ? [`bearer.${token}`] : [];

      const socket = new WebSocket(url, protocols);
      socket.binaryType = 'arraybuffer';
      ws = socket;

      socket.onopen = () => {
        // Send current dimensions immediately so the PTY starts at the right size.
        socket.send(
          JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }),
        );
      };

      socket.onmessage = (event) => {
        // Binary frames are raw PTY output → write directly to the terminal.
        if (event.data instanceof ArrayBuffer) {
          term.write(new Uint8Array(event.data));
        }
        // Text frames from the server are not currently used but tolerated.
      };

      socket.onclose = () => {
        ws = null;
        if (aborted) return;
        const delay = Math.min(1_000 * (1 << Math.min(attempt, 5)), 30_000);
        setTimeout(() => connect(attempt + 1), delay);
      };

      // onerror always fires before onclose — onclose handles the reconnect.
      socket.onerror = () => {};
    }

    connect(0);

    return () => {
      aborted = true;
      ws?.close();
      onData.dispose();
      onResize.dispose();
    };
  }, [terminal]);
}
