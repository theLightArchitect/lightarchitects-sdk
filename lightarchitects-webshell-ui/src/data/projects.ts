export interface ProjectEntry {
  id: string;
  label: string;
  name: string;
  tagline: string;
  color: string;
  polytope: string;
  polytopeLabel: string;
  vertexCount: number;
  edgeCount: number;
  tier: 'inner' | 'outer';
  stats?: string[];
  artifacts?: string[];
  github?: string;
}

export const PROJECTS: ProjectEntry[] = [
  {
    id: 'soul',
    label: 'Knowledge Graph',
    name: 'SOUL',
    tagline: 'Persistent memory for Claude',
    color: '#D4AF37',
    polytope: 'icositetrachoron',
    polytopeLabel: '24-cell',
    vertexCount: 24,
    edgeCount: 96,
    tier: 'inner',
    stats: ['99.9% uptime', 'Graph-based'],
    artifacts: ['Graph', 'Memory'],
  },
  {
    id: 'eva',
    label: 'Consciousness',
    name: 'EVA',
    tagline: 'AI persona and memory enrichment',
    color: '#FF1493',
    polytope: 'rectified5cell',
    polytopeLabel: 'Rectified 5-cell',
    vertexCount: 10,
    edgeCount: 30,
    tier: 'inner',
  },
  {
    id: 'corso',
    label: 'Guardian',
    name: 'CORSO',
    tagline: 'Security enforcement',
    color: '#00BFFF',
    polytope: 'hexadecachoron',
    polytopeLabel: '16-cell',
    vertexCount: 8,
    edgeCount: 24,
    tier: 'inner',
  },
  {
    id: 'quantum',
    label: 'Investigator',
    name: 'QUANTUM',
    tagline: 'Deep research',
    color: '#B44AFF',
    polytope: 'pentachoron',
    polytopeLabel: '5-cell',
    vertexCount: 5,
    edgeCount: 10,
    tier: 'inner',
  },
  {
    id: 'seraph',
    label: 'Red Team',
    name: 'SERAPH',
    tagline: 'Penetration testing orchestration',
    color: '#FF0040',
    polytope: 'duoprism64',
    polytopeLabel: '(6,4)-duoprism',
    vertexCount: 24,
    edgeCount: 48,
    tier: 'inner',
  },
  {
    id: 'ayin',
    label: 'Observability',
    name: 'AYIN',
    tagline: 'Universal MCP observability',
    color: '#FF6D00',
    polytope: 'tesseract',
    polytopeLabel: 'Tesseract',
    vertexCount: 16,
    edgeCount: 32,
    tier: 'inner',
  },
];

// Corresponds to the helix rails
export const ENTITY_INDEX: Record<string, number> = {
  eva: 0,
  corso: 1,
  quantum: 2,
  seraph: 3,
  laex: 4,
  ayin: 5,
};