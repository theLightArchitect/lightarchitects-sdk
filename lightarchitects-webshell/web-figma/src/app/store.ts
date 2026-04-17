import { create } from 'zustand';

export interface Step {
  id: string;
  y: number;
  railIdx: number;
  color: string;
  createdAt: number;
  offsetX: number;
  offsetY: number;
  offsetZ: number;
}

export interface Orb {
  id: string;
  railIdx: number;
  startY: number;
  createdAt: number;
}

export interface SceneState {
  steps: Step[];
  orbs: Orb[];
  ayinStatus: 'connected' | 'reconnecting' | 'offline';
  addStep: (step: Omit<Step, 'createdAt' | 'offsetX' | 'offsetY' | 'offsetZ'>) => void;
  spawnOrb: () => void;
  setAyinStatus: (status: 'connected' | 'reconnecting' | 'offline') => void;
  tick: () => void;
}

export const useSceneStore = create<SceneState>((set) => ({
  steps: [],
  orbs: [],
  ayinStatus: 'reconnecting',
  addStep: (step) => set((state) => {
    const newStep: Step = {
      ...step,
      createdAt: Date.now(),
      offsetX: (Math.random() - 0.5) * 0.4,
      offsetY: (Math.random() - 0.5) * 0.4,
      offsetZ: (Math.random() - 0.5) * 0.4,
    };
    const newSteps = [...state.steps, newStep];
    return { steps: newSteps.slice(-5000) };
  }),
  spawnOrb: () => set((state) => {
    const railIdx = Math.random() > 0.5 ? 0 : 1;
    const orb: Orb = {
      id: Math.random().toString(36).substring(7),
      railIdx,
      startY: 15,
      createdAt: Date.now()
    };
    const newOrbs = [...state.orbs, orb];
    return { orbs: newOrbs.slice(-5) };
  }),
  setAyinStatus: (status) => set({ ayinStatus: status }),
  tick: () => set((state) => {
    const now = Date.now();
    // Orb lifetime = 6000ms
    const aliveOrbs = state.orbs.filter(o => now - o.createdAt < 6000);
    if (aliveOrbs.length !== state.orbs.length) {
      return { orbs: aliveOrbs };
    }
    return {};
  })
}));
