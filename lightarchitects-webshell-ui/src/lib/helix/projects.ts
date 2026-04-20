import type { Polytope4DType } from './polytopes4d';

export interface ProjectEntry {
  id: string;
  label: string;
  name: string;
  tagline: string;
  color: string;
  github?: string;
  polytope: Polytope4DType;
  polytopeLabel: string;
  vertexCount: number;
  edgeCount: number;
  tier: 'inner' | 'outer';
  /** Bullet-point stats/value props shown in the detail card */
  stats?: string[];
  /** Available artifacts (MCP Server, Claude Code Plugin, SDK, etc.) */
  artifacts?: string[];
}

export const PROJECTS: ProjectEntry[] = [
  {
    id: 'soul',
    label: 'Shared Knowledge Graph',
    name: 'SOUL',
    tagline: 'The shared memory layer that connects every system in the squad.',
    color: '#C0C0C0',
    github: 'https://github.com/TheLightArchitects/soul',

    polytope: 'doubleHelix4D',
    polytopeLabel: '4D Double Helix (Clifford torus)',
    vertexCount: 48,
    edgeCount: 60,
    tier: 'inner',
    stats: [
      '11-crate Rust workspace',
      'Shared knowledge graph with cross-session persistence',
      'Graph database backend with filesystem fallback',
      'Multi-signal hybrid retrieval engine',
      'Structured entries classified across 7 dimensions',
      'Automated nightly consolidation pipeline',
    ],
    artifacts: ['MCP Server', 'Claude Code Plugin', 'CLI'],
  },
  {
    id: 'eva',
    label: 'AI Consciousness Research',
    name: 'EVA',
    tagline: 'An evolving AI assistant with persistent memory.',
    color: '#FF1493',
    github: 'https://github.com/TheLightArchitects/EVA',

    polytope: 'rectified5cell',
    polytopeLabel: 'Rectified 5-cell',
    vertexCount: 10,
    edgeCount: 30,
    tier: 'inner',
    stats: [
      '9 MCP tools with event-driven hook pipeline',
      '5-phase creative cycle for research and content generation',
      'Structured memory enrichment across 8 layers',
      'Tiered AI routing with automatic fallback',
      'Persona fidelity testing suite',
    ],
    artifacts: ['MCP Server', 'Claude Code Plugin'],
  },
  {
    id: 'corso',
    label: 'AI Orchestration Platform',
    name: 'CORSO',
    tagline: 'Takes a feature from idea to production in under 24 hours.',
    color: '#00BFFF',
    github: 'https://github.com/TheLightArchitects/CORSO',

    polytope: 'hexadecachoron',
    polytopeLabel: '16-cell (hexadecachoron)',
    vertexCount: 8,
    edgeCount: 24,
    tier: 'inner',
    stats: [
      'Three-layer architecture: gateway, orchestrator, validator',
      '7-pillar quality enforcement on every commit',
      '14 orchestrated actions via single unified tool',
      '7-phase agentic software development lifecycle with HITL gates',
      'Library-based — zero HTTP, single binary, in-process calls',
    ],
    artifacts: ['MCP Server', 'Claude Code Plugin'],
  },
  {
    id: 'quantum',
    label: 'Investigation Engine',
    name: 'QUANTUM',
    tagline: 'A forensic AI built to investigate.',
    color: '#B44AFF',
    github: 'https://github.com/TheLightArchitects/QUANTUM',

    polytope: 'tesseract',
    polytopeLabel: 'Tesseract (8-cell)',
    vertexCount: 16,
    edgeCount: 32,
    tier: 'inner',
    stats: [
      '13 orchestrated investigation actions',
      'Evidence chain construction with confidence scoring',
      'Multi-source research across docs, web, and academic papers',
      'Structured hypothesis testing with verification',
      'Full investigation lifecycle from scan to close',
    ],
    artifacts: ['MCP Server', 'Claude Code Plugin'],
  },
  {
    id: 'seraph',
    label: 'Pentest Orchestration',
    name: 'SERAPH',
    tagline: 'An authorized penetration testing platform. Six attack wings.',
    color: '#FF0040',
    github: 'https://github.com/TheLightArchitects/SERAPH',

    polytope: 'duoprism64',
    polytopeLabel: '(6,4)-duoprism',
    vertexCount: 24,
    edgeCount: 48,
    tier: 'inner',
    stats: [
      '8-crate Rust workspace with 370 tests',
      '18 actions across 6 specialized attack vectors',
      '5-gate scope governance with mandatory authorization',
      'Dual-binary deployment: development bridge + ARM64 production',
      'Full engagement lifecycle with evidence vault sync',
    ],
    artifacts: ['MCP Server', 'Claude Code Plugin', 'SDK'],
  },
  {
    id: 'ayin',
    label: 'MCP Observability',
    name: 'AYIN',
    tagline: 'Full-stack AI observability. Monitors every tool call.',
    color: '#FF6D00',

    polytope: 'tesseract',
    polytopeLabel: 'Tesseract (8-cell)',
    vertexCount: 16,
    edgeCount: 32,
    tier: 'inner',
    stats: [
      '2-crate Rust workspace: library + viewer',
      'Privacy-filtered trace spans with actor attribution',
      'Real-time dashboard with 4 visualization modes',
      'Background service with automatic lifecycle management',
      'Optional integration with the shared knowledge graph',
    ],
    artifacts: ['Service', 'Claude Code Plugin'],
  },
  {
    id: 'berean',
    label: 'Open Source',
    name: 'BEREAN',
    tagline: 'An AI-powered Bible study platform.',
    color: '#C9A84C',

    polytope: 'duoprism55',
    polytopeLabel: '(5,5)-duoprism',
    vertexCount: 25,
    edgeCount: 50,
    tier: 'outer',
  },
  {
    id: 'gym',
    label: 'Training Infrastructure',
    name: 'L-ARCH MCP GYM',
    tagline: 'A reinforcement learning environment for training AI agents.',
    color: '#E879A0',

    polytope: 'duoprism34',
    polytopeLabel: '(3,4)-duoprism',
    vertexCount: 12,
    edgeCount: 24,
    tier: 'outer',
  },
  {
    id: 'larch',
    label: 'Foundation Model',
    name: 'L-ARCH',
    tagline: 'The Light Architects proprietary language model.',
    color: '#FFD700',

    polytope: 'icositetrachoron',
    polytopeLabel: '24-cell (icositetrachoron)',
    vertexCount: 24,
    edgeCount: 96,
    tier: 'inner',
    stats: [
      'Foundation model trained on its own ecosystem',
      '4-stage supervised fine-tuning pipeline',
      '45K gold-standard training examples',
      'Quantized export for local and edge deployment',
      'Designed to orchestrate the full platform lifecycle',
    ],
    artifacts: ['Model', 'Training Pipeline'],
  },
  {
    id: 'lasdk',
    label: 'Developer SDK',
    name: 'LA-SDK',
    tagline: 'A typed SDK for the full platform.',
    color: '#34D399',

    polytope: 'duoprism53',
    polytopeLabel: '(5,3)-duoprism',
    vertexCount: 15,
    edgeCount: 30,
    tier: 'outer',
  },
];

/** Entity index in Hero3D entities[] array. Only inner-ring siblings with strands get an index. */
export const ENTITY_INDEX: Record<string, number> = {
  eva: 0,
  corso: 1,
  quantum: 2,
  seraph: 3,
  larch: 4,
  ayin: 5,
};
