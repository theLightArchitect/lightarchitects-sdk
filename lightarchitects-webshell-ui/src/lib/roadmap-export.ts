// Origin: scope-bleed from radiant-weaving-phoenix; landed via squishy-tome (commit 42bb840) merge. Phoenix is the canonical owner.
// ============================================================================
// roadmap-export.ts — Generates a self-contained roadmap HTML from Build[]
// Replaces sync-roadmap.sh with a frontend-native export.
//
// Security note: All user-supplied strings are escaped via esc() before
// being interpolated into the HTML template. The BUILD_DATA JS object uses
// jsStr() to escape for JS string literals. The generated HTML is a static
// standalone file — no server-side rendering, no eval, no dynamic injection.
// The detail panel uses createElement/textContent (not innerHTML) to render
// BUILD_DATA safely at runtime.
// ============================================================================

import type { Build } from './types';

// ── Column classification (mirrors KanbanBoard) ────────────────────────────

interface RoadmapColumn {
  key: string;
  label: string;
  color: string;
  builds: Build[];
}

function classifyBuilds(builds: Build[]): RoadmapColumn[] {
  const cols: RoadmapColumn[] = [
    { key: 'queued',      label: 'Planned',     color: '#64748b', builds: [] },
    { key: 'in_progress', label: 'In Progress', color: '#22c55e', builds: [] },
    { key: 'paused',      label: 'Blocked',     color: '#f59e0b', builds: [] },
    { key: 'completed',   label: 'Completed',   color: '#3b82f6', builds: [] },
    { key: 'failed',      label: 'Failed',      color: '#ef4444', builds: [] },
  ];
  const byKey = new Map(cols.map(c => [c.key, c]));
  for (const b of builds) {
    const col = byKey.get(b.status);
    if (col) col.builds.push(b);
    else byKey.get('queued')!.builds.push(b);
  }
  // Sort within columns: priority high->med->low, then alphabetical
  const prioOrder: Record<string, number> = { high: 0, medium: 1, low: 2 };
  for (const col of cols) {
    col.builds.sort((a, b) => {
      const pa = prioOrder[a.priority ?? ''] ?? 3;
      const pb = prioOrder[b.priority ?? ''] ?? 3;
      if (pa !== pb) return pa - pb;
      return a.name.localeCompare(b.name);
    });
  }
  return cols;
}

// ── HTML helpers ───────────────────────────────────────────────────────────

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function jsStr(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n').replace(/\r/g, '');
}

function tierColor(tier: number | undefined, status: string): string {
  if (status === 'completed') return '#4ade80';
  switch (tier) {
    case 1: return '#fb923c';
    case 2: return '#60a5fa';
    case 3: return '#a78bfa';
    default: return '#475569';
  }
}

function tierLabel(tier: number | undefined): string {
  switch (tier) {
    case 1: return 'SMALL';
    case 2: return 'MEDIUM';
    case 3: return 'LARGE';
    default: return tier != null ? `TIER ${tier}` : '';
  }
}

const SIBLING_CSS_COLORS: Record<string, string> = {
  soul: '#7C3AED', eva: '#FF1493', corso: '#00BFFF',
  quantum: '#B44AFF', seraph: '#FF0040', ayin: '#FF6D00',
  laex: '#F59E0B', berean: '#c4b5fd',
};

function siblingColor(s: string): string {
  return SIBLING_CSS_COLORS[s.toLowerCase()] ?? '#8B5CF6';
}

// ── Card HTML ──────────────────────────────────────────────────────────────

function renderCard(b: Build): string {
  const tc = tierColor(b.tier, b.status);
  const completed = b.status === 'completed';
  const pillars = b.pillars ?? [];
  const passed = pillars.filter(p => p.status === 'passed').length;

  const sibTags = (b.siblings ?? []).slice(0, 4).map(s =>
    `<span class="tag-sib" style="color:${siblingColor(s)};border-color:${siblingColor(s)}40">${esc(s.toUpperCase())}</span>`
  ).join('');

  const prioTag = b.priority
    ? `<span class="tag-prio" style="color:${b.priority === 'high' ? '#ef4444' : b.priority === 'medium' ? '#f59e0b' : '#22c55e'}">P${b.priority === 'high' ? '1' : b.priority === 'medium' ? '2' : '3'}</span>`
    : '';

  const desc = b.description
    ? `<div class="card-desc">${esc(b.description.length > 180 ? b.description.slice(0, 177) + '...' : b.description)}</div>`
    : '';

  const blockedBy = (b.blockedBy ?? []).length > 0
    ? `<div class="card-blocked">\u26D4 blocked by ${esc(b.blockedBy!.join(', '))}</div>`
    : '';

  const tl = tierLabel(b.tier);
  const tierTag = tl ? `<span class="tag-tier" style="color:${tc}">${esc(tl)}</span>` : '';

  const phaseDots = pillars.map(p => {
    const c = p.status === 'passed' ? '#22c55e' : p.status === 'in_progress' ? '#FFD700' : p.status === 'failed' ? '#ef4444' : '#1e293b';
    return `<span class="phase-dot" style="background:${c}" title="${esc(p.pillar)}: ${p.status}"></span>`;
  }).join('');

  return `<div class="card${completed ? ' completed' : ''}" data-build="${esc(b.id)}">
  <div class="tier-stripe" style="background:${tc};box-shadow:0 0 8px ${tc}40"></div>
  <div class="card-body">
    <div class="card-row1">
      <span class="card-name${completed ? ' strike' : ''}">${esc(b.name)}</span>
      ${prioTag}
    </div>
    ${desc}
    <div class="card-meta">
      <div class="phase-bar">${phaseDots}<span class="phase-frac">${passed}/${pillars.length}</span></div>
      <div class="card-tags">${tierTag}${sibTags}</div>
    </div>
    ${blockedBy}
  </div>
</div>`;
}

// ── BUILD_DATA JS object ───────────────────────────────────────────────────

function renderBuildData(builds: Build[]): string {
  const entries = builds.map(b => {
    const sibs = JSON.stringify(b.siblings ?? []);
    const blocked = JSON.stringify(b.blockedBy ?? []);
    const blocks = JSON.stringify(b.blocks ?? []);
    return `  '${jsStr(b.id)}': {
    name: '${jsStr(b.name)}', tier: ${b.tier ?? 0}, status: '${jsStr(b.status)}',
    siblings: ${sibs}, description: '${jsStr(b.description ?? '')}',
    dependencies: { blockedBy: ${blocked}, blocks: ${blocks} },
    confidence: ${b.confidence}, metaSkill: '${jsStr(b.metaSkill)}'
  }`;
  });
  return `var BUILD_DATA = {\n${entries.join(',\n')}\n};`;
}

// ── Full HTML document ─────────────────────────────────────────────────────

export function generateRoadmapHTML(builds: Build[]): string {
  const columns = classifyBuilds(builds);
  const total = builds.length;
  const done = columns.find(c => c.key === 'completed')?.builds.length ?? 0;
  const active = columns.find(c => c.key === 'in_progress')?.builds.length ?? 0;
  const progressPct = total > 0 ? Math.round((done / total) * 100) : 0;
  const now = new Date().toISOString().slice(0, 19).replace('T', ' ');

  const columnHTML = columns.map(col => `
  <div class="column">
    <div class="col-header" style="border-bottom-color:${col.color}40">
      <h2>${col.label}</h2>
      <span class="count" style="color:${col.color};background:${col.color}15">${col.builds.length}</span>
    </div>
    ${col.builds.length === 0
      ? '<div class="empty-state">No builds</div>'
      : col.builds.map(renderCard).join('\n    ')}
  </div>`).join('\n');

  const statBar = columns.map(c =>
    `${c.label}: <span class="sv" style="color:${c.color}">${c.builds.length}</span>`
  ).join(' &bull; ');

  // The detail panel JS uses safe DOM methods (createElement/textContent)
  // instead of innerHTML to prevent XSS when rendering BUILD_DATA fields.
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Light Architects - Roadmap</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#050508;color:#e2e8f0;font-family:'Inter','Segoe UI',system-ui,sans-serif;min-height:100vh;overflow-x:hidden}
a{color:#f0c040;text-decoration:none}

/* Header */
.header{position:sticky;top:0;z-index:10;display:flex;align-items:center;justify-content:space-between;padding:12px 24px;background:rgba(5,5,8,0.85);backdrop-filter:blur(24px) saturate(1.4);border-bottom:1px solid rgba(240,192,64,0.1)}
.header h1{font-size:18px;font-weight:700;letter-spacing:1px}
.header .meta{font-size:11px;color:#94a3b8}
.header .meta span{margin:0 8px}

/* Board */
.board{display:flex;gap:14px;padding:20px 24px;overflow-x:auto;min-height:calc(100vh - 120px)}
.column{flex:1;min-width:240px;display:flex;flex-direction:column}
.col-header{display:flex;align-items:center;justify-content:space-between;padding:10px 12px;margin-bottom:10px;background:rgba(18,18,30,0.4);backdrop-filter:blur(12px);border:1px solid rgba(42,42,58,0.4);border-bottom:2px solid;border-radius:10px 10px 0 0}
.col-header h2{font-size:13px;font-weight:600}
.count{font-size:10px;font-family:monospace;padding:2px 8px;border-radius:99px}

/* Card */
.card{position:relative;margin-bottom:10px;border-radius:12px;overflow:hidden;background:rgba(18,18,30,0.55);backdrop-filter:blur(20px) saturate(1.2);border:1px solid rgba(42,42,58,0.6);cursor:pointer;transition:border-color .2s,box-shadow .2s,transform .2s}
.card:hover{border-color:#FFD70060;box-shadow:0 4px 20px rgba(0,0,0,0.3);transform:translateY(-1px)}
.card.completed{opacity:0.45}
.card.completed:hover{opacity:0.8}
.tier-stripe{position:absolute;left:0;top:0;bottom:0;width:3px;transition:width .2s}
.card:hover .tier-stripe{width:4px}
.card-body{padding:10px 12px 10px 14px}
.card-row1{display:flex;align-items:center;justify-content:space-between;gap:6px}
.card-name{font-size:12px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.card-name.strike{text-decoration:line-through;text-decoration-color:#4ade80}
.card-desc{font-size:10px;color:#64748b;margin-top:4px;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;line-height:1.4}
.card-meta{display:flex;align-items:center;justify-content:space-between;margin-top:6px;gap:4px}
.card-tags{display:flex;gap:3px;flex-wrap:wrap}
.card-blocked{font-size:8px;color:#ef4444;margin-top:4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}

/* Tags */
.tag-sib{font-size:7px;padding:1px 4px;border-radius:3px;font-family:monospace;text-transform:uppercase;border:1px solid}
.tag-prio{font-size:8px;font-family:monospace;font-weight:700;padding:1px 4px;border-radius:3px;background:rgba(0,0,0,0.3)}
.tag-tier{font-size:8px;font-family:monospace;padding:1px 4px;border-radius:3px;background:rgba(0,0,0,0.2)}

/* Phase dots */
.phase-bar{display:flex;align-items:center;gap:2px}
.phase-dot{width:6px;height:6px;border-radius:2px;flex-shrink:0}
.phase-frac{font-size:8px;color:#475569;font-family:monospace;margin-left:3px}

/* Empty state */
.empty-state{text-align:center;padding:32px 12px;font-size:10px;color:#475569;border:1px dashed #1e293b;border-radius:8px}

/* Summary bar */
.summary{position:fixed;bottom:0;left:0;right:0;z-index:10;display:flex;align-items:center;justify-content:space-between;padding:8px 24px;background:rgba(5,5,8,0.9);backdrop-filter:blur(24px);border-top:1px solid rgba(240,192,64,0.08);font-size:10px;color:#94a3b8}
.sv{font-family:monospace;font-weight:700}
.progress-track{width:100%;height:3px;background:#1e293b;border-radius:2px;overflow:hidden;position:fixed;bottom:36px;left:0;right:0;z-index:11}
.progress-fill{height:100%;background:linear-gradient(90deg,#ef4444,#f59e0b,#22c55e);border-radius:2px;transition:width .6s}

/* Particle canvas */
#particles{position:fixed;top:0;left:0;width:100%;height:100%;z-index:0;pointer-events:none}
.board,.header,.summary{position:relative;z-index:1}

/* Detail panel */
#detailBackdrop{display:none;position:fixed;inset:0;background:rgba(0,0,0,0.5);z-index:50}
#detailBackdrop.open{display:block}
#detailPanel{position:fixed;top:0;right:0;bottom:0;width:min(420px,90vw);background:rgba(10,10,18,0.95);backdrop-filter:blur(32px);border-left:1px solid rgba(240,192,64,0.1);z-index:51;transform:translateX(100%);transition:transform .4s cubic-bezier(.4,0,.2,1);overflow-y:auto;padding:24px}
#detailBackdrop.open #detailPanel{transform:translateX(0)}
.dp-close{position:absolute;top:12px;right:16px;background:none;border:none;color:#94a3b8;font-size:20px;cursor:pointer}
.dp-close:hover{color:#FFD700}
.dp-title{font-size:16px;font-weight:700;margin-bottom:4px}
.dp-meta{font-size:10px;color:#64748b;margin-bottom:12px}
.dp-section{margin-top:16px}
.dp-section h3{font-size:11px;color:#94a3b8;text-transform:uppercase;letter-spacing:1px;margin-bottom:6px}
.dp-desc{font-size:12px;color:#cbd5e1;line-height:1.5}
</style>
</head>
<body>

<canvas id="particles"></canvas>

<div class="header">
  <h1>LIGHT ARCHITECTS &mdash; ROADMAP</h1>
  <div class="meta">
    <span>Updated: ${esc(now.slice(0, 10))}</span>
    <span>Active: ${total} builds</span>
    <span>Completed: ${done}</span>
  </div>
</div>

<div class="board">
${columnHTML}
</div>

<div class="progress-track"><div class="progress-fill" style="width:${progressPct}%"></div></div>
<div class="summary">
  <div>${statBar}</div>
  <div>Generated by <strong>Light Architects Webshell</strong> &mdash; ${esc(now)}</div>
</div>

<div id="detailBackdrop">
  <div id="detailPanel">
    <button class="dp-close">&times;</button>
    <div id="dp-content"></div>
  </div>
</div>

<script>
${renderBuildData(builds)}

// Detail panel — uses safe DOM methods (createElement/textContent)
function el(tag, text, style) {
  var e = document.createElement(tag);
  if (text) e.textContent = text;
  if (style) e.setAttribute('style', style);
  return e;
}
function openDetail(id) {
  var d = BUILD_DATA[id];
  if (!d) return;
  var c = document.getElementById('dp-content');
  c.replaceChildren();
  c.appendChild(el('div', d.name, 'font-size:16px;font-weight:700;margin-bottom:4px'));
  c.appendChild(el('div', d.status + ' | ' + d.metaSkill + ' | ' + Math.round(d.confidence*100) + '% confidence', 'font-size:10px;color:#64748b;margin-bottom:12px'));
  if (d.description) {
    var s = document.createElement('div'); s.style.marginTop='16px';
    s.appendChild(el('h3', 'Description', 'font-size:11px;color:#94a3b8;text-transform:uppercase;letter-spacing:1px;margin-bottom:6px'));
    s.appendChild(el('div', d.description, 'font-size:12px;color:#cbd5e1;line-height:1.5'));
    c.appendChild(s);
  }
  if (d.siblings.length) {
    var s2 = document.createElement('div'); s2.style.marginTop='16px';
    s2.appendChild(el('h3', 'Siblings', 'font-size:11px;color:#94a3b8;text-transform:uppercase;letter-spacing:1px;margin-bottom:6px'));
    s2.appendChild(el('div', d.siblings.join(', '), 'font-size:12px;color:#cbd5e1'));
    c.appendChild(s2);
  }
  if (d.dependencies.blockedBy.length) {
    var s3 = document.createElement('div'); s3.style.marginTop='16px';
    s3.appendChild(el('h3', 'Blocked By', 'font-size:11px;color:#94a3b8;text-transform:uppercase;letter-spacing:1px;margin-bottom:6px'));
    s3.appendChild(el('div', d.dependencies.blockedBy.join(', '), 'font-size:12px;color:#ef4444'));
    c.appendChild(s3);
  }
  document.getElementById('detailBackdrop').className = 'open';
}
function closeDetail() { document.getElementById('detailBackdrop').className = ''; }

document.getElementById('detailBackdrop').addEventListener('click', function(e) { if (e.target === this) closeDetail(); });
document.querySelector('.dp-close').addEventListener('click', closeDetail);
document.addEventListener('click', function(e) {
  var card = e.target.closest('.card[data-build]');
  if (card) openDetail(card.dataset.build);
});
document.addEventListener('keydown', function(e) { if (e.key === 'Escape') closeDetail(); });

// 3D card tilt
document.querySelectorAll('.card').forEach(function(card) {
  card.addEventListener('mousemove', function(e) {
    var r = card.getBoundingClientRect();
    var x = e.clientX - r.left, y = e.clientY - r.top;
    var rx = ((y - r.height/2) / (r.height/2)) * -3;
    var ry = ((x - r.width/2) / (r.width/2)) * 3;
    card.style.transform = 'perspective(800px) rotateX('+rx+'deg) rotateY('+ry+'deg) scale(1.01)';
  });
  card.addEventListener('mouseleave', function() {
    card.style.transform = 'perspective(800px) rotateX(0) rotateY(0) scale(1)';
    card.style.transition = 'transform 0.4s ease, border-color 0.2s, box-shadow 0.2s';
  });
  card.addEventListener('mouseenter', function() { card.style.transition = 'border-color 0.2s, box-shadow 0.2s'; });
});

// Particle canvas
(function() {
  var c = document.getElementById('particles'), ctx = c.getContext('2d');
  var particles = [], mouse = {x:-1, y:-1};
  var COLORS = ['#f0c040','#7C3AED','#FF1493','#00BFFF','#B44AFF','#FF6D00'];
  function hex2rgba(hex, a) {
    var r = parseInt(hex.slice(1,3),16), g = parseInt(hex.slice(3,5),16), b = parseInt(hex.slice(5,7),16);
    return 'rgba('+r+','+g+','+b+','+a+')';
  }
  function resize() {
    c.width = window.innerWidth; c.height = window.innerHeight;
    particles = [];
    var count = Math.floor((c.width * c.height) / 18000);
    for (var i = 0; i < count; i++) {
      particles.push({
        x: Math.random()*c.width, y: Math.random()*c.height,
        vx: (Math.random()-0.5)*0.15, vy: (Math.random()-0.5)*0.15,
        r: 0.3+Math.random()*1.2, a: 0.05+Math.random()*0.3,
        color: COLORS[Math.floor(Math.random()*COLORS.length)]
      });
    }
  }
  resize();
  window.addEventListener('resize', resize);
  document.addEventListener('mousemove', function(e) { mouse.x=e.clientX; mouse.y=e.clientY; });
  (function draw() {
    ctx.clearRect(0,0,c.width,c.height);
    for (var i=0;i<particles.length;i++) {
      var p=particles[i]; p.x+=p.vx; p.y+=p.vy;
      if(p.x<0)p.x=c.width; if(p.x>c.width)p.x=0;
      if(p.y<0)p.y=c.height; if(p.y>c.height)p.y=0;
      var dx=mouse.x-p.x, dy=mouse.y-p.y, dist=Math.sqrt(dx*dx+dy*dy);
      var scale = dist<200 ? 1+(200-dist)/200*2 : 1;
      var alpha = dist<200 ? p.a+0.2 : p.a;
      ctx.beginPath(); ctx.arc(p.x,p.y,p.r*scale,0,Math.PI*2);
      ctx.fillStyle = hex2rgba(p.color, alpha);
      ctx.fill();
    }
    requestAnimationFrame(draw);
  })();
})();
</script>
</body>
</html>`;
}

// ── Public API ──────────────────────────────────────────────────────────────

/**
 * Triggers a browser download of the roadmap HTML.
 */
export function downloadRoadmap(builds: Build[], filename = 'roadmap.html'): void {
  const html = generateRoadmapHTML(builds);
  const blob = new Blob([html], { type: 'text/html' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * Copies the roadmap HTML to clipboard.
 */
export async function copyRoadmapToClipboard(builds: Build[]): Promise<void> {
  const html = generateRoadmapHTML(builds);
  await navigator.clipboard.writeText(html);
}
