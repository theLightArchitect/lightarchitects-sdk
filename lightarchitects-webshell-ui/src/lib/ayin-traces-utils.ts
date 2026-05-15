export interface TraceSpan {
  trace_id: string;
  span_id: string;
  actor: string;
  action: string;
  timestamp: string;
  duration_ms: number;
  outcome: 'Continue' | 'Finish' | string;
  parent_span_id?: string | null;
  tool?: string | null;
  model?: string | null;
  token_count?: number | null;
  tags?: Record<string, string> | null;
}

export function sanitize(s: string): string {
  return s.replace(/[^a-zA-Z0-9_.\-:/ ]/g, '_').slice(0, 40);
}

export function coerceDuration(raw: unknown): number {
  return Math.trunc(Number(raw) || 0);
}

export function buildSequenceDiagram(spans: TraceSpan[]): string {
  const actors = [...new Set(spans.map(s => s.actor))];
  const lines: string[] = ['sequenceDiagram'];
  for (const a of actors) lines.push(`  participant ${sanitize(a)}`);
  for (const span of spans) {
    const actor = sanitize(span.actor);
    const action = sanitize(span.action);
    const dur = coerceDuration(span.duration_ms);
    const durationLabel = dur > 0 ? ` (${dur}ms)` : '';
    const tool = span.tool ? ` [${sanitize(span.tool)}]` : '';
    lines.push(`  Note over ${actor}: ${action}${tool}${durationLabel}`);
    if (span.outcome === 'Finish') {
      lines.push(`  ${actor}-->>+${actor}: ✓ finish`);
    }
  }
  return lines.join('\n');
}

export function buildFlowDiagram(spans: TraceSpan[]): string {
  const lines: string[] = ['graph LR'];
  const nodeIds = new Map<string, string>();
  let nodeIdx = 0;
  function nodeId(actor: string, action: string): string {
    const key = `${actor}::${action}`;
    if (!nodeIds.has(key)) nodeIds.set(key, `N${nodeIdx++}`);
    return nodeIds.get(key)!;
  }
  for (let i = 0; i < spans.length - 1; i++) {
    const a = spans[i];
    const b = spans[i + 1];
    const idA = nodeId(a.actor, a.action);
    const idB = nodeId(b.actor, b.action);
    const labelA = `${sanitize(a.actor)}.${sanitize(a.action)}`;
    const labelB = `${sanitize(b.actor)}.${sanitize(b.action)}`;
    if (a.outcome === 'Finish') {
      const dur = coerceDuration(a.duration_ms);
      lines.push(`  ${idA}["${labelA}"] -->|${dur}ms| ${idB}["${labelB}"]`);
    } else {
      lines.push(`  ${idA}["${labelA}"] -.->|→| ${idB}["${labelB}"]`);
    }
  }
  if (spans.length === 1) {
    const s = spans[0];
    const id = nodeId(s.actor, s.action);
    lines.push(`  ${id}["${sanitize(s.actor)}.${sanitize(s.action)}"]`);
  }
  return lines.join('\n');
}
