<script lang="ts">
  import type { ProjectGroup } from '$lib/types';

  interface Props {
    groups: ProjectGroup[];
    selectedGroup?: ProjectGroup | null;
    width: number;
    height: number;
  }

  let { groups, selectedGroup = null, width, height }: Props = $props();

  let canvas: HTMLCanvasElement | undefined = $state();

  // Project cluster screen positions — callers project 3D → 2D and pass in
  // For now we derive approximate positions from the ring layout
  function clusterScreenPos(gi: number, total: number, w: number, h: number): { x: number; y: number } {
    const angle = (gi / total) * Math.PI * 2 - Math.PI / 2;
    const rx = w * 0.18;
    const ry = h * 0.22;
    return {
      x: w / 2 + rx * Math.cos(angle),
      y: h / 2 + ry * Math.sin(angle),
    };
  }

  $effect(() => {
    if (!canvas) return;
    if (width === 0 || height === 0) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.clearRect(0, 0, width, height);

    // ── Graph-paper grid ─────────────────────────────────────────────────

    const GRID_SMALL = 20;
    const GRID_LARGE = 100;

    ctx.strokeStyle = 'rgba(30,41,59,0.5)';
    ctx.lineWidth = 0.5;
    for (let x = 0; x <= width; x += GRID_SMALL) {
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, height); ctx.stroke();
    }
    for (let y = 0; y <= height; y += GRID_SMALL) {
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(width, y); ctx.stroke();
    }
    // Major grid lines
    ctx.strokeStyle = 'rgba(30,41,59,0.9)';
    ctx.lineWidth = 0.8;
    for (let x = 0; x <= width; x += GRID_LARGE) {
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, height); ctx.stroke();
    }
    for (let y = 0; y <= height; y += GRID_LARGE) {
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(width, y); ctx.stroke();
    }

    if (groups.length === 0) {
      ctx.fillStyle = 'rgba(71,85,105,0.5)';
      ctx.font = '10px monospace';
      ctx.textAlign = 'center';
      ctx.fillText('NO PROJECTS', width / 2, height / 2);
      return;
    }

    // ── Project labels with leader lines ─────────────────────────────────

    const clamped = groups.slice(0, 8);
    clamped.forEach((group, gi) => {
      const pos = clusterScreenPos(gi, clamped.length, width, height);
      const isSelected = selectedGroup?.id === group.id;

      // Leader line anchor — offset label away from center
      const cx = width / 2;
      const cy = height / 2;
      const dx = pos.x - cx;
      const dy = pos.y - cy;
      const len = Math.sqrt(dx * dx + dy * dy);
      const nx = len > 0 ? dx / len : 0;
      const ny = len > 0 ? dy / len : 0;
      const labelX = pos.x + nx * 28;
      const labelY = pos.y + ny * 24;

      // Auto-flip label anchor to avoid viewport edge
      const labelRight = labelX > width / 2;

      // Leader line
      ctx.beginPath();
      ctx.moveTo(pos.x, pos.y);
      ctx.lineTo(labelX, labelY);
      ctx.strokeStyle = isSelected ? 'rgba(255,215,0,0.7)' : 'rgba(30,41,59,0.8)';
      ctx.lineWidth = isSelected ? 1.2 : 0.7;
      ctx.stroke();

      // Elbow segment
      const elbowX = labelRight ? labelX + 18 : labelX - 18;
      ctx.beginPath();
      ctx.moveTo(labelX, labelY);
      ctx.lineTo(elbowX, labelY);
      ctx.stroke();

      // Index badge
      ctx.fillStyle = isSelected ? '#FFD700' : 'rgba(100,116,139,0.7)';
      ctx.font = `bold 8px monospace`;
      ctx.textAlign = 'center';
      ctx.fillText(`P${String(gi + 1).padStart(2, '0')}`, labelX, labelY - 4);

      // Project name
      ctx.fillStyle = isSelected ? '#e2e8f0' : 'rgba(100,116,139,0.85)';
      ctx.font = '9px monospace';
      ctx.textAlign = labelRight ? 'left' : 'right';
      ctx.fillText(
        group.name.slice(0, 16).toUpperCase(),
        labelRight ? elbowX + 3 : elbowX - 3,
        labelY + 1,
      );

      // Active build count badge
      if (group.activePlanCount > 0) {
        ctx.fillStyle = 'rgba(34,197,94,0.8)';
        ctx.font = '7px monospace';
        ctx.textAlign = labelRight ? 'left' : 'right';
        ctx.fillText(
          `${group.activePlanCount} ACTIVE`,
          labelRight ? elbowX + 3 : elbowX - 3,
          labelY + 11,
        );
      }

      // Cross-section hatch on selected cluster
      if (isSelected) {
        const hw = 22;
        ctx.save();
        ctx.translate(pos.x, pos.y);
        ctx.strokeStyle = 'rgba(255,215,0,0.15)';
        ctx.lineWidth = 0.6;
        for (let i = -hw; i <= hw; i += 5) {
          ctx.beginPath(); ctx.moveTo(i, -hw); ctx.lineTo(i + hw, hw); ctx.stroke();
        }
        ctx.restore();
      }
    });

    // ── Compass rose (bottom-right) ────────────────────────────────────

    const cr = { x: width - 28, y: height - 28 };
    const arms = [
      { dx: 0, dy: -12, label: 'N' },
      { dx: 12, dy: 0,  label: 'E' },
      { dx: 0, dy: 12,  label: 'S' },
      { dx: -12, dy: 0, label: 'W' },
    ];
    arms.forEach(arm => {
      ctx.beginPath();
      ctx.moveTo(cr.x, cr.y);
      ctx.lineTo(cr.x + arm.dx, cr.y + arm.dy);
      ctx.strokeStyle = 'rgba(71,85,105,0.6)';
      ctx.lineWidth = 0.8;
      ctx.stroke();
      ctx.fillStyle = 'rgba(71,85,105,0.7)';
      ctx.font = '6px monospace';
      ctx.textAlign = 'center';
      ctx.fillText(arm.label, cr.x + arm.dx * 1.5, cr.y + arm.dy * 1.5 + 2);
    });

    // ── Scale bar (bottom-left) ───────────────────────────────────────

    const sb = { x: 18, y: height - 16 };
    ctx.beginPath();
    ctx.moveTo(sb.x, sb.y);
    ctx.lineTo(sb.x + 40, sb.y);
    ctx.strokeStyle = 'rgba(71,85,105,0.6)';
    ctx.lineWidth = 0.8;
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(sb.x, sb.y - 3);
    ctx.lineTo(sb.x, sb.y + 3);
    ctx.moveTo(sb.x + 40, sb.y - 3);
    ctx.lineTo(sb.x + 40, sb.y + 3);
    ctx.stroke();
    ctx.fillStyle = 'rgba(71,85,105,0.7)';
    ctx.font = '6px monospace';
    ctx.textAlign = 'center';
    ctx.fillText('10 UNITS', sb.x + 20, sb.y - 5);
  });
</script>

<canvas
  bind:this={canvas}
  {width}
  {height}
  class="absolute inset-0 pointer-events-none"
  style="z-index: 2;"
  data-testid="blueprint-hud"
></canvas>
