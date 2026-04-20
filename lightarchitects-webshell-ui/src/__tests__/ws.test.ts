import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// We need to mock the WebSocket global AND auth.ts before importing TerminalWS.
// Vitest hoists vi.mock() calls, so declare them before imports.
vi.mock('$lib/auth', () => ({
  getToken: vi.fn(() => 'test-bearer-token'),
}));

import { TerminalWS } from '$lib/ws';

// ── Mock WebSocket ──────────────────────────────────────────────────────────

type MockWSInstance = {
  binaryType: string;
  readyState: number;
  send: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onmessage: ((e: { data: unknown }) => void) | null;
  onerror: (() => void) | null;
};

let lastWS: MockWSInstance | null = null;
let constructorArgs: [string, string[]] | null = null;

// Must use regular function (not arrow) so `new MockWebSocket()` works.
// eslint-disable-next-line prefer-arrow-callback
const MockWebSocket = vi.fn(function MockWSCtor(this: MockWSInstance, url: string, protocols: string[]) {
  constructorArgs = [url, protocols];
  const self = this;
  self.binaryType = 'blob';
  self.readyState = 1; // WebSocket.OPEN
  self.send = vi.fn();
  self.close = vi.fn();
  self.onopen = null;
  self.onclose = null;
  self.onmessage = null;
  self.onerror = null;
  lastWS = self;
});

// Attach static constants so callers can use WebSocket.OPEN etc.
(MockWebSocket as unknown as Record<string, number>).OPEN = 1;
(MockWebSocket as unknown as Record<string, number>).CONNECTING = 0;
(MockWebSocket as unknown as Record<string, number>).CLOSING = 2;
(MockWebSocket as unknown as Record<string, number>).CLOSED = 3;

beforeEach(() => {
  vi.stubGlobal('WebSocket', MockWebSocket);
  vi.stubGlobal('location', {
    protocol: 'http:',
    host: 'localhost:8733',
  });
  lastWS = null;
  constructorArgs = null;
  MockWebSocket.mockClear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// ── Tests ──────────────────────────────────────────────────────────────────

describe('TerminalWS constructor and connect', () => {
  it('builds URL with correct buildId path', () => {
    const tw = new TerminalWS('build-abc', () => {});
    tw.connect();
    expect(constructorArgs![0]).toBe('ws://localhost:8733/api/builds/build-abc/terminal/ws');
  });

  it('includes bearer subprotocol when token is set', () => {
    const tw = new TerminalWS('build-abc', () => {});
    tw.connect();
    expect(constructorArgs![1]).toContain('bearer.test-bearer-token');
  });

  it('sets binaryType to arraybuffer', () => {
    const tw = new TerminalWS('build-abc', () => {});
    tw.connect();
    expect(lastWS!.binaryType).toBe('arraybuffer');
  });
});

describe('message handling', () => {
  it('calls messageHandler with Uint8Array on binary frame', () => {
    const handler = vi.fn();
    const tw = new TerminalWS('build-xyz', handler);
    tw.connect();

    const buf = new ArrayBuffer(5);
    new Uint8Array(buf).set([104, 101, 108, 108, 111]); // "hello"
    lastWS!.onmessage?.({ data: buf });

    expect(handler).toHaveBeenCalledOnce();
    const received = handler.mock.calls[0][0] as Uint8Array;
    expect(received).toBeInstanceOf(Uint8Array);
    expect(Array.from(received)).toEqual([104, 101, 108, 108, 111]);
  });

  it('does not call handler for non-ArrayBuffer data', () => {
    const handler = vi.fn();
    const tw = new TerminalWS('build-xyz', handler);
    tw.connect();
    lastWS!.onmessage?.({ data: 'string data' });
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('sendText', () => {
  it('sends PTY stdin as Uint8Array binary frame', () => {
    const tw = new TerminalWS('build-send', () => {});
    tw.connect();

    tw.sendText('ls\n');

    expect(lastWS!.send).toHaveBeenCalledOnce();
    const sent = lastWS!.send.mock.calls[0][0];
    // Cross-realm instanceof check: use ArrayBuffer.isView + TextDecoder roundtrip.
    expect(ArrayBuffer.isView(sent)).toBe(true);
    expect(new TextDecoder().decode(sent as Uint8Array)).toBe('ls\n');
  });
});

describe('sendResize', () => {
  it('sends resize as JSON text frame', () => {
    const tw = new TerminalWS('build-resize', () => {});
    tw.connect();

    tw.sendResize(120, 40);

    expect(lastWS!.send).toHaveBeenCalledOnce();
    const sent = lastWS!.send.mock.calls[0][0];
    expect(typeof sent).toBe('string');
    const parsed = JSON.parse(sent as string);
    expect(parsed).toEqual({ type: 'resize', cols: 120, rows: 40 });
  });
});

describe('lifecycle', () => {
  it('connected returns true when WebSocket is OPEN', () => {
    const tw = new TerminalWS('build-lc', () => {});
    tw.connect();
    expect(tw.connected).toBe(true);
  });

  it('calls openHandler on open event', () => {
    const onOpen = vi.fn();
    const tw = new TerminalWS('build-lc', () => {}, onOpen);
    tw.connect();
    lastWS!.onopen?.();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('calls closeHandler on close event', () => {
    const onClose = vi.fn();
    const tw = new TerminalWS('build-lc', () => {}, undefined, onClose);
    tw.connect();
    // manualClose = true prevents reconnect timer
    tw.disconnect();
    lastWS!.onclose?.();
    expect(onClose).toHaveBeenCalledOnce();
  });
});
