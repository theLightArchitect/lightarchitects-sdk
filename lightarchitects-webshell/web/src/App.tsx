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
import { ErrorBoundary } from './components/ErrorBoundary';

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

      {/* Right: 3D helix scene (isolated error boundary — terminal stays live if 3D fails) */}
      <Panel defaultSize={50} minSize={15} id="helix">
        <ErrorBoundary
          fallback={(error, reset) => (
            <div style={{
              height: '100%',
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              background: '#0a0a0f',
              color: '#64748b',
              fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
              fontSize: '0.75rem',
              padding: '1rem',
              textAlign: 'center',
            }}>
              <div style={{ color: '#94a3b8', marginBottom: '0.5rem' }}>
                Helix unavailable
              </div>
              <div style={{ marginBottom: '0.75rem' }}>
                {error.message}
              </div>
              <button
                onClick={reset}
                style={{
                  background: '#1e293b',
                  border: '1px solid #334155',
                  color: '#94a3b8',
                  padding: '0.25rem 0.75rem',
                  borderRadius: '4px',
                  cursor: 'pointer',
                  fontSize: '0.7rem',
                  fontFamily: 'inherit',
                }}
              >
                Retry
              </button>
            </div>
          )}
        >
          <HelixScene />
        </ErrorBoundary>
      </Panel>
    </PanelGroup>
  );
}
