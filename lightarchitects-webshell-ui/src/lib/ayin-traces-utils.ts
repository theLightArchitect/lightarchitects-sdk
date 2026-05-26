export interface TraceSpan {
  trace_id: string;
  span_id: string;
  actor: string;
  action: string;
  timestamp: string;
  duration_ms: number;
  outcome: 'Continue' | 'Finish' | string;
  parent_id?: string | null;
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

  const spanIds = new Set(spans.map(s => s.span_id));
  const childrenOf = new Map<string, TraceSpan[]>();
  const roots: TraceSpan[] = [];
  for (const span of spans) {
    if (span.parent_id && spanIds.has(span.parent_id)) {
      const list = childrenOf.get(span.parent_id) ?? [];
      list.push(span);
      childrenOf.set(span.parent_id, list);
    } else {
      roots.push(span);
    }
  }

  function emitSpan(span: TraceSpan): void {
    const actor = sanitize(span.actor);
    const action = sanitize(span.action);
    const dur = coerceDuration(span.duration_ms);
    const durationLabel = dur > 0 ? ` (${dur}ms)` : '';
    const tool = span.tool ? ` [${sanitize(span.tool)}]` : '';
    const children = childrenOf.get(span.span_id) ?? [];
    lines.push(`  Note over ${actor}: ${action}${tool}${durationLabel}`);
    if (children.length > 0) {
      lines.push(`  activate ${actor}`);
      for (const child of children) emitSpan(child);
      lines.push(`  deactivate ${actor}`);
    }
    if (span.outcome === 'Finish') {
      lines.push(`  ${actor}-->>+${actor}: ✓ finish`);
    }
  }

  for (const root of roots) emitSpan(root);
  return lines.join('\n');
}

// Strip event-handler identifiers (onerror, onclick, …) and dangerous JS
// function names that survive the base sanitize step because they are
// alphanumeric.  Applied only to Mermaid node labels where innerHTML rendering
// may not escape nested strings.
function mermaidLabel(actor: string, action: string): string {
  const raw = `${sanitize(actor)}.${sanitize(action)}`;
  return raw
    .replace(/\bon\w+\b/gi, '_evt_')
    .replace(/\b(?:alert|eval|exec|fetch|document|window|location)\b/gi, '_fn_')
    .replace(/[^a-zA-Z0-9_.\-:/ ]/g, '_')
    .slice(0, 40);
}

export function buildFlowDiagram(spans: TraceSpan[]): string {
  const lines: string[] = ['graph LR'];
  const nodeIds = new Map<string, string>();
  let nodeIdx = 0;
  function nodeId(spanId: string): string {
    if (!nodeIds.has(spanId)) nodeIds.set(spanId, `N${nodeIdx++}`);
    return nodeIds.get(spanId)!;
  }
  const knownSpanIds = new Set(spans.map(s => s.span_id));

  // Pass 1: define all nodes
  for (const span of spans) {
    const nid = nodeId(span.span_id);
    const label = mermaidLabel(span.actor, span.action);
    const isRoot = !span.parent_id || !knownSpanIds.has(span.parent_id);
    lines.push(`  ${nid}${isRoot ? `(["${label}"])` : `["${label}"]`}`);
  }

  // Pass 2: edges via parent_id
  for (const span of spans) {
    if (span.parent_id && knownSpanIds.has(span.parent_id)) {
      const pnid = nodeId(span.parent_id);
      const nid = nodeId(span.span_id);
      const dur = coerceDuration(span.duration_ms);
      if (span.outcome === 'Finish') {
        lines.push(`  ${pnid} -->|${dur}ms| ${nid}`);
      } else {
        lines.push(`  ${pnid} -.->|→| ${nid}`);
      }
    }
  }
  return lines.join('\n');
}
