/**
 * Tutorial framework — Shepherd.js wrapper for Light Architects (#27).
 *
 * Tutorials are versioned by id (T1, T2, ...) and tracked per-user in
 * localStorage so they only fire on first visit. URL params (`?onboarding=t1`)
 * force re-trigger for explicit re-runs.
 *
 * Convention: each TutorialId has a tutorialFor<X>() factory that builds the
 * Tour for that flow. Step targets reference DOM elements via
 * selectors, so the screen markup must include `data-onboarding="..."` attrs
 * on the elements we attach to (avoids brittle CSS-class coupling).
 */
import Shepherd from 'shepherd.js';
import type { Tour } from 'shepherd.js';
import 'shepherd.js/dist/css/shepherd.css';

/** All tutorials available in the app. T1 ships in #27; T2-T6 follow. */
export type TutorialId = 't1' | 't2' | 't3' | 't4' | 't5' | 't6';

const STORAGE_PREFIX = 'la.tutorial.completed.';

/** Has this tutorial been completed (or dismissed) by this user? */
export function hasCompleted(id: TutorialId): boolean {
  if (typeof localStorage === 'undefined') return false;
  try {
    return localStorage.getItem(STORAGE_PREFIX + id) === 'true';
  } catch {
    return false;
  }
}

/** Mark a tutorial as completed so it doesn't auto-fire again. */
function markCompleted(id: TutorialId): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(STORAGE_PREFIX + id, 'true');
  } catch {
    /* localStorage write failed — tutorial may re-fire next session, acceptable */
  }
}

/**
 * Read a `?onboarding=<id>` URL param and clear it after consumption so
 * page-reloads don't re-trigger. Used by screens to detect explicit
 * "show me this tutorial again" requests.
 */
export function consumeOnboardingParam(): TutorialId | null {
  if (typeof window === 'undefined') return null;
  const url = new URL(window.location.href);
  const id = url.searchParams.get('onboarding');
  if (!id) return null;
  url.searchParams.delete('onboarding');
  window.history.replaceState({}, '', url.toString());
  const valid: TutorialId[] = ['t1', 't2', 't3', 't4', 't5', 't6'];
  return valid.includes(id as TutorialId) ? (id as TutorialId) : null;
}

/**
 * Run a tutorial if it hasn't been completed (or always, if `force`).
 * Returns the Tour instance so callers can listen to lifecycle
 * events; the framework auto-marks completion on tour finish/cancel.
 */
export function runTutorial(id: TutorialId, force = false): Tour | null {
  if (!force && hasCompleted(id)) return null;
  const tour = buildTour(id);
  if (!tour) return null;
  tour.on('complete', () => markCompleted(id));
  tour.on('cancel', () => markCompleted(id));
  tour.start();
  return tour;
}

function buildTour(id: TutorialId): Tour | null {
  switch (id) {
    case 't1': return tutorialT1FirstBuild();
    case 't2': return tutorialT2MemoryDrawer();
    // T3-T6 land in follow-up tasks (#30, #31)
    default: return null;
  }
}

function tutorialT1FirstBuild(): Tour {
  const tour = new Shepherd.Tour({
    useModalOverlay: true,
    defaultStepOptions: {
      classes: 'la-shepherd',
      scrollTo: { behavior: 'smooth', block: 'center' },
      cancelIcon: { enabled: true },
    },
  });

  tour.addStep({
    id: 't1-welcome',
    title: 'First Build',
    text: `<p>Welcome. This walkthrough takes ~30 seconds — it'll show you how to start your first build.</p>
           <p>Press <kbd>Esc</kbd> any time to dismiss.</p>`,
    buttons: [
      { text: 'Skip', action: () => tour.cancel(), classes: 'la-shepherd-secondary' },
      { text: 'Start', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't1-source',
    title: 'Pick a source',
    text: 'Where does this build start? Manual is the most flexible — you describe it. GitHub imports from an issue or PR. Cargo Audit creates one from `cargo audit` findings.',
    attachTo: { element: '[data-onboarding="intake-source"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't1-meta-skill',
    title: 'Meta-skill = which agent leads',
    text: '/BUILD lets the engineer drive. /SCRUM is squad review. /Q is investigation. The polytope icon under each card hints at the agent\'s role.',
    attachTo: { element: '[data-onboarding="intake-meta-skill"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't1-mode',
    title: 'Quick vs Plan',
    text: 'Quick Build kicks off immediately with sensible defaults. Plan Builder lets you preview and edit phases, gates, and criteria before launching.',
    attachTo: { element: '[data-onboarding="intake-mode-toggle"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't1-submit',
    title: 'Launch',
    text: 'Hit Create when ready. The squad picks it up, runs the phase pipeline, and you\'ll see the trace stream in Activity.',
    attachTo: { element: '[data-onboarding="intake-submit"]', on: 'top' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Got it', action: () => tour.complete(), classes: 'la-shepherd-primary' },
    ],
  });

  return tour;
}

function tutorialT2MemoryDrawer(): Tour {
  const tour = new Shepherd.Tour({
    useModalOverlay: true,
    defaultStepOptions: {
      classes: 'la-shepherd',
      scrollTo: { behavior: 'smooth', block: 'center' },
      cancelIcon: { enabled: true },
    },
  });

  tour.addStep({
    id: 't2-intro',
    title: 'Memory — Three Tiers',
    text: `<p>The Memory drawer shows what each squad agent remembers. There are three views: <strong>Hot</strong>, <strong>Cold</strong>, and <strong>Convergences</strong>.</p>
           <p>Press <kbd>Esc</kbd> any time to dismiss.</p>`,
    attachTo: { element: '[data-onboarding="memory-header"]', on: 'left' },
    buttons: [
      { text: 'Skip', action: () => tour.cancel(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't2-hot',
    title: 'Hot Memory (Turnlog)',
    text: `<p><strong>Hot</strong> is the active session context — what the agent is working through right now. Entries live here transiently and are promoted to Cold when they reach significance ≥7.0.</p>`,
    attachTo: { element: '[data-onboarding="memory-tab-hot"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't2-cold',
    title: 'Cold Memory (Helix)',
    text: `<p><strong>Cold</strong> is long-term memory — the SOUL helix vault. Each entry has a significance score (0–10) and is classified by type (entry, plan, standard, lesson…). Click any row to read the full markdown.</p>`,
    attachTo: { element: '[data-onboarding="memory-tab-cold"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't2-convergences',
    title: 'Convergences',
    text: `<p><strong>Convergences</strong> are cross-agent shared moments — events where two or more agents arrived at related conclusions. Detected nightly by the SOUL consolidator via Louvain community detection.</p>`,
    attachTo: { element: '[data-onboarding="memory-tab-convergences"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Next', action: () => tour.next(), classes: 'la-shepherd-primary' },
    ],
  });

  tour.addStep({
    id: 't2-search',
    title: 'Search the Vault',
    text: `<p>Search across all cold entries. Three modes: <strong>BM25</strong> (keyword), <strong>Semantic</strong> (embedding-based), <strong>Hybrid</strong> (RRF fusion of both). Look for the RRF badge — it means results were blended from multiple retrieval signals.</p>`,
    attachTo: { element: '[data-onboarding="memory-search"]', on: 'bottom' },
    buttons: [
      { text: 'Back', action: () => tour.back(), classes: 'la-shepherd-secondary' },
      { text: 'Got it', action: () => tour.complete(), classes: 'la-shepherd-primary' },
    ],
  });

  return tour;
}

