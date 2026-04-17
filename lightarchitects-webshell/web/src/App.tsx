/**
 * App — root component.
 *
 * Phase 8 layout: horizontal split-pane — terminal left, 3D helix right.
 * The resize handle is draggable; panels have a 15% minimum width each.
 *
 * Phase 6 note: the SSE connection (useEventSource) is wired here, not inside
 * HelixScene, so it stays alive even if the helix panel is unmounted.
 */
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { useEventSource } from './hooks/useEventSource';
import { TerminalPane } from './components/Terminal/TerminalPane';
import { HelixScene } from './three/HelixScene';

export default function App() {
  // SSE connection — dispatches into Zustand, HelixScene reads from store.
  useEventSource();

  return (
    <PanelGroup
      direction="horizontal"
      style={{ height: '100vh', width: '100vw' }}
    >
      {/* Left: PTY terminal */}
      <Panel defaultSize={50} minSize={15} id="terminal">
        <TerminalPane />
      </Panel>

      {/* Drag handle */}
      <PanelResizeHandle
        style={{
          width: 4,
          background: '#1e293b',
          cursor: 'col-resize',
          flexShrink: 0,
          transition: 'background 150ms',
        }}
        onDragging={(isDragging) => {
          // Briefly highlight while dragging for visual feedback.
          const el = document.activeElement as HTMLElement | null;
          el?.blur();
          void isDragging; // acknowledged — styling via CSS :active would need a class
        }}
      />

      {/* Right: 3D helix scene */}
      <Panel defaultSize={50} minSize={15} id="helix">
        <HelixScene />
      </Panel>
    </PanelGroup>
  );
}
