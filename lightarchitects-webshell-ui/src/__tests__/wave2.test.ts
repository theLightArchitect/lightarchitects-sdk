import { describe, it, expect } from 'vitest';

// ── atmosphere.ts ──────────────────────────────────────────────────────────

describe('atmosphere', () => {
  it('exports ATMOSPHERE_SOURCE_COLORS with squad siblings', async () => {
    const { ATMOSPHERE_SOURCE_COLORS } = await import('$lib/atmosphere');
    expect(ATMOSPHERE_SOURCE_COLORS['soul']).toBe('#f0c040');
    expect(ATMOSPHERE_SOURCE_COLORS['eva']).toBe('#FF6B9D');
    expect(ATMOSPHERE_SOURCE_COLORS['corso']).toBe('#00BFFF');
  });

  it('sourceColor resolves case-insensitively', async () => {
    const { sourceColor } = await import('$lib/atmosphere');
    expect(sourceColor('SOUL')).toBe('#f0c040');
    expect(sourceColor('EVA')).toBe('#FF6B9D');
    expect(sourceColor('unknown-source')).toBe('#64748b');
  });

  it('SEVERITY_COLORS covers all four severity levels', async () => {
    const { SEVERITY_COLORS } = await import('$lib/atmosphere');
    expect(SEVERITY_COLORS.info).toBe('#94a3b8');
    expect(SEVERITY_COLORS.ok).toBe('#22c55e');
    expect(SEVERITY_COLORS.warn).toBe('#f59e0b');
    expect(SEVERITY_COLORS.err).toBe('#ef4444');
  });

  it('scanLinesEnabled initialises to false', async () => {
    const { scanLinesEnabled } = await import('$lib/atmosphere');
    const { get } = await import('svelte/store');
    expect(get(scanLinesEnabled)).toBe(false);
  });
});

// ── EventStream.svelte ────────────────────────────────────────────────────

describe('EventStream', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/EventStream.svelte');
    expect(mod.default).toBeDefined();
  });

  it('logLevelToSeverity maps known log levels', async () => {
    const { logLevelToSeverity } = await import('$lib/../components/EventStream.svelte');
    expect(logLevelToSeverity('error')).toBe('err');
    expect(logLevelToSeverity('warn')).toBe('warn');
    expect(logLevelToSeverity('success')).toBe('ok');
    expect(logLevelToSeverity('info')).toBe('info');
    expect(logLevelToSeverity('debug')).toBe('info');
    expect(logLevelToSeverity('unknown')).toBe('info');
  });
});

// ── ScanLines.svelte ──────────────────────────────────────────────────────

describe('ScanLines', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/atmosphere/ScanLines.svelte');
    expect(mod.default).toBeDefined();
  });
});

// ── GlobalEventsOverlay.svelte ────────────────────────────────────────────

describe('GlobalEventsOverlay', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/GlobalEventsOverlay.svelte');
    expect(mod.default).toBeDefined();
  });
});

// ── LogStream.svelte (refactored) ─────────────────────────────────────────

describe('LogStream (EventStream delegate)', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/LogStream.svelte');
    expect(mod.default).toBeDefined();
  });
});
