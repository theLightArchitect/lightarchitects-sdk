"use client";

import { useCallback, useState, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'motion/react';
import { PolytopeIcon } from './PolytopeIcon';
import { useHelixContext } from './HelixContext';
import type { ProjectEntry } from '../data/projects';

export const ExploreSection = () => {
  const { unfocusRef, focusedProject, highlightRef } = useHelixContext();
  const [showIntro, setShowIntro] = useState(true);
  const sectionRef = useRef<HTMLDivElement>(null);
  const hasHighlighted = useRef(false);

  // Auto-highlight SOUL when section scrolls into view
  useEffect(() => {
    const el = sectionRef.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !hasHighlighted.current) {
          hasHighlighted.current = true;
          highlightRef.current?.('soul');
        }
      },
      { threshold: 0.3 },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [highlightRef]);

  const handleDismiss = useCallback(() => {
    unfocusRef.current?.();
  }, [unfocusRef]);

  return (
    <section ref={sectionRef} id="explore" className="absolute inset-0 pointer-events-none z-10 overflow-hidden">

      {/* Top-left intro card — dismissible, hides on project focus */}
      <AnimatePresence>
        {showIntro && !focusedProject && (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
            className="absolute top-1/2 -translate-y-1/2 left-[8%] md:left-[12%] z-10 pointer-events-auto"
          >
            <div
              className="bg-black/90 backdrop-blur-xl rounded-2xl p-6 md:p-8 max-w-[320px]"
              style={{
                border: '1px solid rgba(212, 175, 55, 0.3)',
                boxShadow: '0 0 30px rgba(212, 175, 55, 0.12), 0 0 60px rgba(212, 175, 55, 0.05), 0 12px 40px rgba(0, 0, 0, 0.7)',
              }}
            >
              {/* Close button */}
              <button
                onClick={() => setShowIntro(false)}
                className="absolute top-4 right-4 w-6 h-6 rounded-full border border-white/15 bg-white/[0.03] flex items-center justify-center text-white/40 hover:text-white hover:border-white/30 transition-all duration-200"
              >
                <svg width="8" height="8" viewBox="0 0 12 12" fill="none">
                  <path d="M9 3L3 9M3 3L9 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
                </svg>
              </button>

              <h2 className="text-xl md:text-2xl font-bold text-white leading-tight mb-3">
                The squad, in action.
              </h2>
              <p className="text-white/55 text-sm leading-relaxed mb-5">
                SOUL remembers. CORSO enforces. QUANTUM investigates. SERAPH hunts. EVA creates. AYIN traces.
                <br className="hidden sm:block" />
                Click any node to see how it works.
              </p>

              {/* CTA hint */}
              <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ delay: 1, duration: 0.8 }}
                className="flex items-center gap-3 bg-[#D4AF37]/[0.08] border border-[#D4AF37]/20 rounded-lg px-4 py-3"
              >
                <motion.div
                  animate={{ scale: [1, 1.3, 1], opacity: [0.6, 1, 0.6] }}
                  transition={{ duration: 2, repeat: Infinity, ease: 'easeInOut' }}
                  className="w-3 h-3 rounded-full bg-[#D4AF37] flex-shrink-0"
                />
                <span className="text-[#D4AF37]/80 text-sm font-medium">
                  Click a glowing node
                </span>
                <motion.span
                  animate={{ x: [0, 5, 0] }}
                  transition={{ duration: 1.5, repeat: Infinity, ease: 'easeInOut' }}
                  className="text-[#D4AF37]/50 text-base ml-auto"
                >
                  →
                </motion.span>
              </motion.div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Centered project card — appears on polytope click */}
      <div className="absolute inset-0 flex items-center justify-center">
        <AnimatePresence>
          {focusedProject && (
            <ProjectHeroCard
              key={focusedProject.id}
              project={focusedProject}
              onDismiss={handleDismiss}
            />
          )}
        </AnimatePresence>
      </div>
    </section>
  );
};

// ── Centered Project Hero Card ──────────────────────────────────────────────

interface ProjectHeroCardProps {
  project: ProjectEntry;
  onDismiss: () => void;
}

const ProjectHeroCard = ({ project, onDismiss }: ProjectHeroCardProps) => {
  const stats = project.stats ?? [];
  const artifacts = project.artifacts ?? [];

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95, y: 20 }}
      animate={{ opacity: 1, scale: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.95, y: 20 }}
      transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
      className="pointer-events-auto w-[90vw] max-w-xl"
    >
      <div
        className="rounded-xl overflow-hidden backdrop-blur-xl"
        style={{
          border: `1px solid ${project.color}25`,
          boxShadow: `0 16px 64px rgba(0,0,0,0.7), 0 0 40px ${project.color}10`,
        }}
      >
        {/* Top accent */}
        <div
          className="h-[2px] w-full"
          style={{ background: `linear-gradient(90deg, transparent, ${project.color}66, transparent)` }}
        />

        <div className="bg-black/85 p-6 md:p-8">
          {/* Header: polytope + name + close */}
          <div className="flex items-start gap-5 mb-5">
            <PolytopeIcon type={project.polytope} color={project.color} size={72} />
            <div className="flex-1 min-w-0">
              <p
                className="text-[9px] font-medium uppercase tracking-[0.3em] mb-1"
                style={{ color: project.color }}
              >
                {project.label}
              </p>
              <h3 className="text-white font-bold text-2xl tracking-wide">
                {project.name}
              </h3>
              <p className="text-white/45 text-sm leading-relaxed mt-1">
                {project.tagline}
              </p>
            </div>
            <button
              onClick={onDismiss}
              className="text-white/30 hover:text-white text-sm transition-colors duration-200 flex-shrink-0 mt-1"
            >
              ✕
            </button>
          </div>

          {/* Divider */}
          <div
            className="h-[1px] mb-5"
            style={{ background: `linear-gradient(90deg, transparent, ${project.color}20, transparent)` }}
          />

          {/* Stats bullets */}
          {stats.length > 0 && (
            <ul className="space-y-2 mb-5">
              {stats.map((stat) => (
                <li key={stat} className="flex items-start gap-2.5 text-sm text-white/60 leading-relaxed">
                  <span
                    className="w-1 h-1 rounded-full mt-2 flex-shrink-0"
                    style={{ background: project.color }}
                  />
                  {stat}
                </li>
              ))}
            </ul>
          )}

          {/* Divider */}
          <div
            className="h-[1px] mb-5"
            style={{ background: `linear-gradient(90deg, transparent, ${project.color}20, transparent)` }}
          />

          {/* Bottom row: artifacts + github */}
          <div className="flex items-center justify-between flex-wrap gap-3">
            {/* Artifact badges */}
            <div className="flex flex-wrap gap-1.5">
              {artifacts.map((artifact) => (
                <span
                  key={artifact}
                  className="px-2.5 py-1 rounded text-[10px] font-medium uppercase tracking-[0.1em] border"
                  style={{
                    color: `${project.color}cc`,
                    borderColor: `${project.color}25`,
                    background: `${project.color}08`,
                  }}
                >
                  {artifact}
                </span>
              ))}
            </div>

            {/* GitHub link */}
            {project.github && (
              <a
                href={project.github}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-xs uppercase tracking-[0.15em] font-medium transition-all duration-200 hover:text-white"
                style={{ color: `${project.color}cc` }}
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
                </svg>
                GitHub
              </a>
            )}
          </div>

          {/* Geometry footer */}
          <div className="mt-4 pt-3 border-t border-white/[0.04] flex items-center justify-between">
            <p className="text-white/20 text-[10px]">{project.polytopeLabel}</p>
            <p className="text-white/15 text-[10px]">{project.vertexCount}v · {project.edgeCount}e</p>
          </div>
        </div>
      </div>
    </motion.div>
  );
};
