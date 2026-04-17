import type { Polytope4DType } from '../imports/polytopes4d';

export interface ProjectEntry {
  id: string;
  label: string;
  name: string;
  tagline: string;
  color: string;
  polytope: Polytope4DType;
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
    polytope: 'doubleHelix4D',
    polytopeLabel: 'Double Helix',
    vertexCount: 16,
    edgeCount: 24,
    tier: 'inner',
    stats: ['99.9% uptime', 'Graph-based'],
    artifacts: ['Graph', 'Memory'],
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
    polytope: 'tesseract',
    polytopeLabel: 'Tesseract',
    vertexCount: 16,
    edgeCount: 32,
    tier: 'inner',
  }
];

// Corresponds to the rails
export const ENTITY_INDEX: Record<string, number> = {
  eva: 0,
  corso: 1,
  quantum: 2,
  seraph: 3,
  larc: 4,
  ayin: 5,
};
