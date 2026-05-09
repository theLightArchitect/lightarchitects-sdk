// ============================================================================
// WebSocket client — per-build PTY terminal bridge
// Protocol: binary frames = PTY stdin/stdout; text frames = JSON control
// ============================================================================

import { getToken } from './auth';

export class TerminalWS {
  private ws: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private onMessage: ((data: Uint8Array) => void) | null = null;
  private onOpen: (() => void) | null = null;
  private onClose: (() => void) | null = null;
  private manualClose = false;
  private buildId: string | null;

  constructor(
    buildId: string | null,
    messageHandler: (data: Uint8Array) => void,
    openHandler?: () => void,
    closeHandler?: () => void,
  ) {
    this.buildId = buildId;
    this.onMessage = messageHandler;
    this.onOpen = openHandler ?? null;
    this.onClose = closeHandler ?? null;
  }

  connect(): void {
    this.manualClose = false;
    this._connect();
  }

  private _connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) return;

    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    // Use build-bound PTY if a build exists, otherwise standalone PTY (inherits server CWD)
    const path = this.buildId
      ? `/api/builds/${this.buildId}/terminal/ws`
      : '/api/terminal/ws';
    const url = `${proto}//${window.location.host}${path}`;
    const token = getToken() ?? '';
    // Bearer token delivered via Sec-WebSocket-Protocol subprotocol header —
    // the webshell validates it before upgrading the connection.
    this.ws = new WebSocket(url, token ? [`bearer.${token}`] : []);
    this.ws.binaryType = 'arraybuffer';

    this.ws.onopen = () => {
      this.onOpen?.();
    };

    this.ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        this.onMessage?.(new Uint8Array(event.data));
      }
    };

    this.ws.onclose = () => {
      this.onClose?.();
      if (!this.manualClose) {
        this.reconnectTimer = setTimeout(() => this._connect(), 3_000);
      }
    };

    this.ws.onerror = () => {
      this.ws?.close();
    };
  }

  /** Send PTY stdin as a binary frame (webshell reads raw bytes from PTY). */
  sendText(data: string): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(new TextEncoder().encode(data));
    }
  }

  /** Send a terminal resize event as a JSON text frame. */
  sendResize(cols: number, rows: number): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'resize', cols, rows }));
    }
  }

  disconnect(): void {
    this.manualClose = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  get connected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }
}

// ============================================================================
// Agent WebSocket client — native agent bridge (AgentEvent streaming)
// Protocol: text frames = JSON AgentEvent lines
// ============================================================================

const MAX_FRAME_BYTES = 2 * 1024 * 1024; // 2 MiB — defense-in-depth against oversized frames

function isValidAgentEvent(parsed: Record<string, unknown>): parsed is import('./types').AgentEvent {
  if (typeof parsed.type !== 'string') return false;
  switch (parsed.type) {
    case 'text':
      return typeof parsed.chunk === 'string';
    case 'thinking':
      return typeof parsed.content === 'string';
    case 'tool_start':
      return typeof parsed.name === 'string' && typeof parsed.id === 'string';
    case 'tool_complete':
      return (
        typeof parsed.id === 'string' &&
        typeof parsed.success === 'boolean' &&
        typeof parsed.duration_ms === 'number'
      );
    case 'status_update':
      return typeof parsed.text === 'string';
    case 'error':
      return typeof parsed.message === 'string';
    case 'complete':
      return parsed.reason !== undefined;
    case 'token_usage':
      return typeof parsed.input === 'number' && typeof parsed.output === 'number';
    case 'heartbeat':
      return true;
    default:
      return false;
  }
}

export class AgentWS {
  private ws: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private onEvent: ((ev: import('./types').AgentEvent) => void) | null = null;
  private onOpen: (() => void) | null = null;
  private onClose: (() => void) | null = null;
  private manualClose = false;
  private buildId: string;

  constructor(
    buildId: string,
    eventHandler: (ev: import('./types').AgentEvent) => void,
    openHandler?: () => void,
    closeHandler?: () => void,
  ) {
    this.buildId = buildId;
    this.onEvent = eventHandler;
    this.onOpen = openHandler ?? null;
    this.onClose = closeHandler ?? null;
  }

  connect(): void {
    this.manualClose = false;
    this._connect();
  }

  private _connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) return;

    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const path = `/api/builds/${this.buildId}/agent/ws`;
    const url = `${proto}//${window.location.host}${path}`;
    const token = getToken() ?? '';
    this.ws = new WebSocket(url, token ? [`bearer.${token}`] : []);

    this.ws.onopen = () => {
      this.onOpen?.();
    };

    this.ws.onmessage = (event) => {
      if (typeof event.data === 'string') {
        if (event.data.length > MAX_FRAME_BYTES) {
          this.onEvent?.({ type: 'error', message: `Frame too large (${event.data.length} bytes)` });
          return;
        }
        try {
          const parsed = JSON.parse(event.data) as Record<string, unknown>;
          // ControlResponse types (ack/reject/pong/etc.) are NOT AgentEvents — ignore them.
          const controlTypes = ['ack', 'reject', 'permission_resolved', 'interrupted', 'pong', 'server_error'];
          if (typeof parsed.type === 'string' && controlTypes.includes(parsed.type)) {
            return;
          }
          if (!isValidAgentEvent(parsed)) {
            // Malformed event — surface as error so the UI doesn't silently drop it
            this.onEvent?.({ type: 'error', message: `Malformed event: ${String(parsed.type)}` });
            return;
          }
          this.onEvent?.(parsed);
        } catch {
          // Unparseable line — treat as raw text event only if within size bounds (already checked)
          this.onEvent?.({ type: 'text', chunk: event.data });
        }
      }
    };

    this.ws.onclose = () => {
      this.onClose?.();
      if (!this.manualClose) {
        this.reconnectTimer = setTimeout(() => this._connect(), 3_000);
      }
    };

    this.ws.onerror = () => {
      this.ws?.close();
    };
  }

  /** Send a user message to start a new agent turn. */
  sendMessage(text: string): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'send_message', text }));
    }
  }

  /** Send an interrupt to cancel the current turn. */
  sendInterrupt(): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'interrupt' }));
    }
  }

  /** Send a steer message to append mid-turn instructions. */
  sendSteer(text: string): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ action: 'steer', text }));
    }
  }

  disconnect(): void {
    this.manualClose = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  get connected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }
}
