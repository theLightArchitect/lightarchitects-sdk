/**
 * Topology controls — shared reset/navigation helpers for 3D scenes.
 *
 * `resetTopologyView()` mirrors the prototype's "FIT ALL = match a fresh page
 * load" semantic: closes the events overlay, closes any open detail gutter,
 * then signals the active scene to home its camera. This ensures the canvas
 * framing after reset is identical to the initial page-load state.
 *
 * Components that own a 3D camera (VoxelProjects3D, Helix3D) listen for the
 * `la:topology-home-camera` CustomEvent on `window` and animate their camera
 * to the default position. The helper fires that event after closing overlays
 * so the canvas has its full dimensions before the camera targets recalculate.
 */

import { eventsOverlayOpen } from '$lib/stores';

/**
 * Closes the events overlay + any open detail gutter, then fires the
 * `la:topology-home-camera` event for active 3D scenes to handle.
 *
 * Wave 3 wires this to the VoxelProjects3D "FIT ALL" button and the `F` hotkey
 * on the /ops screen. Helix3D can also honour this event if needed.
 */
export function resetTopologyView(): void {
  eventsOverlayOpen.set(false);
  window.dispatchEvent(new CustomEvent('la:topology-close-gutter'));
  // Delay camera reset one frame so the overlay transition completes and the
  // canvas ResizeObserver fires before the camera targets are recalculated.
  requestAnimationFrame(() => {
    window.dispatchEvent(new CustomEvent('la:topology-home-camera'));
  });
}
