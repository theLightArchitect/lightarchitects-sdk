"use client";

import { createContext, useContext, useRef, useState, useCallback, type MutableRefObject } from 'react';
import type { ProjectEntry } from '../data/projects';
import type { BlogPost } from '../data/blog-posts';

interface HelixContextValue {
  navigateRef: MutableRefObject<((projectId: string) => void) | null>;
  unfocusRef: MutableRefObject<(() => void) | null>;
  highlightRef: MutableRefObject<((projectId: string) => void) | null>;
  focusedProject: ProjectEntry | null;
  setFocusedProject: (p: ProjectEntry | null) => void;
  focusedBlogPost: BlogPost | null;
  setFocusedBlogPost: (p: BlogPost | null) => void;
}

const HelixContext = createContext<HelixContextValue | null>(null);

export function HelixProvider({ children }: { children: React.ReactNode }) {
  const navigateRef = useRef<((projectId: string) => void) | null>(null);
  const unfocusRef = useRef<(() => void) | null>(null);
  const highlightRef = useRef<((projectId: string) => void) | null>(null);
  const [focusedProject, setFocusedProjectState] = useState<ProjectEntry | null>(null);
  const [focusedBlogPost, setFocusedBlogPostState] = useState<BlogPost | null>(null);

  const setFocusedProject = useCallback((p: ProjectEntry | null) => {
    setFocusedProjectState(p);
  }, []);

  const setFocusedBlogPost = useCallback((p: BlogPost | null) => {
    setFocusedBlogPostState(p);
  }, []);

  return (
    <HelixContext.Provider value={{ navigateRef, unfocusRef, highlightRef, focusedProject, setFocusedProject, focusedBlogPost, setFocusedBlogPost }}>
      {children}
    </HelixContext.Provider>
  );
}

export function useHelixContext() {
  const ctx = useContext(HelixContext);
  if (!ctx) throw new Error('useHelixContext must be inside HelixProvider');
  return ctx;
}

/** @deprecated Use useHelixContext() instead for full context access */
export function useHelixNavigate() {
  const ctx = useContext(HelixContext);
  if (!ctx) throw new Error('useHelixNavigate must be inside HelixProvider');
  return ctx.navigateRef;
}
