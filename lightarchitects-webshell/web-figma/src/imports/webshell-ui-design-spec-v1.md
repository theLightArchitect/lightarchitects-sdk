# Light Architects Webshell — UI Design Specification

> **Version**: 1.0  
> **Date**: 2026-04-16  
> **Purpose**: Figma handoff — full design tokens, component hierarchy, layout geometry, and Three.js scene composition.  
> **Status**: Extracted from production codebase (`lightarchitects-webshell/web/src/`)

---

## 1. Design Tokens

### 1.1 Color Palette

| Token | Hex | RGB | Usage |
|-------|-----|-----|-------|
| `color-bg` | `#0a0a0f` | (10, 10, 15) | Page background, terminal background, Canvas background |
| `color-surface` | `#111827` | (17, 24, 39) | Surface panels, error boundary buttons |
| `color-border` | `#1e293b` | (30, 41, 59) | Resize handle, borders |
| `color-text` | `#e2e8f0` | (226, 232, 240) | Primary text, terminal foreground |
| `color-muted` | `#94a3b8` | (148, 163, 184) | Secondary labels, status badge text |
| `color-dim` | `#64748b` | (100, 116, 139) | Tertiary text, error boundary stack traces |
| `color-accent` | `#00f5ff` | (0, 245, 255) | Cursor, orb spheres, hit rings, interactive highlights |
| `color-resize-handle` | `#1e293b` | (30, 41, 59) | Resize handle fill |
| `color-btn-border` | `#334155` | (51, 65, 85) | Button borders |

#### Actor Colors (Step Spheres & Helix Entities)

| Actor | Hex | RGB | Helix Rail |
|-------|-----|-----|------------|
| EVA | `#FF1493` | (255, 20, 147) | Rail 0 |
| CORSO | `#00BFFF` | (0, 191, 255) | Rail 0 |
| QUANTUM | `#B44AFF` | (180, 74, 255) | Rail 0 |
| SERAPH | `#FF0040` | (255, 0, 64) | Rail 1 |
| L-ARC | `#F59E0B` | (245, 158, 11) | Rail 1 |
| AYIN | `#FF6D00` | (255, 109, 0) | Rail 1 |
| Default | `#FFFFFF` | (255, 255, 255) | Rail 0 |

#### Terminal ANSI Colors

| Name | Hex | Usage |
|------|-----|-------|
| black | `#1e293b` | ANSI 0 |
| red | `#ef4444` | ANSI 1 |
| green | `#22c55e` | ANSI 2 |
| yellow | `#f59e0b` | ANSI 3 |
| blue | `#3b82f6` | ANSI 4 |
| magenta | `#FF1493` | ANSI 5 |
| cyan | `#00f5ff` | ANSI 6 |
| white | `#e2e8f0` | ANSI 7 |
| brightBlack | `#334155` | ANSI 8 |
| brightRed | `#fca5a5` | ANSI 9 |
| brightGreen | `#86efac` | ANSI 10 |
| brightYellow | `#fcd34d` | ANSI 11 |
| brightBlue | `#93c5fd` | ANSI 12 |
| brightMagenta | `#f9a8d4` | ANSI 13 |
| brightCyan | `#67e8f9` | ANSI 14 |
| brightWhite | `#f8fafc` | ANSI 15 |
| cursor | `#00f5ff` | Terminal cursor |
| cursorAccent | `#0a0a0f` | Terminal cursor accent (background fill) |

#### Status Indicator Colors

| State | Dot Hex | Label Pattern |
|-------|---------|--------------|
| Connected | `#22c55e` | `AYIN live · {N} steps` |
| Reconnecting | `#f59e0b` | `reconnecting… ({attempt})` |
| Offline | `#ef4444` | `AYIN offline` |

### 1.2 Typography

| Token | Value |
|-------|-------|
| `font-primary` | `'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace` |
| `font-size-terminal` | `14px` |
| `font-size-status` | `11px` |
| `font-size-error-title` | `0.875rem` (14px) |
| `font-size-error-detail` | `0.875rem` |
| `font-size-error-stack` | `0.7rem` |
| `font-size-btn-error` | `0.7rem` / `0.875rem` |
| `line-height-terminal` | `1.2` |

### 1.3 Spacing & Sizing

| Token | Value | Notes |
|-------|-------|-------|
| `viewport` | `100vh × 100vw` | Full-screen, no scrolling |
| `panel-default` | 50/50 split | Terminal 50%, Helix 50% |
| `panel-min` | 15% each | Minimum panel width |
| `resize-handle-width` | `4px` | Drag handle between panels |
| `status-badge-offset` | `bottom: 12px, left: 12px` | Absolute positioning |
| `status-dot-size` | `7px × 7px` | Circular indicator |
| `status-gap` | `6px` | Gap between dot and label |
| `error-padding` | `2rem` (default) / `1rem` (helix fallback) |
| `btn-padding` | `0.25rem 0.75rem` (small) / `0.5rem 1.5rem` (default) |
| `btn-border-radius` | `4px` |
| `notification-ttl` | `8000ms` | Auto-dismiss timeout |

### 1.4 Three.js Scene Parameters

| Parameter | Value | Source |
|-----------|-------|--------|
| Camera position | `[0, 0, 8]` | `HelixScene.tsx` |
| Camera FOV | `60°` | `HelixScene.tsx` |
| OrbitControls: enablePan | `false` | `HelixScene.tsx` |
| OrbitControls: minDistance | `3` | `HelixScene.tsx` |
| OrbitControls: maxDistance | `20` | `HelixScene.tsx` |
| OrbitControls: autoRotate | `false` | `HelixScene.tsx` |
| Ambient light intensity | `0.4` | `HelixScene.tsx` |
| Point light position | `[3, 5, 3]` | `HelixScene.tsx` |
| Point light intensity | `1.2` | `HelixScene.tsx` |
| Canvas background | `#0a0a0f` | `HelixScene.tsx` |
| WebGL antialias | `true` | `HelixScene.tsx` |
| WebGL alpha | `false` | `HelixScene.tsx` |

### 1.5 Helix Geometry Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `R_bundle` | `1.05` | Primary helix bundle radius |
| `w_twist` | `0.76` | Primary twist rate (rad/Y-unit) |
| `R_micro` | `0.3` | Entity orbit radius (micro-structure) |
| `w_micro` | `1.5` | Entity orbit twist rate |
| `R_nano` | `0.08` | Mini-anchor orbit radius |
| `w_nano` | `4.0` | Mini-anchor twist rate |
| `R_pico` | `0.025` | Sub-strand orbit radius |
| `w_pico` | `20.0` | Sub-strand twist rate |
| `tMin` | `-35` | Bottom Y boundary |
| `tMax` | `15` | Top Y boundary |
| `fadeDist` | `4.5` | Edge fade distance |
| Step pitch | `0.01` | Y-units between consecutive steps |
| Max visible steps | `5000` | Step eviction threshold |

### 1.6 3D Object Specs

| Object | Geometry | Detail | Color | Source |
|--------|----------|--------|-------|--------|
| Step sphere | `SphereGeometry(0.04, 8, 8)` | Low-poly | Actor color (dynamic) | `HelixScene.tsx` |
| Orb sphere | `SphereGeometry(0.07, 12, 12)` | Medium | `0x00f5ff` | `OrbEntity.tsx` |
| Hit ring | `RingGeometry(0.09, 0.14, 24)` | Flat ring | `0x00f5ff` | `OrbEntity.tsx` |
| Helix spine rail | `Line` (R3F drei) | 400 samples | `0x334155` | `HelixScene.tsx` |
| Orb material | `MeshBasicMaterial` | transparent, opacity fade | `0x00f5ff` | `OrbEntity.tsx` |
| Ring material | `MeshBasicMaterial` | transparent, double-sided, opacity pulse | `0x00f5ff` | `OrbEntity.tsx` |
| Step material | `MeshBasicMaterial` | opaque | Actor color | `HelixScene.tsx` |

### 1.7 Animation Timing

| Constant | Value | Source |
|----------|-------|--------|
| Step scale-in rate | `+0.05/frame` → clamp 1.0 | `HelixScene.tsx` |
| Orb travel speed | `0.12 sec/Y-unit` | `orbAnimator.ts` |
| Orb minimum travel | `0.12 sec` | `orbAnimator.ts` |
| Orb pulse dwell | `0.22 sec` | `orbAnimator.ts` |
| Orb fade-out window | `0.3 sec` before end | `OrbEntity.tsx` |
| Ring pulse rise | `arriveAt - 0.05 sec` | `orbAnimator.ts` |
| Ring pulse decay | `departAt + 0.25 sec` | `orbAnimator.ts` |
| Ring pulse function | `sin(π × t)` | `orbAnimator.ts` |
| Ring scale peak | `1.0 + intensity × 2.5` | `OrbEntity.tsx` |
| Ring opacity peak | `intensity × 0.8` | `OrbEntity.tsx` |
| SSE reconnect backoff | `1s → 2s → 4s → … → 30s cap` | `useEventSource.ts` |
| WS reconnect backoff | `1s → 2s → 4s → … → 30s cap` | `useTerminalSocket.ts` |
| Browser state report interval | `5 sec` | `useBrowserStateReporter.ts` |

---

## 2. Component Hierarchy (Figma Frame Tree)

```
Root: #root (100vh × 100vw)
├── ErrorBoundary
│   └── App
│       └── PanelGroup (horizontal, 100vh × 100vw)
│           ├── Panel "terminal" (default 50%, min 15%)
│           │   └── TerminalPane (100% × 100%, bg: #0a0a0f)
│           │       └── xterm.js container
│           │           └── Canvas/WebGL terminal
│           ├── PanelResizeHandle (4px wide, bg: #1e293b, cursor: col-resize)
│           └── Panel "helix" (default 50%, min 15%)
│               └── ErrorBoundary (isolated fallback)
│                   └── HelixScene (100% × 100%, position: relative)
│                       ├── Canvas (R3F, bg: #0a0a0f)
│                       │   ├── AmbientLight (intensity 0.4)
│                       │   ├── PointLight (pos [3,5,3], intensity 1.2)
│                       │   ├── HelixSpine
│                       │   │   ├── Line rail0 (400 samples, color: #334155)
│                       │   │   └── Line rail1 (400 samples, color: #334155)
│                       │   ├── StepCloud
│                       │   │   └── StepSphere[] (sphereGeometry r=0.04, seg=8)
│                       │   ├── OrbLayer
│                       │   │   └── OrbGroup[]
│                       │   │       ├── OrbSphere (sphereGeometry r=0.07, seg=12, color: #00f5ff)
│                       │   │       └── HitRing[] (ringGeometry inner=0.09, outer=0.14, seg=24)
│                       │   └── OrbitControls (noPan, minDist=3, maxDist=20)
│                       └── StatusBadge (absolute, bottom: 12px, left: 12px)
│                           ├── Dot (7×7 circle, color varies by state)
│                           └── Label (11px, #94a3b8)
```

---

## 3. Layout Specification

### 3.1 Viewport

- **Full-screen**: `100vh × 100vw`, no scroll, `overflow: hidden`
- **Background**: `#0a0a0f`
- **Font**: `'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace`
- **Color**: `#e2e8f0`

### 3.2 Panel Group

- **Direction**: Horizontal (`direction: "horizontal"`)
- **Container**: `100vh × 100vw`
- **Left Panel** (Terminal): default 50%, min 15%
- **Right Panel** (Helix): default 50%, min 15%

### 3.3 Resize Handle

- **Width**: `4px`
- **Background**: `#1e293b`
- **Cursor**: `col-resize`
- **Flex**: `flexShrink: 0`
- **Transition**: `background 150ms` (hover/active state)

### 3.4 Terminal Pane

- **Container**: `width: 100%`, `height: 100%`, `overflow: hidden`
- **Background**: `#0a0a0f`
- **Font**: `'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace`
- **Font Size**: `14px`
- **Line Height**: `1.2`
- **Cursor Blink**: Enabled
- **Scrollback**: `5000` lines
- **Renderer**: WebGL (primary), Canvas (fallback on context loss)
- **Addons**: FitAddon (auto-resize), WebLinksAddon (clickable URLs)

### 3.5 Helix Scene Container

- **Container**: `position: relative`, `width: 100%`, `height: 100%`
- **Canvas**: Three.js R3F `<Canvas>` inside the container
- **Canvas background**: `#0a0a0f`

### 3.6 Status Badge

- **Position**: `absolute`, `bottom: 12px`, `left: 12px`
- **Layout**: `flex`, `align-items: center`, `gap: 6px`
- **Font Size**: `11px`
- **Color**: `#94a3b8`
- **Pointer Events**: `none`
- **Dot**: `7px × 7px`, `border-radius: 50%`, background per state:
  - Connected: `#22c55e`
  - Reconnecting: `#f59e0b`
  - Offline: `#ef4444`

---

## 4. Three.js Scene Graph

### 4.1 Scene Composition

```
Scene
├── AmbientLight (intensity: 0.4)
├── PointLight (position: [3, 5, 3], intensity: 1.2)
├── Group: HelixSpine
│   ├── Line: rail0 (Vector3[401], color: 0x334155, lineWidth: 1)
│   └── Line: rail1 (Vector3[401], color: 0x334155, lineWidth: 1)
├── Group: StepCloud
│   └── StepSphere[] (per step)
│       └── Mesh
│           ├── sphereGeometry (radius: 0.04, widthSeg: 8, heightSeg: 8)
│           └── meshBasicMaterial (color: step.actorColor)
├── Group: OrbLayer
│   └── OrbGroup[] (per active orb, max 5)
│       ├── OrbSphere
│       │   └── Mesh
│       │       ├── sphereGeometry (radius: 0.07, widthSeg: 12, heightSeg: 12)
│       │       └── meshBasicMaterial (color: 0x00f5ff, transparent: true)
│       └── HitRing[] (per pulse waypoint)
│           └── Mesh
│               ├── ringGeometry (innerRadius: 0.09, outerRadius: 0.14, thetaSeg: 24)
│               └── meshBasicMaterial (color: 0x00f5ff, transparent: true, side: DoubleSide)
└── OrbitControls
    ├── enablePan: false
    ├── minDistance: 3
    ├── maxDistance: 20
    └── autoRotate: false
```

### 4.2 Helix Spine Math

The double-helix is defined by two parametric rails that twist around the Y axis:

```
Rail i at height Y:
  θ = Y × w_twist + (i === 0 ? 0 : π)
  X = R_bundle × cos(θ)
  Z = R_bundle × sin(θ)
  Position = (X, Y, Z)
```

- **R_bundle** = 1.05 units
- **w_twist** = 0.76 rad/Y-unit
- **Y range**: tMin (-35) → tMax (15), total span = 50 Y-units
- **Sampling**: 400 intervals (401 points per rail)
- **Fade**: Within 4.5 Y-units of each boundary, opacity fades linearly to 0

### 4.3 Entity Distribution

Each of the 6 sibling entities orbits its assigned rail at a micro-structure radius:

| Entity | Rail | Color | Phase Offset | Age Factor | Orbit Radius |
|--------|------|-------|-------------|------------|-------------|
| EVA | 0 | `#FF1493` | 0° | 0.468 (171/365) | `0.7 + 0.3 × 0.468 = 0.840`
| CORSO | 0 | `#00BFFF` | 120° | 0.118 (43/365) | `0.7 + 0.3 × 0.118 = 0.735`
| QUANTUM | 0 | `#B44AFF` | 240° | 0.471 (172/365) | `0.841`
| SERAPH | 1 | `#FF0040` | 0° | 0.060 (22/365) | `0.718`
| L-ARC | 1 | `#F59E0B` | 120° | 1.000 (381/365 clamped) | `1.000`
| AYIN | 1 | `#FF6D00` | 240° | 0.014 (5/365) | `0.704`

Phase offsets are `entityIdx % 3 × (2π/3)`. Age factor = `min(age/365, 1.0)`, affecting the micro-structure orbit radius from `0.7×R_micro` to `1.0×R_micro`.

### 4.4 SOUL Center

The SOUL position at any Y is the midpoint of the two primary rail centers:

```
SOUL(Y) = (Rail0_C(Y) + Rail1_C(Y)) / 2
```

### 4.5 Camera & Controls

- **Default position**: `(0, 0, 8)`
- **Field of view**: `60°`
- **Near/Far**: Three.js defaults (0.1 / 1000)
- **OrbitControls**: 
  - Pan disabled
  - Zoom range: 3–20 units
  - No auto-rotation
  - Target: scene origin `(0, 0, 0)`

---

## 5. State Architecture

### 5.1 Zustand Store (`sceneState.ts`)

```
SceneState {
  steps: SessionStep[]          // Max 5000, FIFO eviction
  orbQueue: OrbState[]           // Max 5, FIFO eviction  
  ayinStatus: AyinConnStatus     // { connected, reconnecting, attempt }
  activePanel: string            // 'terminal' | 'helix'
  panelVisibility: { terminal: bool, helix: bool }
  panelSizes: { terminal: number, helix: number }  // Percentages, sum = 100
  helixZoom: number              // Zoom level (default 5)
  notifications: Notification[]  // Auto-dismiss after 8s
}
```

### 5.2 SSE Event Types

| SSE Event | Action | Store Mutation |
|-----------|--------|---------------|
| `ayin_span` | `addStep(id, actor, action)` | Push to `steps[]`, assign Y, color |
| `ayin_status` | `setAyinStatus(...)` | Update connection state |
| `helix_entry` | `spawnOrb(queryId, hitStepIds)` | Create orb with waypoint path |
| `control` | Various | `focusPanel`, `setPanelVisibility`, `resizePanels`, `setHelixZoom`, `pushNotification` |
| `lag` | Console warning only | No store mutation |
| `build_update` | Removed from frontend | (Backend still emits) |

### 5.3 WebSocket Protocol

| Direction | Frame Type | Payload |
|-----------|-----------|---------|
| Server → Browser | Binary | Raw PTY stdout bytes |
| Browser → Server | Binary | Keystrokes/paste (UTF-8) |
| Browser → Server | Text (JSON) | `{"type":"resize","cols":N,"rows":M}` |
| Browser → Server | Text (JSON) | `{"type":"ping"}` |

Auth: WebSocket sub-protocol `bearer.<token>`

---

## 6. Error States

### 6.1 Helix Error Fallback

When the Three.js scene crashes, a minimal fallback is shown in the helix panel:

- **Container**: `height: 100%`, `flex`, centered, `background: #0a0a0f`
- **Title**: "Helix unavailable" — `color: #94a3b8`, `margin-bottom: 0.5rem`
- **Message**: Error message — `color: inherited`, `margin-bottom: 0.75rem`
- **Retry button**: `background: #1e293b`, `border: 1px solid #334155`, `color: #94a3b8`, `padding: 0.25rem 0.75rem`, `border-radius: 4px`, `font-size: 0.7rem`

### 6.2 Full-Page Error

When the entire app crashes (top-level ErrorBoundary):

- **Container**: `100vh × 100vw`, centered, `background: #0a0a0f`, `color: #e2e8f0`
- **Title**: "Something went wrong" — `font-size: 2rem`, `margin-bottom: 1rem`
- **Detail**: Error message — `font-size: 0.875rem`, `color: #94a3b8`
- **Toggle Stack button**: `background: none`, `border: 1px solid #334155`, `color: #94a3b8`, `padding: 0.25rem 0.75rem`, `border-radius: 4px`, `font-size: 0.75rem`
- **Stack trace** (collapsible): `font-size: 0.7rem`, `color: #64748b`, `background: #1e293b`, `padding: 0.75rem`, `border-radius: 4px`, `max-height: 200px`
- **Reload button**: `background: #1e293b`, `border: 1px solid #334155`, `color: #e2e8f0`, `padding: 0.5rem 1.5rem`, `border-radius: 4px`, `font-size: 0.875rem`

---

## 7. Figma Component Map

### 7.1 Recommended Figma Component Structure

```
🎨 Light Architects Webshell
│
├── 📐 Design Tokens
│   ├── Colors (all palettes above)
│   ├── Typography (JetBrains Mono hierarchy)
│   └── Spacing (4px grid system)
│
├── 🖼️ Frames
│   ├── Desktop — 1440×900 (default viewport)
│   ├── Desktop — 1920×1080 (large viewport)
│   └── Mobile — 375×812 (portrait, if needed)
│
├── 🧩 Components
│   ├── Terminal Pane
│   │   ├── Terminal Container (bg: #0a0a0f)
│   │   └── Terminal Cursor (color: #00f5ff, blink)
│   │
│   ├── Resize Handle
│   │   └── 4px wide bar (bg: #1e293b, hover: lighten)
│   │
│   ├── Helix Scene
│   │   ├── 3D Viewport (bg: #0a0a0f)
│   │   ├── Helix Spine (2 rails, #334155)
│   │   ├── Step Sphere (r=0.04, actor colors)
│   │   ├── Orb Sphere (r=0.07, #00f5ff)
│   │   ├── Hit Ring (inner r=0.09, outer r=0.14, #00f5ff)
│   │   └── Status Badge
│   │       ├── Connected (dot: #22c55e)
│   │       ├── Reconnecting (dot: #f59e0b)
│   │       └── Offline (dot: #ef4444)
│   │
│   ├── Error States
│   │   ├── Helix Fallback (minimal)
│   │   └── Full-Page Error (with stack trace)
│   │
│   └── Notifications
│       ├── Info (#00f5ff accent)
│       ├── Warn (#f59e0b accent)
│       └── Error (#ef4444 accent)
│
└── 🎭 Variants
    ├── Panel Split — 50/50 (default)
    ├── Panel Split — 70/30 (terminal focused)
    ├── Panel Split — 30/70 (helix focused)
    ├── Panel Split — Terminal only
    └── Panel Split — Helix only
```

### 7.2 Figma Auto-Layout Settings

| Component | Direction | Spacing | Padding |
|-----------|-----------|---------|----------|
| Root | Horizontal | 0 | 0 |
| Terminal Panel | Vertical | 0 | 0 |
| Helix Panel | Vertical | 0 | 0 |
| Status Badge | Horizontal | 6px | 0 |
| Error Fallback | Vertical | varies | 2rem |
| Full-Page Error | Vertical | varies | 2rem |

---

## 8. Three.js Reference Implementation

All Three.js implementation uses **React Three Fiber (R3F)** v9.5+ with **@react-three/drei** v10.7+.

### 8.1 Key Dependencies

| Package | Version | Purpose |
|---------|---------|----------|
| `three` | `^0.183.0` | Core 3D engine |
| `@react-three/fiber` | `^9.5.0` | React renderer for Three.js |
| `@react-three/drei` | `^10.7.0` | Helpers (OrbitControls, Line) |
| `@xterm/xterm` | `^5.5.0` | Terminal emulator |
| `@xterm/addon-webgl` | `^0.18.0` | GPU-accelerated terminal rendering |
| `@xterm/addon-canvas` | `^0.7.0` | Canvas fallback renderer |
| `@xterm/addon-fit` | `^0.10.0` | Auto-fit terminal to container |
| `@xterm/addon-web-links` | `^0.11.0` | Clickable URLs |
| `react-resizable-panels` | `^2.1.0` | Split-pane layout |
| `zustand` | `^5.0.0` | State management |

### 8.2 Scene Initialization

```tsx
<Canvas
  camera={{ position: [0, 0, 8], fov: 60 }}
  gl={{ antialias: true, alpha: false }}
  style={{ background: '#0a0a0f' }}
>
```

### 8.3 Helix Position Computation

Reference: `web/src/helix-math.ts`

```
getPrimaryFrame(railIdx, y) → { C, N, B }
  C = center point on rail at height y
  N = normal vector (radial outward)
  B = binormal vector
  T = tangent vector (computed internally)
```

### 8.4 Orb Animation Pipeline

Reference: `web/src/three/orbAnimator.ts`

1. `buildOrbPath(originY, hitYPositions)` — builds waypoint array with timing
2. `orbPositionFromPath(elapsed, waypoints)` — O(k) per-frame position lookup
3. `waypointPulseIntensity(elapsed, waypoint)` — sin(π×t) pulse envelope
4. `tickOrbs(delta)` — advance elapsed time, evict completed orbs

### 8.5 Performance Budget

| Metric | Value |
|--------|-------|
| Max steps | 5,000 spheres |
| Max concurrent orbs | 5 |
| Sphere geometry segments | 8 (step) / 12 (orb) |
| Ring geometry segments | 24 |
| Helix spine samples | 400 per rail |
| Step geometry radius | 0.04 units |
| Orb geometry radius | 0.07 units |

---

## 9. Data Flow Diagram

```
┌──────────────────────────────────────────────────────────┐
│                     Browser (React)                      │
│                                                           │
│  ┌─────────────┐    ┌──────────────┐    ┌─────────────┐  │
│  │  SSE Client  │───▶│ Zustand Store │───▶│ HelixScene   │  │
│  │  /api/events │    │ sceneState    │    │ (R3F Canvas) │  │
│  └─────────────┘    └──────┬───────┘    └─────────────┘  │
│                            │                               │
│  ┌─────────────┐          │          ┌─────────────┐      │
│  │  WS Client    │───▶ TerminalPane  │ BrowserState │      │
│  │  /api/terminal│    (xterm.js)     │  Reporter     │      │
│  │     /ws       │                   │  POST /api/   │      │
│  └─────────────┘                   │  browser-state│      │
│                                     └─────────────┘      │
└──────────────────────────────────────────────────────────┘
                         │
                    Auth: Bearer token
                    (#token= or sessionStorage)
                         │
┌──────────────────────────────────────────────────────────┐
│                  Rust/Axum Backend                         │
│                                                           │
│  GET /api/health ─── Unauthenticated liveness             │
│  GET /api/auth-check ── Bearer token validation            │
│  GET /api/terminal/ws ── PTY WebSocket bridge             │
│  GET /api/events ────── SSE fan-out (authenticated)       │
│  POST /api/control ─── External commands                 │
│  GET /api/builds ────── Build tracking (authenticated)    │
│  GET/POST /api/browser-state ── Viewport sync             │
│  GET /* ─────────────── Embedded SPA fallback              │
└──────────────────────────────────────────────────────────┘
```

---

## 10. File Reference Index

| File | Purpose |
|------|----------|
| `web/src/App.tsx` | Root layout, PanelGroup split, SSE hook |
| `web/src/main.tsx` | Entry point, StrictMode, ErrorBoundary wrap |
| `web/src/index.html` | Base HTML, global reset styles |
| `web/src/helix-math.ts` | All helix geometry constants and position functions |
| `web/src/store/sceneState.ts` | Zustand store, actor colors, state shape, actions |
| `web/src/three/HelixScene.tsx` | R3F Canvas, lights, spine, step spheres, status badge |
| `web/src/three/OrbEntity.tsx` | Orb spheres, hit rings, per-frame animation |
| `web/src/three/orbAnimator.ts` | Waypoint path math, easing, pulse intensity |
| `web/src/components/Terminal/TerminalPane.tsx` | xterm.js terminal, theme, WebGL renderer |
| `web/src/components/Terminal/useTerminalSocket.ts` | WebSocket connection, resize protocol |
| `web/src/components/ErrorBoundary.tsx` | React error boundary, fallback UI |
| `web/src/hooks/useEventSource.ts` | SSE connection, event dispatch, reconnect |
| `web/src/hooks/useBrowserStateReporter.ts` | Periodic viewport state POST |
| `web/src/lib/auth.ts` | Token resolution (hash fragment → sessionStorage) |
| `web/package.json` | Dependencies and versions |
| `web/vite.config.ts` | Build config, dev proxy, test settings |

---

## 11. Figma Import Notes

### What Figma Cannot Reproduce

1. **Three.js 3D Scene**: The helix visualization is a live WebGL scene. In Figma, represent it as a **dark placeholder frame** (`#0a0a0f`) with a **screenshot or illustration** showing the double-helix structure.

2. **xterm.js Terminal**: The terminal is a full canvas-based emulator. In Figma, represent it as a **dark container** (`#0a0a0f`) with **monospace placeholder text** in `#e2e8f0`.

3. **Orb Animations**: Orbs travel the helix with ease-in-out cubic easing. In Figma, create a **motion prototype** showing: spawn → travel → pulse ring → fade out.

4. **Resize Interaction**: The panel split is draggable. In Figma, create **variant states** for different split ratios (50/50, 70/30, 30/70).

### What Figma Should Reproduce Exactly

- All color tokens (Section 1.1)
- Typography tokens (Section 1.2)
- Spacing tokens (Section 1.3)
- Status badge styling and states
- Error boundary fallback UI
- Button styling
- Resize handle appearance
- Panel proportions and constraints

### Suggested Figma Plugin for Three.js

To bridge Three.js → Figma, consider:
- **[Three.js Figma Plugin](https://www.figma.com/community/plugin/three-js)** — Renders 3D scenes as vector layers
- **Screenshot approach**: Capture the helix scene at key states and import as PNG layers
- **SVG export**: The helix spine can be projected to 2D and exported as SVG paths using the `sampleRail()` function with orthographic projection

---

## 12. Visual Reference Links

### Design Inspiration

- **Cyberpunk terminal aesthetic**: Dark near-black backgrounds with neon cyan accents
- **Double helix structure**: DNA-inspired visualization of the SOUL knowledge graph
- **xterm.js**: [xtermjs.org](https://xtermjs.org/) — terminal emulator documentation
- **React Three Fiber**: [r3f.docs.pmnd.rs](https://r3f.docs.pmnd.rs/) — React renderer for Three.js
- **@react-three/drei**: [github.com/pmndrs/drei](https://github.com/pmndrs/drei) — Useful helpers for R3F
- **react-resizable-panels**: [github.com/bvaughn/react-resizable-panels](https://github.com/bvaughn/react-resizable-panels) — Panel layout library

### Color Reference

- **Tailwind Slate palette**: The `#1e293b`, `#334155`, `#64748b`, `#94a3b8`, `#e2e8f0`, `#f8fafc` colors are from the Tailwind Slate scale
- **Tailwind colors used**: slate (borders, text), red (errors, SERAPH), green (success, AYIN live), amber (warnings, L-ARC), blue (CORSO), violet (QUANTUM), pink (EVA), orange (AYIN actor)
- **Neon cyan `#00f5ff`**: Primary accent color throughout the interface

---

*End of specification. This document was auto-generated from the production codebase at `lightarchitects-webshell/web/src/`.*
