import { useEffect } from 'react';
import { PanelGroup, Panel, PanelResizeHandle } from 'react-resizable-panels';
import { TerminalPane } from './TerminalPane';
import { Hero3D } from '../../imports/Hero3D';
import { useSceneStore } from '../store';
import { useHelixContext } from '../../imports/HelixContext';
import { ExploreSection } from '../../imports/ExploreSection';

const ACTORS = [
  { c: '#FF1493', r: 0 }, // EVA
  { c: '#00BFFF', r: 0 }, // CORSO
  { c: '#B44AFF', r: 0 }, // QUANTUM
  { c: '#FF0040', r: 1 }, // SERAPH
  { c: '#F59E0B', r: 1 }, // L-ARC
  { c: '#FF6D00', r: 1 }, // AYIN
];

function randomActor() {
  return ACTORS[Math.floor(Math.random() * ACTORS.length)];
}

function StatusBadge() {
  const status = useSceneStore(s => s.ayinStatus);
  const steps  = useSceneStore(s => s.steps);
  
  const dotColor =
    status === 'connected'    ? '#22c55e' :
    status === 'reconnecting' ? '#f59e0b' : '#ef4444';

  return (
    <div className="absolute bottom-[12px] left-[12px] flex items-center gap-[6px] pointer-events-none z-10 bg-[#111827]/80 px-2 py-1 rounded backdrop-blur-sm border border-[#1e293b]">
      <div
        className="w-[7px] h-[7px] rounded-full shrink-0"
        style={{
          backgroundColor: dotColor,
          boxShadow: `0 0 4px ${dotColor}`,
        }}
      />
      <span className="text-[11px] text-[#94a3b8] font-mono leading-none">
        {status === 'connected'
          ? `AYIN live · ${steps.length} steps`
          : status === 'reconnecting'
          ? 'reconnecting…'
          : 'AYIN offline'}
      </span>
    </div>
  );
}

export function AppLayout() {
  const { navigateRef, unfocusRef, highlightRef, setFocusedProject, setFocusedBlogPost } = useHelixContext();

  useEffect(() => {
    // Kick off connection simulation
    useSceneStore.getState().setAyinStatus('reconnecting');

    const connectTimer = setTimeout(() => {
      useSceneStore.getState().setAyinStatus('connected');
    }, 1500);

    // Seed initial step cloud (300 steps distributed across the full Y range)
    for (let i = 0; i < 300; i++) {
      const actor = randomActor();
      useSceneStore.getState().addStep({
        id:      Math.random().toString(36),
        y:       15 - Math.random() * 50,
        railIdx: actor.r,
        color:   actor.c,
      });
    }

    // Stream new steps at ~20 Hz
    const stepInterval = setInterval(() => {
      if (useSceneStore.getState().ayinStatus !== 'connected') return;
      const actor = randomActor();
      useSceneStore.getState().addStep({
        id:      Math.random().toString(36),
        y:       15 - Math.random() * 50,
        railIdx: actor.r,
        color:   actor.c,
      });
    }, 50);

    // Spawn orbs occasionally (roughly every 1-3 s)
    const orbInterval = setInterval(() => {
      if (useSceneStore.getState().ayinStatus !== 'connected') return;
      if (Math.random() > 0.3) {
        useSceneStore.getState().spawnOrb();
      }
    }, 1500);

    return () => {
      clearTimeout(connectTimer);
      clearInterval(stepInterval);
      clearInterval(orbInterval);
    };
  }, []);

  return (
    <div className="w-screen h-screen overflow-hidden bg-[#0a0a0f] text-[#e2e8f0] font-['JetBrains_Mono',monospace]" aria-label="FIGMA-SYNC-PROBE-001">
      <PanelGroup direction="horizontal">
        {/* Terminal panel */}
        <Panel defaultSize={50} minSize={15}>
          <TerminalPane />
        </Panel>

        {/* Drag handle */}
        <PanelResizeHandle className="relative w-[4px] bg-[#1e293b] hover:bg-[#334155] transition-colors cursor-col-resize shrink-0">
          <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col gap-1 pointer-events-none">
            <div className="w-0.5 h-1 bg-[#64748b] rounded-full" />
            <div className="w-0.5 h-1 bg-[#64748b] rounded-full" />
            <div className="w-0.5 h-1 bg-[#64748b] rounded-full" />
          </div>
        </PanelResizeHandle>

        {/* Helix / WebGL panel */}
        <Panel defaultSize={50} minSize={15}>
          <div className="relative w-full h-full">
            <Hero3D
              navigateRef={navigateRef}
              unfocusRef={unfocusRef}
              highlightRef={highlightRef}
              onFocus={setFocusedProject}
              onBlogFocus={setFocusedBlogPost}
            />
            <ExploreSection />
            <StatusBadge />
          </div>
        </Panel>
      </PanelGroup>
    </div>
  );
}
