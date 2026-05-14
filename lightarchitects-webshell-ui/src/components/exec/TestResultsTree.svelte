<script lang="ts">
  // Parses cargo test, nextest JSON, vitest JSON, and playwright JSON reporter output
  // from an exec output buffer. Displays a collapsible pass/fail tree.

  interface TestResult {
    name: string;
    status: 'pass' | 'fail' | 'skip' | 'pending';
    duration?: number;
    message?: string;
    children?: TestResult[];
  }

  interface Props {
    lines: string[];
    format?: 'cargo' | 'nextest' | 'vitest' | 'playwright' | 'auto';
  }

  let { lines, format = 'auto' }: Props = $props();

  let results: TestResult[] = $derived(parseLines(lines, format));
  let totalPass = $derived(countByStatus(results, 'pass'));
  let totalFail = $derived(countByStatus(results, 'fail'));
  let totalSkip = $derived(countByStatus(results, 'skip'));
  let expanded = $state<Set<string>>(new Set());

  function toggle(key: string) {
    const next = new Set(expanded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expanded = next;
  }

  function countByStatus(nodes: TestResult[], status: string): number {
    let n = 0;
    for (const r of nodes) {
      if (r.status === status) n++;
      if (r.children) n += countByStatus(r.children, status);
    }
    return n;
  }

  function parseLines(rawLines: string[], fmt: string): TestResult[] {
    const detected = fmt === 'auto' ? detect(rawLines) : fmt;
    if (detected === 'cargo') return parseCargo(rawLines);
    if (detected === 'nextest') return parseNextest(rawLines);
    if (detected === 'vitest') return parseVitest(rawLines);
    if (detected === 'playwright') return parsePlaywright(rawLines);
    return parseCargo(rawLines);
  }

  function detect(lines: string[]): string {
    for (const l of lines) {
      try {
        const o = JSON.parse(l);
        if (o.type === 'suite' || o.type === 'test') return 'nextest';
        if (o.numPassedTests !== undefined) return 'vitest';
        if (Array.isArray(o?.suites)) return 'playwright';
      } catch {
        // not JSON
      }
      if (l.includes('test result:') || /^test .+ \.\.\. (ok|FAILED|ignored)$/.test(l.trim())) {
        return 'cargo';
      }
    }
    return 'cargo';
  }

  // --- cargo test (stable libtest output) ---
  function parseCargo(lines: string[]): TestResult[] {
    const results: TestResult[] = [];
    for (const line of lines) {
      const m = line.match(/^test (.+?) \.\.\. (ok|FAILED|ignored)$/);
      if (m) {
        results.push({
          name: m[1],
          status: m[2] === 'ok' ? 'pass' : m[2] === 'ignored' ? 'skip' : 'fail',
        });
      }
    }
    return results;
  }

  // --- nextest libtest-JSON ---
  function parseNextest(lines: string[]): TestResult[] {
    const results: TestResult[] = [];
    for (const line of lines) {
      try {
        const o = JSON.parse(line);
        if (o.type === 'test' && o.event === 'ok') {
          results.push({ name: o.name, status: 'pass', duration: o.exec_time });
        } else if (o.type === 'test' && o.event === 'failed') {
          results.push({ name: o.name, status: 'fail', message: o.stdout ?? '' });
        } else if (o.type === 'test' && o.event === 'ignored') {
          results.push({ name: o.name, status: 'skip' });
        }
      } catch {
        // non-JSON line
      }
    }
    return results;
  }

  // --- vitest JSON reporter ---
  function parseVitest(lines: string[]): TestResult[] {
    for (const line of lines) {
      try {
        const o = JSON.parse(line);
        if (o.testResults) {
          return o.testResults.flatMap((suite: Record<string, unknown>) => {
            const children = ((suite.assertionResults ?? []) as Array<Record<string, unknown>>).map((t) => ({
              name: String(t.fullName ?? t.title ?? ''),
              status: t.status === 'passed' ? ('pass' as const) : t.status === 'pending' ? ('skip' as const) : ('fail' as const),
              duration: typeof t.duration === 'number' ? t.duration : undefined,
              message: Array.isArray(t.failureMessages) ? (t.failureMessages as string[]).join('\n') : undefined,
            }));
            return [{
              name: String(suite.testFilePath ?? ''),
              status: (suite.status === 'passed' ? 'pass' : 'fail') as 'pass' | 'fail',
              children,
            }];
          });
        }
      } catch {
        // not vitest JSON
      }
    }
    return [];
  }

  // --- Playwright JSON reporter ---
  function parsePlaywright(lines: string[]): TestResult[] {
    for (const line of lines) {
      try {
        const o = JSON.parse(line);
        if (!Array.isArray(o?.suites)) continue;
        return walkSuites(o.suites as Array<Record<string, unknown>>);
      } catch {
        // not playwright JSON
      }
    }
    return [];
  }

  function walkSuites(suites: Array<Record<string, unknown>>): TestResult[] {
    const out: TestResult[] = [];
    for (const suite of suites) {
      const children = [
        ...walkSuites((suite.suites ?? []) as Array<Record<string, unknown>>),
        ...((suite.specs ?? []) as Array<Record<string, unknown>>).map((spec) => {
          const result = ((spec.tests ?? []) as Array<Record<string, unknown>>)[0];
          const outcome = ((result?.results ?? []) as Array<Record<string, unknown>>)[0];
          const status = outcome?.status === 'passed' ? 'pass' : outcome?.status === 'skipped' ? 'skip' : 'fail';
          return {
            name: String(spec.title ?? ''),
            status: status as 'pass' | 'fail' | 'skip',
            duration: typeof outcome?.duration === 'number' ? outcome.duration : undefined,
            message: typeof outcome?.error === 'object' && outcome?.error !== null ? String((outcome.error as Record<string, unknown>).message ?? '') : undefined,
          };
        }),
      ];
      out.push({
        name: String(suite.title ?? suite.file ?? ''),
        status: children.some((c) => c.status === 'fail') ? 'fail' : 'pass',
        children,
      });
    }
    return out;
  }

  function statusIcon(s: string) {
    if (s === 'pass') return '✓';
    if (s === 'fail') return '✗';
    if (s === 'skip') return '—';
    return '?';
  }
</script>

<div class="test-tree">
  <div class="tree-header">
    <span class="tree-label">TEST RESULTS</span>
    <span class="count pass">{totalPass} passed</span>
    {#if totalFail > 0}<span class="count fail">{totalFail} failed</span>{/if}
    {#if totalSkip > 0}<span class="count skip">{totalSkip} skipped</span>{/if}
  </div>

  <div class="tree-body">
    {#if results.length === 0}
      <p class="empty">No test results detected yet.</p>
    {:else}
      {#each results as node, i}
        {@const key = `${i}:${node.name}`}
        <div class="node" class:fail={node.status === 'fail'}>
          {#if node.children?.length}
            <button class="node-row expandable" onclick={() => toggle(key)}>
              <span class="expand-icon">{expanded.has(key) ? '▾' : '▸'}</span>
              <span class="status-icon" class:ok={node.status === 'pass'} class:bad={node.status === 'fail'}>{statusIcon(node.status)}</span>
              <span class="node-name">{node.name}</span>
            </button>
            {#if expanded.has(key)}
              <div class="children">
                {#each node.children as child, j}
                  <div class="node leaf" class:fail={child.status === 'fail'}>
                    <div class="node-row">
                      <span class="spacer"></span>
                      <span class="status-icon" class:ok={child.status === 'pass'} class:bad={child.status === 'fail'} class:dim={child.status === 'skip'}>{statusIcon(child.status)}</span>
                      <span class="node-name">{child.name}</span>
                      {#if child.duration !== undefined}
                        <span class="duration">{child.duration.toFixed(0)}ms</span>
                      {/if}
                    </div>
                    {#if child.message && expanded.has(`${key}:${j}`)}
                      <pre class="error-msg">{child.message}</pre>
                    {/if}
                    {#if child.message}
                      <button class="toggle-msg" onclick={() => toggle(`${key}:${j}`)}>
                        {expanded.has(`${key}:${j}`) ? 'Hide' : 'Show error'}
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          {:else}
            <div class="node-row">
              <span class="status-icon" class:ok={node.status === 'pass'} class:bad={node.status === 'fail'} class:dim={node.status === 'skip'}>{statusIcon(node.status)}</span>
              <span class="node-name">{node.name}</span>
              {#if node.duration !== undefined}
                <span class="duration">{node.duration.toFixed(0)}ms</span>
              {/if}
            </div>
            {#if node.message && expanded.has(key)}
              <pre class="error-msg">{node.message}</pre>
            {/if}
            {#if node.message}
              <button class="toggle-msg" onclick={() => toggle(key)}>
                {expanded.has(key) ? 'Hide' : 'Show error'}
              </button>
            {/if}
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .test-tree {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--la-drawer-border, #2a2a3a);
    border-radius: 6px;
    overflow: hidden;
    background: var(--la-bg-frame, #0d0d14);
  }

  .tree-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--la-bg-elev-1, #111118);
    border-bottom: 1px solid var(--la-drawer-border, #2a2a3a);
    flex-shrink: 0;
  }

  .tree-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--la-text-label, #8888aa);
    letter-spacing: 0.08em;
  }

  .count {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 3px;
  }

  .count.pass { background: #1a3a1a; color: #4ade80; }
  .count.fail { background: #3a1a1a; color: #f87171; }
  .count.skip { background: #2a2a1a; color: #fbbf24; }

  .tree-body {
    padding: 8px 0;
    overflow-y: auto;
    max-height: 400px;
  }

  .empty {
    font-size: 12px;
    color: var(--la-text-dim, #555570);
    padding: 12px 16px;
  }

  .node { padding: 1px 12px; }
  .node.fail { background: rgba(248, 113, 113, 0.04); }

  .node-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    font-size: 12px;
    background: none;
    border: none;
    width: 100%;
    text-align: left;
    cursor: default;
    color: var(--la-text-primary, #e2e8f0);
  }

  .expandable { cursor: pointer; }
  .expandable:hover { color: var(--la-accent, #a78bfa); }

  .expand-icon { width: 12px; color: var(--la-text-dim, #555570); }
  .spacer { width: 18px; flex-shrink: 0; }

  .status-icon { width: 14px; font-weight: 700; flex-shrink: 0; }
  .status-icon.ok  { color: #4ade80; }
  .status-icon.bad { color: #f87171; }
  .status-icon.dim { color: #555570; }

  .node-name { flex: 1; font-family: monospace; font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .duration  { font-size: 10px; color: var(--la-text-dim, #555570); flex-shrink: 0; }

  .children { padding-left: 20px; }

  .error-msg {
    margin: 4px 0 4px 20px;
    padding: 6px 8px;
    background: #1a0a0a;
    border-left: 2px solid #f87171;
    font-size: 10px;
    font-family: monospace;
    color: #f87171;
    white-space: pre-wrap;
    word-break: break-word;
    border-radius: 0 3px 3px 0;
  }

  .toggle-msg {
    margin-left: 20px;
    font-size: 10px;
    color: var(--la-text-dim, #555570);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }

  .toggle-msg:hover { color: var(--la-text-label, #8888aa); }
</style>
