// WHY: WCAG 2.1 SC 2.4.3 + ARIA APG Dialog pattern — Tab must not leave a
// modal dialog, and focus must return to the triggering element on close.

const FOCUSABLE_SELECTOR = [
  'button:not([disabled])',
  '[href]',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ');

/** Svelte action: trap keyboard focus within `node` and restore it on destroy. */
export function focusTrap(node: HTMLElement) {
  const trigger = document.activeElement as HTMLElement | null;

  function focusable(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    const els = focusable();
    if (els.length === 0) return;

    const first = els[0];
    const last = els[els.length - 1];

    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  node.addEventListener('keydown', handleKeydown);

  return {
    destroy() {
      node.removeEventListener('keydown', handleKeydown);
      trigger?.focus();
    },
  };
}
