"use client";
import { useRef, useEffect } from 'react';
import * as THREE from 'three';
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
import {
  tMin, tMax, entities, R_bundle, w_twist, R_micro, w_micro,
  R_nano, w_nano, R_pico, w_pico,
  getFade, getPrimaryFrame, getEntityCenter, getMiniAnchorFrame,
  getSubStrandPos, seededRandom,
} from './helix-math';
import { HelixPolytopeManager } from './helix-polytopes';
import { HelixInteraction } from './helix-interaction';
import type { ProjectEntry } from '../data/projects';
import type { BlogPost } from '../data/blog-posts';

function createGlowTexture() {
  const canvas = document.createElement('canvas');
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext('2d');
  if (ctx) {
    const gradient = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
    gradient.addColorStop(0, 'rgba(255, 255, 255, 1)');
    gradient.addColorStop(0.2, 'rgba(255, 255, 255, 0.8)');
    gradient.addColorStop(0.5, 'rgba(255, 255, 255, 0.2)');
    gradient.addColorStop(1, 'rgba(255, 255, 255, 0)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 64, 64);
  }
  return new THREE.CanvasTexture(canvas);
}

interface Hero3DProps {
  onFocus?: (project: ProjectEntry | null) => void;
  onBlogFocus?: (post: BlogPost | null) => void;
  onHover?: (project: ProjectEntry | null) => void;
  navigateRef?: React.MutableRefObject<((projectId: string) => void) | null>;
  unfocusRef?: React.MutableRefObject<(() => void) | null>;
  highlightRef?: React.MutableRefObject<((projectId: string) => void) | null>;
}

export const Hero3D = ({ onFocus, onBlogFocus, onHover, navigateRef, unfocusRef, highlightRef }: Hero3DProps = {}) => {
  const mountRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!mountRef.current) return;

    const width = mountRef.current.clientWidth;
    const height = mountRef.current.clientHeight;

    // SCENE SETUP
    const scene = new THREE.Scene();
    scene.fog = new THREE.FogExp2(0x000000, 0.06);
    
    const glowTexture = createGlowTexture();

    const camera = new THREE.PerspectiveCamera(60, width / height, 0.1, 200);
    camera.position.set(0, 0, 5.5); 
    camera.lookAt(0, 0, 0);

    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, powerPreference: "high-performance" });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5)); // Cap at 1.5 for perf (indistinguishable from 2 with bloom)
    renderer.setSize(width, height);
    // Clear any stale canvas from React Strict Mode double-mount
    while (mountRef.current.firstChild) {
      mountRef.current.removeChild(mountRef.current.firstChild);
    }
    mountRef.current.appendChild(renderer.domElement);

    // POST-PROCESSING PIPELINE
    const renderScene = new RenderPass(scene, camera);
    const composer = new EffectComposer(renderer);
    composer.addPass(renderScene);

    const bloomPass = new UnrealBloomPass(new THREE.Vector2(width, height), 1.0, 0.6, 0.25);
    bloomPass.threshold = 0.25;  // Slightly higher — only bright elements bloom
    bloomPass.strength = 1.1;    // Dialed back for cinematic subtlety
    bloomPass.radius = 0.6;      // Tighter glow radius — less hazy
    composer.addPass(bloomPass);

    // ATMOSPHERIC DATA DUST (Layered & Colored)
    const dustGroup = new THREE.Group();
    scene.add(dustGroup);

    // Layer 1: Fine, sharp data dust
    const fineDustGeom = new THREE.BufferGeometry();
    const fineDustCount = 600; // Reduced for performance — bloom makes them look denser
    const fineDustPos = new Float32Array(fineDustCount * 3);
    const fineDustCol = new Float32Array(fineDustCount * 3);
    
    const palette = [
      new THREE.Color(0xFF1493), // Deep pink (EVA)
      new THREE.Color(0x00BFFF), // Deep sky blue (CORSO)
      new THREE.Color(0xB44AFF), // Bright purple (QUANTUM)
      new THREE.Color(0xFFD700), // Gold (User/L-ARC)
      new THREE.Color(0xFF6D00), // Highlighter orange (AYIN)
      new THREE.Color(0xffffff)  // White
    ];

    for (let i = 0; i < fineDustCount; i++) {
       fineDustPos[i*3] = (Math.random() - 0.5) * 15;
       fineDustPos[i*3+1] = (Math.random() - 0.5) * 15;
       fineDustPos[i*3+2] = (Math.random() - 0.5) * 15;
       
       const color = palette[Math.floor(Math.random() * palette.length)];
       fineDustCol[i*3] = color.r;
       fineDustCol[i*3+1] = color.g;
       fineDustCol[i*3+2] = color.b;
    }
    fineDustGeom.setAttribute('position', new THREE.BufferAttribute(fineDustPos, 3));
    fineDustGeom.setAttribute('color', new THREE.BufferAttribute(fineDustCol, 3));

    const fineDustMat = new THREE.PointsMaterial({
      size: 0.05,
      transparent: true,
      opacity: 0.25,
      vertexColors: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      map: glowTexture
    });
    const fineDustSystem = new THREE.Points(fineDustGeom, fineDustMat);
    dustGroup.add(fineDustSystem);

    // Layer 2: Ambient Bokeh/Nebula (large, soft particles)
    const bokehGeom = new THREE.BufferGeometry();
    const bokehCount = 30; // Minimal nebula — agents and polytopes are the focus
    const bokehPos = new Float32Array(bokehCount * 3);
    const bokehCol = new Float32Array(bokehCount * 3);
    for (let i = 0; i < bokehCount; i++) {
       bokehPos[i*3] = (Math.random() - 0.5) * 20;
       bokehPos[i*3+1] = (Math.random() - 0.5) * 20;
       // Push backwards slightly so they sit behind the main structure
       bokehPos[i*3+2] = (Math.random() - 0.5) * 8 - 4; // Push further back
       
       const color = palette[Math.floor(Math.random() * 3)]; // Exclude white
       bokehCol[i*3] = color.r;
       bokehCol[i*3+1] = color.g;
       bokehCol[i*3+2] = color.b;
    }
    bokehGeom.setAttribute('position', new THREE.BufferAttribute(bokehPos, 3));
    bokehGeom.setAttribute('color', new THREE.BufferAttribute(bokehCol, 3));

    const bokehMat = new THREE.PointsMaterial({
      size: 0.12, // Shrink
      transparent: true,
      opacity: 0.05, // Fade
      vertexColors: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      map: glowTexture
    });
    const bokehSystem = new THREE.Points(bokehGeom, bokehMat);
    dustGroup.add(bokehSystem);

    // SCROLL TRACKING
    let scrollPercent = 0;
    const handleScroll = () => {
      const scrollTop = window.scrollY;
      const maxScroll = document.documentElement.scrollHeight - window.innerHeight;
      scrollPercent = maxScroll > 0 ? scrollTop / maxScroll : 0;
    };
    // Initialize
    handleScroll();
    window.addEventListener('scroll', handleScroll);

    // LIGHTING
    const ambient = new THREE.AmbientLight(0xffffff, 0.15);
    scene.add(ambient);

    const group = new THREE.Group();
    group.position.x = 0; // Centered — UI is at the bottom, helix fills the viewport
    scene.add(group);

    // Non-rotating group for outer-tier polytopes (syncs Y scroll, no rotation)
    const outerPolytopeGroup = new THREE.Group();
    scene.add(outerPolytopeGroup);

    // 4D POLYTOPE LANDMARKS (helix-embedded + outer satellites)
    const polytopeManager = new HelixPolytopeManager(group, glowTexture, outerPolytopeGroup);

    // HELPER FUNCTIONS
    const makePoints = (posArray: number[], colArray: number[], size: number, opacity: number) => {
      const geom = new THREE.BufferGeometry();
      geom.setAttribute('position', new THREE.Float32BufferAttribute(posArray, 3));
      geom.setAttribute('color', new THREE.Float32BufferAttribute(colArray, 3));
      const mat = new THREE.PointsMaterial({
        size,
        sizeAttenuation: true,
        vertexColors: true,
        transparent: true,
        opacity,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        map: glowTexture
      });
      return new THREE.Points(geom, mat);
    };

    const makeLines = (posArray: number[], colArray: number[], opacity: number, width = 1) => {
      const geom = new THREE.BufferGeometry();
      geom.setAttribute('position', new THREE.Float32BufferAttribute(posArray, 3));
      geom.setAttribute('color', new THREE.Float32BufferAttribute(colArray, 3));
      const mat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        linewidth: width
      });
      return new THREE.LineSegments(geom, mat);
    };

    // --- DATA GENERATION (Fractal Consciousness Model) ---
    // Position math imported from helix-math.ts (shared with helix-polytopes.ts)
    // Convert raw hex colors to THREE.Color once — used throughout the helix builder
    const entityColors = entities.map(e => new THREE.Color(e.color));

    // Constants and functions imported from helix-math.ts

    // 1. BUILD CONTINUOUS RAILS
    const numSegments = 800; // Reduced — primary rails are barely visible guide lines
    
    // Primary Anchor Rails (Barely visible guide lines)
    const pRailPos: number[] = [];
    const pRailCol: number[] = [];
    
    const pRailColor = 0.08; // Very subtle grey
    for(let i=0; i<numSegments; i++) {
        const y1 = tMin + (tMax - tMin) * (i / numSegments);
        const y2 = tMin + (tMax - tMin) * ((i + 1) / numSegments);
        
        const p0_1 = getPrimaryFrame(0, y1).C;
        const p0_2 = getPrimaryFrame(0, y2).C;
        const p1_1 = getPrimaryFrame(1, y1).C;
        const p1_2 = getPrimaryFrame(1, y2).C;
        
        const pRailColor1 = 0.08 * getFade(y1);
        const pRailColor2 = 0.08 * getFade(y2);
        
        pRailPos.push(p0_1.x, p0_1.y, p0_1.z, p0_2.x, p0_2.y, p0_2.z);
        pRailCol.push(pRailColor1, pRailColor1, pRailColor1, pRailColor2, pRailColor2, pRailColor2);
        
        pRailPos.push(p1_1.x, p1_1.y, p1_1.z, p1_2.x, p1_2.y, p1_2.z);
        pRailCol.push(pRailColor1, pRailColor1, pRailColor1, pRailColor2, pRailColor2, pRailColor2);
    }
    
    // Add thin primary rails
    group.add(makeLines(pRailPos, pRailCol, 0.15, 1));

    const domPos: number[] = [], domCol: number[] = [];
    const mRailPos: number[] = [], mRailCol: number[] = [];
    
    // We need more segments for the micro-strands to capture the tight helix curve smoothly
    const microSegments = 3500; // Reduced — helix curves still smooth at this density
    
    for (let i = 0; i < microSegments; i++) {
        const t1 = tMin + (tMax - tMin) * (i / microSegments);
        const t2 = tMin + (tMax - tMin) * ((i + 1) / microSegments);
        
        for (let s = 0; s < 6; s++) {
            const entity = entities[s];
            const ageFactor = Math.min(entity.age / 365, 1.0);

            const hsl = {h:0, s:0, l:0};
            entityColors[s].getHSL(hsl);
            const col = new THREE.Color().setHSL(hsl.h, Math.min(hsl.s * 1.2, 1.0), Math.min(hsl.l * 1.5, 0.7));

            // Temporal breathing (adds hot/cold zones to strands)
            const breathe1 = Math.sin(t1 * 0.5 + s * 1.7) * 0.1 + 0.9;
            const breathe2 = Math.sin(t2 * 0.5 + s * 1.7) * 0.1 + 0.9;

            const baseOpacity = 0.8 + 0.2 * ageFactor;
            const fade1 = getFade(t1);
            const fade2 = getFade(t2);

            const op1 = baseOpacity * breathe1 * fade1;
            const op2 = baseOpacity * breathe2 * fade2;

            // Divide strands across the 2 mini rails
            const numM0 = Math.ceil(entity.strands / 2);
            const numM1 = Math.floor(entity.strands / 2);

            for (let d = 0; d < numM0; d++) {
                const d1 = getSubStrandPos(s, 0, d, numM0, t1);
                const d2 = getSubStrandPos(s, 0, d, numM0, t2);
                domPos.push(d1.x, d1.y, d1.z, d2.x, d2.y, d2.z);
                domCol.push(col.r*op1, col.g*op1, col.b*op1, col.r*op2, col.g*op2, col.b*op2);
            }
            for (let d = 0; d < numM1; d++) {
                const d1 = getSubStrandPos(s, 1, d, numM1, t1);
                const d2 = getSubStrandPos(s, 1, d, numM1, t2);
                domPos.push(d1.x, d1.y, d1.z, d2.x, d2.y, d2.z);
                domCol.push(col.r*op1, col.g*op1, col.b*op1, col.r*op2, col.g*op2, col.b*op2);
            }
            
            // Mini anchor rails for each entity
            const mOp = 0.2 + 0.5 * ageFactor;
            const m1_a = getMiniAnchorFrame(s, 0, t1).M;
            const m1_b = getMiniAnchorFrame(s, 0, t2).M;
            const m2_a = getMiniAnchorFrame(s, 1, t1).M;
            const m2_b = getMiniAnchorFrame(s, 1, t2).M;
            
            const mop1 = 0.35 * mOp * fade1;
            const mop2 = 0.35 * mOp * fade2;

            mRailPos.push(m1_a.x, m1_a.y, m1_a.z, m1_b.x, m1_b.y, m1_b.z);
            mRailCol.push(mop1, mop1, mop1, mop2, mop2, mop2);
            mRailPos.push(m2_a.x, m2_a.y, m2_a.z, m2_b.x, m2_b.y, m2_b.z);
            mRailCol.push(mop1, mop1, mop1, mop2, mop2, mop2);
        }
    }
    group.add(makeLines(domPos, domCol, 1.0, 1.5));
    group.add(makeLines(mRailPos, mRailCol, 1.0, 1.0));

    // 2. NODE PLACEMENT
    const entityNodes: number[][] = [[], [], [], [], [], []];
    const mNodesPos: number[] = [], mNodesCol: number[] = [];
    const mNodesBrightPos: number[] = [], mNodesBrightCol: number[] = [];
    const mNodesHaloPos: number[] = [], mNodesHaloCol: number[] = [];

    for (let s = 0; s < 6; s++) {
        const entity = entities[s];
        const ageFactor = Math.min(entity.age / 365, 1.0);
        const colorSaturation = 0.4 + 0.6 * ageFactor;
        const hsl = {h:0, s:0, l:0};
        entityColors[s].getHSL(hsl);
        const col = new THREE.Color().setHSL(hsl.h, hsl.s * colorSaturation, hsl.l);

        if (entity.id === 'seraph') {
            entityNodes[s].push(0);
            const E_pos = getEntityCenter(s, 0).E;
            
            mNodesPos.push(E_pos.x, E_pos.y, E_pos.z);
            const fade = getFade(0);
            mNodesCol.push(col.r * 1.5 * fade, col.g * 1.5 * fade, col.b * 1.5 * fade);
        } else {
            const rnd = seededRandom(42 + s);
            const baseSpacing = (tMax - tMin) / Math.max(entity.entries, 1);
            let currY = tMin;
            while (currY <= tMax) {
                entityNodes[s].push(currY);
                const E_pos = getEntityCenter(s, currY).E;
                const fade = getFade(currY);
                
                // MOCK SIGNIFICANCE SCORE (0.0 to 10.0)
                const significance = rnd() * 10;
                
                if (significance >= 7.0) {
                    // Self-defining moment: Larger, brighter, with halo
                    mNodesBrightPos.push(E_pos.x, E_pos.y, E_pos.z);
                    mNodesBrightCol.push(col.r * 2.0 * fade, col.g * 2.0 * fade, col.b * 2.0 * fade);
                    
                    mNodesHaloPos.push(E_pos.x, E_pos.y, E_pos.z);
                    mNodesHaloCol.push(col.r * 0.5 * fade, col.g * 0.5 * fade, col.b * 0.5 * fade);
                } else {
                    // Regular moment: scale based on significance
                    mNodesPos.push(E_pos.x, E_pos.y, E_pos.z);
                    const scaleFactor = 0.5 + (significance / 10.0) * 1.0;
                    mNodesCol.push(col.r * 1.5 * scaleFactor * fade, col.g * 1.5 * scaleFactor * fade, col.b * 1.5 * scaleFactor * fade);
                }
                
                // Clustered deterministic spacing
                const step = baseSpacing * (0.7 + 0.6 * rnd());
                currY += step;
            }
        }
    }
    group.add(makePoints(mNodesPos, mNodesCol, 0.06, 0.9));
    group.add(makePoints(mNodesBrightPos, mNodesBrightCol, 0.12, 1.0));
    group.add(makePoints(mNodesHaloPos, mNodesHaloCol, 0.25, 0.4));

    // 3. STEPS & RUNGS
    const pStepPos: number[] = [], pStepCol: number[] = [];
    const connPos: number[] = [], connCol: number[] = [];
    const pNodesPos: number[] = [], pNodesCol: number[] = [];
    const stepHaloPos: number[] = [], stepHaloCol: number[] = [];
    const mStepPos: number[] = [], mStepCol: number[] = [];
    const crossConnPos: number[] = [], crossConnCol: number[] = [];

    const convergenceNodes: any[] = [];

    const numTicks = Math.round((tMax - tMin) / 0.05);
    for (let tick = 0; tick <= numTicks; tick++) {
        const y = tMin + tick * 0.05;
        const isPrimary = tick % 20 === 0;
        const isSub = !isPrimary && tick % 5 === 0;
        const isSubSub = !isPrimary && !isSub;
        const fadeY = getFade(y);

        // Mini steps (rungs) for each entity
        const isMiniStep = tick % 4 === 0; // step every 0.2 units
        if (isMiniStep) {
            for (let s = 0; s < 6; s++) {
                const entity = entities[s];
                const ageFactor = Math.min(entity.age / 365, 1.0);
                const mOp = (0.2 + 0.4 * ageFactor) * fadeY;
                
                const m1 = getMiniAnchorFrame(s, 0, y).M;
                const m2 = getMiniAnchorFrame(s, 1, y).M;
                mStepPos.push(m1.x, m1.y, m1.z, m2.x, m2.y, m2.z);
                mStepCol.push(0.35*mOp, 0.35*mOp, 0.35*mOp, 0.35*mOp, 0.35*mOp, 0.35*mOp);
            }
        }

        // Structural Guy-Wires (Entity Mini-rails directly bridging to Primary Rails)
        if (tick % 7 === 0) {
            for (let s = 0; s < 6; s++) {
                // 30% chance an entity binds directly to its primary rail host
                if (Math.random() < 0.3) {
                    const eCenter = getEntityCenter(s, y).E;
                    const pFrame = getPrimaryFrame(entities[s].rail, y).C;
                    
                    crossConnPos.push(eCenter.x, eCenter.y, eCenter.z, pFrame.x, pFrame.y, pFrame.z);
                    const c = entityColors[s];
                    crossConnCol.push(c.r*0.3*fadeY, c.g*0.3*fadeY, c.b*0.3*fadeY, 0.15*fadeY, 0.15*fadeY, 0.15*fadeY);
                }
            }
        }

        // Cross-Entity Links (Server to Server communication links)
        // Visually differentiating Neo4j LinkTypes: Wikilink (Solid), InspiredBy (Curved), Contradicts (Red tension)
        if (tick % 5 === 0) {
            for (let s1 = 0; s1 < 6; s1++) {
                for (let s2 = s1 + 1; s2 < 6; s2++) {
                    // Only bridge if they are on different rails or structurally adjacent
                    if (Math.random() < 0.12) { // 12% chance per pair every 0.25 units
                        const e1 = getEntityCenter(s1, y).E;
                        const e2 = getEntityCenter(s2, y).E;
                        const c1 = entityColors[s1];
                        const c2 = entityColors[s2];
                        
                        const linkType = Math.random();
                        if (linkType < 0.4) {
                            // Wikilink: Solid line using entity colors
                            crossConnPos.push(e1.x, e1.y, e1.z, e2.x, e2.y, e2.z);
                            crossConnCol.push(c1.r * 0.4 * fadeY, c1.g * 0.4 * fadeY, c1.b * 0.4 * fadeY);
                            crossConnCol.push(c2.r * 0.4 * fadeY, c2.g * 0.4 * fadeY, c2.b * 0.4 * fadeY);
                        } else if (linkType < 0.6) {
                            // Contradicts: Red-tinted tension lines
                            crossConnPos.push(e1.x, e1.y, e1.z, e2.x, e2.y, e2.z);
                            crossConnCol.push(0.8 * fadeY, 0.1 * fadeY, 0.1 * fadeY);
                            crossConnCol.push(0.8 * fadeY, 0.1 * fadeY, 0.1 * fadeY);
                        } else {
                            // InspiredBy: Curved bezier path approximated by 2 segments
                            const mid = new THREE.Vector3().addVectors(e1, e2).multiplyScalar(0.5);
                            mid.x += (Math.random() - 0.5) * 0.8;
                            mid.z += (Math.random() - 0.5) * 0.8;
                            
                            crossConnPos.push(e1.x, e1.y, e1.z, mid.x, mid.y, mid.z);
                            crossConnCol.push(c1.r * 0.3 * fadeY, c1.g * 0.3 * fadeY, c1.b * 0.3 * fadeY);
                            crossConnCol.push(c2.r * 0.3 * fadeY, c2.g * 0.3 * fadeY, c2.b * 0.3 * fadeY);
                            
                            crossConnPos.push(mid.x, mid.y, mid.z, e2.x, e2.y, e2.z);
                            crossConnCol.push(c2.r * 0.3 * fadeY, c2.g * 0.3 * fadeY, c2.b * 0.3 * fadeY);
                            crossConnCol.push(c2.r * 0.3 * fadeY, c2.g * 0.3 * fadeY, c2.b * 0.3 * fadeY);
                        }
                    }
                }
            }
        }

        const c1 = getPrimaryFrame(0, y).C;
        const c2 = getPrimaryFrame(1, y).C;
        const center = new THREE.Vector3().addVectors(c1, c2).multiplyScalar(0.5);

        let isShared = false;
        if (isPrimary) isShared = Math.random() < 0.85; 
        else if (isSub) isShared = Math.random() < 0.35; 
        else isShared = Math.random() < 0.04; 

        if (isShared) {
            const allActive: number[] = [];
            for (let s = 0; s < 6; s++) {
                const entity = entities[s];
                if (entity.id === 'seraph') {
                    if (isPrimary && Math.abs(y) < 0.05) allActive.push(s);
                } else if (entity.id === 'quantum') {
                    if (isPrimary && Math.random() > 0.6) allActive.push(s);
                } else {
                    if (Math.random() > 0.4) allActive.push(s);
                }
            }

            if (allActive.length === 0) {
                allActive.push(Math.floor(Math.random() * 3));
                allActive.push(Math.floor(Math.random() * 3) + 3);
            } else if (allActive.length === 1 && allActive[0] !== 5) {
                const side = allActive[0] < 3 ? 1 : 0;
                allActive.push(Math.floor(Math.random() * 3) + (side * 3));
            }

            let r=0, g=0, b=0;
            allActive.forEach(s => {
                r += entityColors[s].r;
                g += entityColors[s].g;
                b += entityColors[s].b;
            });
            r /= allActive.length; g /= allActive.length; b /= allActive.length;
            
            const isGlowingWhite = Math.random() < 0.15;
            const stepR = (isGlowingWhite ? 1.0 : r) * fadeY;
            const stepG = (isGlowingWhite ? 1.0 : g) * fadeY;
            const stepB = (isGlowingWhite ? 1.0 : b) * fadeY;

            if (isPrimary) {
                pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
                pStepPos.push(c1.x, c1.y+0.025, c1.z, c2.x, c2.y+0.025, c2.z);
                pStepPos.push(c1.x, c1.y-0.025, c1.z, c2.x, c2.y-0.025, c2.z);
                pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
                pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
                pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
                
                stepHaloPos.push(center.x, center.y, center.z);
                stepHaloCol.push(stepR*0.5*fadeY, stepG*0.5*fadeY, stepB*0.5*fadeY);
            } else if (isSub) {
                pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
                pStepPos.push(c1.x, c1.y+0.01, c1.z, c2.x, c2.y+0.01, c2.z);
                pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
                pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
            } else {
                pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
                pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
            }

            allActive.forEach(s => {
                const E_pos = getEntityCenter(s, y).E;
                const c = entityColors[s];
                connPos.push(E_pos.x, E_pos.y, E_pos.z, center.x, center.y, center.z);
                
                let op = 0.5 * fadeY;
                if (entities[s].id === 'corso') op = 0.35 * fadeY;
                if (entities[s].id === 'quantum') op = 0.25 * fadeY;
                if (entities[s].id === 'seraph') op = 0.8 * fadeY;
                
                // Converges LinkType: Pulsing white hint toward shared node
                const convergeWhite = isGlowingWhite ? 0.8 * fadeY : 0;
                
                connCol.push(c.r*op + convergeWhite, c.g*op + convergeWhite, c.b*op + convergeWhite, 
                             1.0 * fadeY, 1.0 * fadeY, 1.0 * fadeY); 
            });

            pNodesPos.push(center.x, center.y, center.z);
            pNodesCol.push(stepR * 1.2 * fadeY, stepG * 1.2 * fadeY, stepB * 1.2 * fadeY);
            
            if (isPrimary && isGlowingWhite && fadeY > 0.1) {
                 // COMMUNITY CLUSTER ASSIGNMENT (Louvain community_id mock)
                 const communityColors = [
                     new THREE.Color(0xffffff), // Base white (no distinct community)
                     new THREE.Color(0x3B82F6), // Blue-ish
                     new THREE.Color(0x10B981), // Emerald
                     new THREE.Color(0x8B5CF6)  // Purple
                 ];
                 const communityId = Math.floor(Math.random() * communityColors.length);
                 const commColor = communityColors[communityId];

                 // Particle emission from convergence nodes
                 const pGeo = new THREE.BufferGeometry();
                 const pPos = new Float32Array(8 * 3);
                 const pAngles = new Float32Array(8);
                 const pRadii = new Float32Array(8);
                 for(let i=0; i<8; i++) {
                     pAngles[i] = Math.random() * Math.PI * 2;
                     pRadii[i] = Math.random() * 0.5;
                     pPos[i*3] = center.x; pPos[i*3+1] = center.y; pPos[i*3+2] = center.z;
                 }
                 pGeo.setAttribute('position', new THREE.BufferAttribute(pPos, 3));
                 
                 // Tint particles by community_id
                 const pMat = new THREE.PointsMaterial({
                     color: commColor, 
                     size: 0.03, 
                     transparent: true, 
                     opacity: 0.7 * fadeY, 
                     blending: THREE.AdditiveBlending, 
                     depthWrite: false, 
                     map: glowTexture
                 });
                 const pPoints = new THREE.Points(pGeo, pMat);
                 group.add(pPoints);
                 
                 convergenceNodes.push({ offset: Math.random() * Math.PI * 2, particles: pPoints, pAngles, pRadii, center: center.clone(), communityId });
            }
        } else {
            if (isPrimary) {
                pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
                pStepPos.push(c1.x, c1.y+0.025, c1.z, c2.x, c2.y+0.025, c2.z);
                pStepPos.push(c1.x, c1.y-0.025, c1.z, c2.x, c2.y-0.025, c2.z);
                pStepCol.push(0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY);
                pStepCol.push(0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY);
                pStepCol.push(0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY, 0.08*fadeY);
                
                stepHaloPos.push(center.x, center.y, center.z);
                stepHaloCol.push(0.05*fadeY, 0.05*fadeY, 0.05*fadeY);
            } else if (isSub) {
                pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
                pStepPos.push(c1.x, c1.y+0.01, c1.z, c2.x, c2.y+0.01, c2.z);
                pStepCol.push(0.05*fadeY, 0.05*fadeY, 0.05*fadeY, 0.05*fadeY, 0.05*fadeY, 0.05*fadeY);
                pStepCol.push(0.05*fadeY, 0.05*fadeY, 0.05*fadeY, 0.05*fadeY, 0.05*fadeY, 0.05*fadeY);
            } else if (isSubSub) {
                pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
                pStepCol.push(0.02*fadeY, 0.02*fadeY, 0.02*fadeY, 0.02*fadeY, 0.02*fadeY, 0.02*fadeY); 
            }
        }
    }

    group.add(makeLines(pStepPos, pStepCol, 0.15, 1)); 
    group.add(makeLines(mStepPos, mStepCol, 1.0, 1.0)); 
    group.add(makeLines(connPos, connCol, 1.0, 1.5)); 
    group.add(makeLines(crossConnPos, crossConnCol, 0.8, 0.7)); // New structural links
    group.add(makePoints(pNodesPos, pNodesCol, 0.12, 0.8)); 
    group.add(makePoints(stepHaloPos, stepHaloCol, 0.08, 0.4));

    // AGENT ORBS (Purposeful RAG Agents)
    const agentCount = 60; // Visible RAG agents traversing the helix strands
    const agentGeo = new THREE.BufferGeometry();
    const agentPos = new Float32Array(agentCount * 3);
    const agentColor = new Float32Array(agentCount * 3);
    const agentData: Array<{
      entityIdx: number;
      mIdx: number;
      y: number;
      speed: number;
      retrievalMode: string;
      state: string;
      stateTimer: number;
      targetNode: { center: THREE.Vector3 } | null;
      originNode: { center: THREE.Vector3 };
      color: THREE.Color;
      oldColor: THREE.Color;
      jumpStart: THREE.Vector3;
      jumpEnd: THREE.Vector3;
      jumpControl: THREE.Vector3;
    }> = [];

    for(let i=0; i<agentCount; i++) {
        // Start agents at random convergence nodes (fallback to origin if none exist)
        let startNode = { center: new THREE.Vector3(0, 0, 0) };
        if (convergenceNodes.length > 0) {
            const startNodeIdx = Math.floor(Math.random() * convergenceNodes.length);
            startNode = convergenceNodes[startNodeIdx];
        }
        
        // Assign to a random entity and sub-strand
        const entityIdx = Math.floor(Math.random() * 6);
        const entity = entities[entityIdx];
        const mIdx = Math.floor(Math.random() * entity.strands);
        
        // Find the Y position of the start node to begin traversal
        // Approximate Y by searching the entity path
        const y = tMin + Math.random() * (tMax - tMin);
        
        // Retrieval Mode logic based on entity entries count
        let retrievalMode = 'Balanced';
        if (entity.entries < 20) retrievalMode = 'KeywordDominated';
        else if (entity.entries > 150) retrievalMode = 'GraphWeighted';
        
        const baseSpeed = (1.5 + Math.random() * 2.5) * (Math.random() > 0.5 ? 1 : -1); 
        const speed = retrievalMode === 'KeywordDominated' ? baseSpeed * 0.3 : 
                      retrievalMode === 'GraphWeighted' ? baseSpeed * 1.8 : baseSpeed;
        
        agentData.push({ 
            entityIdx, 
            mIdx, 
            y, 
            speed,
            retrievalMode,
            state: 'TRAVERSE', // Skip SPAWN to ensure instant even distribution
            stateTimer: 0,
            targetNode: null,
            originNode: startNode,
            color: entityColors[entityIdx].clone(),
            oldColor: new THREE.Color(),
            jumpStart: new THREE.Vector3(),
            jumpEnd: new THREE.Vector3(),
            jumpControl: new THREE.Vector3()
        });
        
        // Initialize position at origin node
        agentPos[i*3] = startNode.center.x;
        agentPos[i*3+1] = startNode.center.y;
        agentPos[i*3+2] = startNode.center.z;
        
        // Agents start bright white during SPAWN
        agentColor[i*3] = 1.0;
        agentColor[i*3+1] = 1.0;
        agentColor[i*3+2] = 1.0;
    }
    agentGeo.setAttribute('position', new THREE.BufferAttribute(agentPos, 3));
    agentGeo.setAttribute('color', new THREE.BufferAttribute(agentColor, 3));
    
    const agentMat = new THREE.PointsMaterial({
        size: 0.18, // Visible RAG agents traversing the strands
        sizeAttenuation: true,
        map: glowTexture,
        transparent: true,
        opacity: 1.0,
        vertexColors: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        depthTest: false, // Always visible through helix structure
    });
    const agentSystem = new THREE.Points(agentGeo, agentMat);
    group.add(agentSystem);

    // RETRIEVAL LINES removed — flashing tracers were visually distracting

    // QUERY STREAM removed — vertical tracers were visually distracting

    // --- ANIMATION SYSTEM ---
    let pointerX = 0;
    let pointerY = 0;

    const cameraControl = {
      targetX: 0,
      targetY: 0,
      targetZ: 5.5,        // Default camera Z (matches camera.position.set(0, 0, 5.5))
      targetRotX: 0,
      targetRotZ: 0,
      lookAt: new THREE.Vector3(0, 0, 0),
      lerpSpeed: 0.03,
      scrollLocked: false,
    };

    const handleMouseMove = (event: MouseEvent) => {
      pointerX = event.clientX / window.innerWidth;
      pointerY = event.clientY / window.innerHeight;
      if (!cameraControl.scrollLocked) {
        cameraControl.targetX = (pointerX - 0.5) * 2.0;
        cameraControl.targetY = -(pointerY - 0.5) * 2.0;
        cameraControl.targetRotX = (pointerY - 0.5) * 0.15;
        cameraControl.targetRotZ = (pointerX - 0.5) * 0.15;
      }
    };
    window.addEventListener('mousemove', handleMouseMove);

    // INTERACTION LAYER (raycaster + state machine + fly-in/out)
    renderer.domElement.style.pointerEvents = 'auto';
    const interaction = new HelixInteraction(
      camera,
      renderer.domElement,
      cameraControl,
      polytopeManager,
      () => group.position.y,  // Live group Y for bob-compensated camera targeting
    );
    // Wire focus callbacks — no canvas-level opacity (causes dark box).
    // The Three.js setGlobalOpacityScale in update() handles fading non-focused elements.
    interaction.setOnFocus((project) => {
      onFocus?.(project);
    });
    interaction.setOnBlogFocus((post) => {
      onBlogFocus?.(post);
    });
    if (onHover) interaction.setOnHover(onHover);
    if (navigateRef) navigateRef.current = (id: string) => interaction.navigateToProject(id);
    if (unfocusRef) unfocusRef.current = () => interaction.unfocus();
    // Expose highlight for carousel arrows (no zoom, just glow)
    if (highlightRef) highlightRef.current = (id: string) => interaction.highlightProject(id);

    const clock = new THREE.Clock();
    let animationFrameId: number;
    let currentGroupY = -10.0;

    const animate = () => {
      animationFrameId = requestAnimationFrame(animate);
      const time = clock.getElapsedTime();

      // Helix rotation — pauses smoothly when a polytope is focused
      const focusIntensity = interaction.getFocusIntensity();
      const spinRate = 0.0006 * (1.0 - focusIntensity * 0.95); // 95% slowdown at full focus
      group.rotation.y += spinRate;

      // Dim competing elements (dust, agents) when focused
      fineDustMat.opacity = 0.25 * (1.0 - focusIntensity * 0.7);
      bokehMat.opacity = 0.05 * (1.0 - focusIntensity * 0.8);
      agentMat.opacity = 0.95 * (1.0 - focusIntensity * 0.8);

      dustGroup.rotation.y -= 0.0002 * (1.0 - focusIntensity * 0.9);
      dustGroup.rotation.x += 0.0001 * (1.0 - focusIntensity * 0.9);
      
      // Make the background nebula bokeh drift up slowly
      const bokehPos = bokehSystem.geometry.attributes.position.array;
      for(let i=0; i<bokehCount; i++) {
         bokehPos[i*3+1] += 0.001; 
         if(bokehPos[i*3+1] > 10) bokehPos[i*3+1] = -10;
      }
      bokehSystem.geometry.attributes.position.needsUpdate = true;

      // Purposeful RAG Agent Animation
      const aPos = agentSystem.geometry.attributes.position.array;
      const aCol = agentSystem.geometry.attributes.color.array;
      
      for(let i=0; i<agentCount; i++) {
          const ad = agentData[i];
          const dt = 0.016; // Approx delta time
          
          if (ad.state === 'SPAWN') {
              ad.stateTimer += dt;
              
              // White flash fading into entity color over 0.3s
              const mix = Math.min(ad.stateTimer / 0.3, 1.0);
              aCol[i*3] = 1.0 * (1-mix) + ad.color.r * mix;
              aCol[i*3+1] = 1.0 * (1-mix) + ad.color.g * mix;
              aCol[i*3+2] = 1.0 * (1-mix) + ad.color.b * mix;
              
              if (ad.stateTimer >= 0.3) {
                  ad.state = 'TRAVERSE';
                  ad.stateTimer = 0;
                  
                  // Pick a random target node to hit eventually
                  if (convergenceNodes.length > 0) {
                      ad.targetNode = convergenceNodes[Math.floor(Math.random() * convergenceNodes.length)];
                  }
              }
          }
          else if (ad.state === 'TRAVERSE') {
              // Accelerate/Decelerate simulating search
              // Very basic sine wave speed variation
              const currentSpeed = ad.speed * (0.5 + Math.sin(time * 5 + i) * 0.5);
              ad.y += currentSpeed * 0.01; 
              
              if (ad.y > tMax) ad.y = tMin;
              if (ad.y < tMin) ad.y = tMax;
              
              const fadeY = getFade(ad.y);
              
              // Get position on sub-strand
              const pos = getMiniAnchorFrame(ad.entityIdx, ad.mIdx, ad.y).M;
              aPos[i*3] = pos.x;
              aPos[i*3+1] = pos.y;
              aPos[i*3+2] = pos.z;
              
              // Base color with fade, slightly boosted to keep them prominent against translucent background
              aCol[i*3] = Math.min(ad.color.r * 1.3, 1.0) * fadeY;
              aCol[i*3+1] = Math.min(ad.color.g * 1.3, 1.0) * fadeY;
              aCol[i*3+2] = Math.min(ad.color.b * 1.3, 1.0) * fadeY;
              
              // Chance to hit a node (simulate finding context)
              const hitChance = ad.retrievalMode === 'KeywordDominated' ? 0.002 :
                                ad.retrievalMode === 'GraphWeighted' ? 0.015 : 0.005;
              
              if (Math.random() < hitChance) {
                  ad.state = 'HIT';
                  ad.stateTimer = 0;
                  
                  // Retrieval lines removed
              }
          }
          else if (ad.state === 'HIT') {
              // Pause at node for 0.2 - 0.5 seconds
              ad.stateTimer += dt;
              
              // Flash bright while collecting context
              const fadeY = getFade(ad.y);
              aCol[i*3] = Math.min(ad.color.r * 1.5, 1.0) * fadeY;
              aCol[i*3+1] = Math.min(ad.color.g * 1.5, 1.0) * fadeY;
              aCol[i*3+2] = Math.min(ad.color.b * 1.5, 1.0) * fadeY;
              
              if (ad.stateTimer > 0.3) {
                  // Jump strand or continue based on RetrievalMode
                  const jumpChance = ad.retrievalMode === 'KeywordDominated' ? 0.0 :
                                     ad.retrievalMode === 'GraphWeighted' ? 0.8 : 0.3;
                  
                  if (Math.random() < jumpChance) {
                      ad.state = 'JUMP';
                      ad.stateTimer = 0;
                      
                      ad.oldColor.copy(ad.color);
                      ad.jumpStart.set(aPos[i*3], aPos[i*3+1], aPos[i*3+2]);
                      
                      ad.entityIdx = Math.floor(Math.random() * 6);
                      const newEntity = entities[ad.entityIdx];
                      ad.color = entityColors[ad.entityIdx].clone();
                      ad.mIdx = Math.floor(Math.random() * newEntity.strands);
                      
                      const endFrame = getMiniAnchorFrame(ad.entityIdx, ad.mIdx, ad.y);
                      ad.jumpEnd.copy(endFrame.M);
                      
                      // Pull control point to the center step core to simulate traversing across internal rungs
                      ad.jumpControl.set(0, ad.y, 0);
                      
                      // Tracer arcs removed
                  } else {
                      ad.state = 'TRAVERSE';
                  }
              }
          }
          else if (ad.state === 'JUMP') {
              ad.stateTimer += dt;
              const jumpDuration = 0.4;
              const progress = Math.min(ad.stateTimer / jumpDuration, 1.0);
              
              const inv = 1 - progress;
              const curY = inv * inv * ad.jumpStart.y + 2 * inv * progress * ad.jumpControl.y + progress * progress * ad.jumpEnd.y;
              const fadeY = getFade(curY);
              
              aPos[i*3] = inv * inv * ad.jumpStart.x + 2 * inv * progress * ad.jumpControl.x + progress * progress * ad.jumpEnd.x;
              aPos[i*3+1] = curY;
              aPos[i*3+2] = inv * inv * ad.jumpStart.z + 2 * inv * progress * ad.jumpControl.z + progress * progress * ad.jumpEnd.z;
              
              aCol[i*3] = (ad.oldColor.r * inv + ad.color.r * progress) * fadeY;
              aCol[i*3+1] = (ad.oldColor.g * inv + ad.color.g * progress) * fadeY;
              aCol[i*3+2] = (ad.oldColor.b * inv + ad.color.b * progress) * fadeY;
              
              if (progress >= 1.0) {
                  ad.state = 'TRAVERSE';
              }
          }
      }
      agentSystem.geometry.attributes.position.needsUpdate = true;
      agentSystem.geometry.attributes.color.needsUpdate = true;
      
      // 4D polytope + interaction updates
      // Polytopes fade in as user scrolls into the Explore section and beyond
      const polytopeVisibility = Math.max(0, Math.min(1, (scrollPercent - 0.4) * 4));
      polytopeManager.setGlobalVisibility(polytopeVisibility);
      polytopeManager.update(time, camera);
      interaction.update();

      // Scroll down the helix gracefully — extended range for blog section below Explore
      if (!cameraControl.scrollLocked) {
        const targetGroupY = -10.0 + (scrollPercent * 35.0);
        currentGroupY += (targetGroupY - currentGroupY) * 0.025; // Silkier scroll glide
      }

      const bob = Math.sin(time * (Math.PI * 2 / 10)) * 0.1; // Gentler, slower breathing
      group.position.y = currentGroupY + bob;
      // Outer polytopes sync Y scroll + bob but don't rotate with the helix
      outerPolytopeGroup.position.y = currentGroupY + bob;

      convergenceNodes.forEach(node => {
         const positions = node.particles.geometry.attributes.position.array;
         for(let i=0; i<8; i++) {
             node.pRadii[i] += 0.002;
             if (node.pRadii[i] > 0.5) {
                 node.pRadii[i] = 0;
                 node.pAngles[i] = Math.random() * Math.PI * 2;
             }
             const r = node.pRadii[i];
             const a = node.pAngles[i];
             positions[i*3] = node.center.x + r * Math.cos(a);
             positions[i*3+1] = node.center.y + r * Math.sin(time + a) * 0.5;
             positions[i*3+2] = node.center.z + r * Math.sin(a);
         }
         node.particles.geometry.attributes.position.needsUpdate = true;
      });

      group.rotation.x += (cameraControl.targetRotX - group.rotation.x) * 0.03;
      group.rotation.z += (cameraControl.targetRotZ - group.rotation.z) * 0.03;

      camera.position.x += (cameraControl.targetX - camera.position.x) * cameraControl.lerpSpeed;
      camera.position.y += (cameraControl.targetY - camera.position.y) * cameraControl.lerpSpeed;
      camera.position.z += (cameraControl.targetZ - camera.position.z) * cameraControl.lerpSpeed;
      camera.lookAt(cameraControl.lookAt);

      composer.render();
    };

    animate();

    const handleResize = () => {
      if (!mountRef.current) return;
      const w = mountRef.current.clientWidth;
      const h = mountRef.current.clientHeight;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
      composer.setSize(w, h); 
    };
    
    const resizeObserver = new ResizeObserver(() => handleResize());
    if (mountRef.current) {
      resizeObserver.observe(mountRef.current);
    }
    window.addEventListener('resize', handleResize);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener('resize', handleResize);
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('scroll', handleScroll);
      cancelAnimationFrame(animationFrameId);
      if (mountRef.current && renderer.domElement) {
        mountRef.current.removeChild(renderer.domElement);
      }
      interaction.dispose();
      polytopeManager.dispose();
      renderer.dispose();
      glowTexture.dispose();
      fineDustGeom.dispose();
      fineDustMat.dispose();
      bokehGeom.dispose();
      bokehMat.dispose();
      agentGeo.dispose();
      agentMat.dispose();
      // linesGeo/streamGeo removed (retrieval lines + query stream)
    };
  }, []);

  return (
    <div className="relative w-full h-full z-0 pointer-events-auto opacity-90 overflow-hidden bg-[#0a0a0f]">
      <div ref={mountRef} className="absolute inset-0 w-full h-full" />
    </div>
  );
};
