import type { Polytope4DType } from '../imports/polytopes4d';

export interface BlogPost {
  slug: string;
  title: string;
  excerpt: string;
  color: string;
  polytope: Polytope4DType;
  tags: string[];
  readTime: string;
  date: string;
}

export const BLOG_POSTS: BlogPost[] = [
  {
    slug: 'the-memory-graph',
    title: 'Building a Knowledge Graph for AI',
    excerpt: 'How we used graph DBs for persistent context.',
    color: '#D4AF37',
    polytope: 'tesseract',
    tags: ['Architecture', 'SOUL'],
    readTime: '5 min read',
    date: 'Apr 12, 2026',
  }
];
