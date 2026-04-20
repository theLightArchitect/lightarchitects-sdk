import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { resolveToken, getToken, authHeaders } from '$lib/auth';

const SESSION_KEY = 'la_webshell_token';

// Mock history.replaceState to avoid jsdom navigation errors.
const replaceStateMock = vi.fn();

beforeEach(() => {
  sessionStorage.clear();
  vi.stubGlobal('history', { replaceState: replaceStateMock });
  replaceStateMock.mockClear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubLocation(hash: string) {
  vi.stubGlobal('location', {
    hash,
    pathname: '/',
    search: '',
    href: `http://localhost/${hash}`,
  });
}

describe('resolveToken', () => {
  it('reads token from URL hash and stores in sessionStorage', () => {
    stubLocation('#token=abc123hex');
    const token = resolveToken();
    expect(token).toBe('abc123hex');
    expect(sessionStorage.getItem(SESSION_KEY)).toBe('abc123hex');
  });

  it('strips the hash after reading (replaceState called)', () => {
    stubLocation('#token=abc123hex');
    resolveToken();
    expect(replaceStateMock).toHaveBeenCalledWith(null, '', '/');
  });

  it('falls back to sessionStorage when hash has no token', () => {
    sessionStorage.setItem(SESSION_KEY, 'stored-token');
    stubLocation('#unrelated=stuff');
    const token = resolveToken();
    expect(token).toBe('stored-token');
    expect(replaceStateMock).not.toHaveBeenCalled();
  });

  it('returns null when both hash and sessionStorage are absent', () => {
    stubLocation('');
    const token = resolveToken();
    expect(token).toBeNull();
  });

  it('prefers hash token over existing sessionStorage value', () => {
    sessionStorage.setItem(SESSION_KEY, 'old-token');
    stubLocation('#token=new-token');
    const token = resolveToken();
    expect(token).toBe('new-token');
    expect(sessionStorage.getItem(SESSION_KEY)).toBe('new-token');
  });
});

describe('getToken', () => {
  it('returns null when sessionStorage is empty', () => {
    expect(getToken()).toBeNull();
  });

  it('returns stored token', () => {
    sessionStorage.setItem(SESSION_KEY, 'my-token');
    expect(getToken()).toBe('my-token');
  });
});

describe('authHeaders', () => {
  it('returns Authorization header when token is present', () => {
    sessionStorage.setItem(SESSION_KEY, 'test-token-xyz');
    const headers = authHeaders();
    expect(headers).toEqual({ Authorization: 'Bearer test-token-xyz' });
  });

  it('returns empty object when no token', () => {
    const headers = authHeaders();
    expect(Object.keys(headers)).toHaveLength(0);
  });
});
