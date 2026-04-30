// ============================================================================
// tilt3d.ts — Svelte action for 3D perspective card tilt on mouse hover
// Ported from roadmap-content.html 3D card effect
// ============================================================================

interface Tilt3dOptions {
  intensity?: number;
}

export function tilt3d(node: HTMLElement, opts?: Tilt3dOptions) {
  const intensity = opts?.intensity ?? 4;

  function onMove(e: MouseEvent) {
    const rect = node.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const rotateX = ((y - rect.height / 2) / (rect.height / 2)) * -intensity;
    const rotateY = ((x - rect.width / 2) / (rect.width / 2)) * intensity;
    node.style.transform = `perspective(800px) rotateX(${rotateX}deg) rotateY(${rotateY}deg) scale(1.01)`;
  }

  function onLeave() {
    node.style.transform = 'perspective(800px) rotateX(0) rotateY(0) scale(1)';
    node.style.transition = 'transform 0.4s ease';
  }

  function onEnter() {
    node.style.transition = 'none';
  }

  node.addEventListener('mouseenter', onEnter);
  node.addEventListener('mousemove', onMove);
  node.addEventListener('mouseleave', onLeave);

  return {
    destroy() {
      node.removeEventListener('mouseenter', onEnter);
      node.removeEventListener('mousemove', onMove);
      node.removeEventListener('mouseleave', onLeave);
    },
  };
}
