<script lang="ts">
  import { activeSkin, vaultCounts } from '$lib/stores';
  import {
    DEFAULT_SKIN, PRESET_SKINS, exportSkin, importSkin, hashColor,
    type HelixSkin,
  } from '$lib/helix-skin';

  /** Canonical fallback when vaultCounts hasn't loaded yet. */
  const CANONICAL_SIBLINGS = ['eva', 'corso', 'quantum', 'seraph', 'larc', 'ayin'];

  // --- Panel open/close ---
  let open = $state(false);

  // --- Tabs ---
  type Tab = 'colors' | 'glow' | 'atmosphere' | 'rails';
  let tab = $state<Tab>('colors');
  const tabs: { id: Tab; label: string }[] = [
    { id: 'colors', label: 'Colors' },
    { id: 'glow', label: 'Glow' },
    { id: 'atmosphere', label: 'Atmosphere' },
    { id: 'rails', label: 'Rails' },
  ];

  // --- Derived sibling list ---
  let siblingIds = $derived(
    $vaultCounts ? Object.keys($vaultCounts) : CANONICAL_SIBLINGS,
  );

  // --- Colors tab: custom sibling input ---
  let newSiblingId = $state('');
  let newSiblingColor = $state('#888888');

  function addCustomSibling() {
    const id = newSiblingId.trim().toLowerCase();
    if (!id) return;
    activeSkin.update(s => ({
      ...s,
      colors: { ...s.colors, [id]: newSiblingColor },
    }));
    newSiblingId = '';
    newSiblingColor = '#888888';
  }

  /** Resolve color for a sibling — skin override, then hash fallback. */
  function resolveColor(siblingId: string): string {
    const c = $activeSkin.colors[siblingId];
    if (c) return c;
    return hashColor(siblingId);
  }

  function setColor(siblingId: string, color: string) {
    activeSkin.update(s => ({
      ...s,
      colors: { ...s.colors, [siblingId]: color },
    }));
  }

  // --- Glow helpers ---
  type GlowKey = keyof HelixSkin['glow'];
  const glowSliders: { key: GlowKey; label: string; min: number; max: number; step: number; group: string }[] = [
    { key: 'bloomStrength',  label: 'Strength',    min: 0, max: 3,    step: 0.05, group: 'Bloom' },
    { key: 'bloomRadius',    label: 'Radius',      min: 0, max: 2,    step: 0.05, group: 'Bloom' },
    { key: 'bloomThreshold', label: 'Threshold',   min: 0, max: 1,    step: 0.01, group: 'Bloom' },
    { key: 'dustOpacity',    label: 'Fine opacity', min: 0, max: 1,   step: 0.01, group: 'Dust' },
    { key: 'bokehOpacity',   label: 'Bokeh opacity', min: 0, max: 0.3, step: 0.005, group: 'Dust' },
    { key: 'bokehSize',      label: 'Bokeh size',  min: 0.05, max: 0.5, step: 0.01, group: 'Dust' },
    { key: 'strandOpacity',  label: 'Opacity',     min: 0, max: 1,    step: 0.01, group: 'Strands' },
    { key: 'nodeGlow',       label: 'Node glow',   min: 0, max: 1,    step: 0.01, group: 'Strands' },
    { key: 'fogDensity',     label: 'Density',     min: 0, max: 0.15, step: 0.005, group: 'Fog' },
  ];

  function setGlow(key: GlowKey, value: number) {
    activeSkin.update(s => ({ ...s, glow: { ...s.glow, [key]: value } }));
  }

  // --- Rails helpers ---
  type RailKey = keyof HelixSkin['rails'];
  const railSliders: { key: RailKey; label: string; min: number; max: number; step: number; isColor?: boolean }[] = [
    { key: 'railOpacity',       label: 'Rail opacity',      min: 0,   max: 1,   step: 0.01 },
    { key: 'railColor',         label: 'Rail color',         min: 0,   max: 0,   step: 0,   isColor: true },
    { key: 'crossRungOpacity',  label: 'Cross-rung opacity', min: 0,   max: 0.2, step: 0.005 },
    { key: 'crossRungColor',    label: 'Cross-rung color',   min: 0,   max: 0,   step: 0,   isColor: true },
    { key: 'strandBrightness',  label: 'Strand brightness',  min: 0.5, max: 2.0, step: 0.05 },
    { key: 'nodeSizeScale',     label: 'Node size scale',    min: 0.5, max: 2.0, step: 0.05 },
    { key: 'haloOpacity',       label: 'Halo opacity',       min: 0,   max: 1,   step: 0.01 },
  ];

  function setRail(key: RailKey, value: string | number) {
    activeSkin.update(s => ({ ...s, rails: { ...s.rails, [key]: value } }));
  }

  // --- Atmosphere helpers ---
  function setAtmosphere<K extends keyof HelixSkin['atmosphere']>(key: K, value: HelixSkin['atmosphere'][K]) {
    activeSkin.update(s => ({ ...s, atmosphere: { ...s.atmosphere, [key]: value } }));
  }

  function addPaletteColor(field: 'dustPalette' | 'bokehPalette', color: string) {
    activeSkin.update(s => ({
      ...s,
      atmosphere: { ...s.atmosphere, [field]: [...s.atmosphere[field], color] },
    }));
  }

  function removePaletteColor(field: 'dustPalette' | 'bokehPalette', index: number) {
    activeSkin.update(s => ({
      ...s,
      atmosphere: {
        ...s.atmosphere,
        [field]: s.atmosphere[field].filter((_, i) => i !== index),
      },
    }));
  }

  let newDustColor = $state('#ffffff');
  let newBokehColor = $state('#ffffff');

  // --- Presets ---
  function applyPreset(preset: HelixSkin) {
    activeSkin.set(structuredClone(preset));
  }

  function resetSkin() {
    activeSkin.set(structuredClone(DEFAULT_SKIN));
  }

  // --- Export ---
  function doExport() {
    const json = exportSkin($activeSkin);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${$activeSkin.name.toLowerCase().replace(/\s+/g, '-')}.helix-skin.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // --- Import ---
  let fileInput: HTMLInputElement;

  function doImport(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const result = importSkin(reader.result as string);
      if (result) activeSkin.set(result);
    };
    reader.readAsText(file);
    input.value = '';
  }

  // --- Name editing ---
  function setName(name: string) {
    activeSkin.update(s => ({ ...s, name }));
  }

  // --- Group glow sliders by group label ---
  function groupedGlowSliders() {
    const groups: { group: string; items: typeof glowSliders }[] = [];
    let current: typeof groups[0] | null = null;
    for (const s of glowSliders) {
      if (!current || current.group !== s.group) {
        current = { group: s.group, items: [] };
        groups.push(current);
      }
      current.items.push(s);
    }
    return groups;
  }
</script>

<!-- Toggle button (always visible in top-right of helix container) -->
<button class="toggle-btn" class:open onclick={() => (open = !open)}>
  Skin
</button>

<!-- Slide-out panel -->
<div class="panel" class:open>
  <!-- Skin name -->
  <div class="skin-name-row">
    <input
      class="skin-name-input"
      type="text"
      value={$activeSkin.name}
      oninput={(e) => setName((e.target as HTMLInputElement).value)}
      placeholder="Skin name"
    />
  </div>

  <!-- Tab bar -->
  <div class="tab-bar">
    {#each tabs as t}
      <button
        class="tab-btn"
        class:active={tab === t.id}
        onclick={() => (tab = t.id)}
      >{t.label}</button>
    {/each}
  </div>

  <!-- Tab content -->
  <div class="tab-content">
    <!-- ═══ Colors ═══ -->
    {#if tab === 'colors'}
      <div class="tab-scroll">
        {#each siblingIds as id}
          <div class="color-row">
            <span class="color-badge" style:background={resolveColor(id)}></span>
            <span class="color-label">{id}</span>
            <input
              type="color"
              class="color-picker"
              value={resolveColor(id)}
              oninput={(e) => setColor(id, (e.target as HTMLInputElement).value)}
            />
          </div>
        {/each}

        <!-- Custom siblings already in skin but not in vaultCounts -->
        {#each Object.keys($activeSkin.colors).filter(k => !siblingIds.includes(k)) as id}
          <div class="color-row">
            <span class="color-badge" style:background={$activeSkin.colors[id]}></span>
            <span class="color-label">{id}</span>
            <input
              type="color"
              class="color-picker"
              value={$activeSkin.colors[id]}
              oninput={(e) => setColor(id, (e.target as HTMLInputElement).value)}
            />
          </div>
        {/each}

        <div class="add-custom-row">
          <input
            class="add-custom-input"
            type="text"
            bind:value={newSiblingId}
            placeholder="agent id"
          />
          <input
            type="color"
            class="color-picker"
            bind:value={newSiblingColor}
          />
          <button class="add-custom-btn" onclick={addCustomSibling}>+</button>
        </div>
      </div>

    <!-- ═══ Glow ═══ -->
    {:else if tab === 'glow'}
      <div class="tab-scroll">
        {#each groupedGlowSliders() as group}
          <div class="slider-group-label">{group.group}</div>
          {#each group.items as s}
            <div class="slider-row">
              <span class="slider-label">{s.label}</span>
              <input
                type="range"
                class="slider"
                min={s.min}
                max={s.max}
                step={s.step}
                value={$activeSkin.glow[s.key]}
                oninput={(e) => setGlow(s.key, parseFloat((e.target as HTMLInputElement).value))}
              />
              <span class="slider-value">{$activeSkin.glow[s.key].toFixed(2)}</span>
            </div>
          {/each}
        {/each}
      </div>

    <!-- ═══ Atmosphere ═══ -->
    {:else if tab === 'atmosphere'}
      <div class="tab-scroll">
        <div class="slider-group-label">Background</div>
        <div class="color-row">
          <span class="color-label">Color</span>
          <input
            type="color"
            class="color-picker"
            value={$activeSkin.atmosphere.backgroundColor}
            oninput={(e) => setAtmosphere('backgroundColor', (e.target as HTMLInputElement).value)}
          />
        </div>

        <div class="slider-group-label">Ambient Light</div>
        <div class="color-row">
          <span class="color-label">Color</span>
          <input
            type="color"
            class="color-picker"
            value={$activeSkin.atmosphere.ambientLightColor}
            oninput={(e) => setAtmosphere('ambientLightColor', (e.target as HTMLInputElement).value)}
          />
        </div>
        <div class="slider-row">
          <span class="slider-label">Intensity</span>
          <input
            type="range"
            class="slider"
            min={0}
            max={1}
            step={0.01}
            value={$activeSkin.atmosphere.ambientLightIntensity}
            oninput={(e) => setAtmosphere('ambientLightIntensity', parseFloat((e.target as HTMLInputElement).value))}
          />
          <span class="slider-value">{$activeSkin.atmosphere.ambientLightIntensity.toFixed(2)}</span>
        </div>

        <div class="slider-group-label">Dust Palette</div>
        <div class="palette-row">
          {#each $activeSkin.atmosphere.dustPalette as color, i}
            <div class="palette-swatch-wrap">
              <span class="palette-swatch" style:background={color}></span>
              <button class="palette-remove" onclick={() => removePaletteColor('dustPalette', i)}>x</button>
            </div>
          {/each}
          <div class="palette-add">
            <input type="color" class="color-picker" bind:value={newDustColor} />
            <button class="add-custom-btn" onclick={() => { addPaletteColor('dustPalette', newDustColor); }}>+</button>
          </div>
        </div>

        <div class="slider-group-label">Bokeh Palette</div>
        <div class="palette-row">
          {#each $activeSkin.atmosphere.bokehPalette as color, i}
            <div class="palette-swatch-wrap">
              <span class="palette-swatch" style:background={color}></span>
              <button class="palette-remove" onclick={() => removePaletteColor('bokehPalette', i)}>x</button>
            </div>
          {/each}
          <div class="palette-add">
            <input type="color" class="color-picker" bind:value={newBokehColor} />
            <button class="add-custom-btn" onclick={() => { addPaletteColor('bokehPalette', newBokehColor); }}>+</button>
          </div>
        </div>
      </div>

    <!-- ═══ Rails ═══ -->
    {:else if tab === 'rails'}
      <div class="tab-scroll">
        {#each railSliders as s}
          {#if s.isColor}
            <div class="color-row">
              <span class="color-label">{s.label}</span>
              <input
                type="color"
                class="color-picker"
                value={$activeSkin.rails[s.key] as string}
                oninput={(e) => setRail(s.key, (e.target as HTMLInputElement).value)}
              />
            </div>
          {:else}
            <div class="slider-row">
              <span class="slider-label">{s.label}</span>
              <input
                type="range"
                class="slider"
                min={s.min}
                max={s.max}
                step={s.step}
                value={$activeSkin.rails[s.key] as number}
                oninput={(e) => setRail(s.key, parseFloat((e.target as HTMLInputElement).value))}
              />
              <span class="slider-value">{($activeSkin.rails[s.key] as number).toFixed(2)}</span>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  <!-- Bottom bar -->
  <div class="bottom-bar">
    <div class="preset-row">
      {#each PRESET_SKINS as preset}
        <button
          class="preset-btn"
          class:active={$activeSkin.id === preset.id}
          onclick={() => applyPreset(preset)}
        >{preset.name}</button>
      {/each}
    </div>
    <div class="action-row">
      <button class="action-btn" onclick={doExport}>Export</button>
      <button class="action-btn" onclick={() => fileInput.click()}>Import</button>
      <button class="action-btn reset" onclick={resetSkin}>Reset</button>
      <input
        bind:this={fileInput}
        type="file"
        accept=".json"
        class="hidden-file"
        onchange={doImport}
      />
    </div>
  </div>
</div>

<style>
  /* ── Toggle button ─────────────────────────────────────────────────────── */
  .toggle-btn {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 50;
    background: rgba(13, 17, 23, 0.85);
    border: 1px solid #1e293b;
    color: #64748b;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    padding: 4px 10px;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .toggle-btn:hover,
  .toggle-btn.open {
    color: #FFD700;
    border-color: #FFD700;
  }

  /* ── Panel ──────────────────────────────────────────────────────────────── */
  .panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 240px;
    z-index: 45;
    background: rgba(13, 17, 23, 0.92);
    border-left: 1px solid #1e293b;
    display: flex;
    flex-direction: column;
    transform: translateX(100%);
    transition: transform 300ms ease;
    overflow: hidden;
  }
  .panel.open {
    transform: translateX(0);
  }

  /* ── Skin name ─────────────────────────────────────────────────────────── */
  .skin-name-row {
    padding: 32px 10px 4px;
  }
  .skin-name-input {
    width: 100%;
    background: transparent;
    border: none;
    border-bottom: 1px solid #1e293b;
    color: #e2e8f0;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 12px;
    font-weight: 600;
    padding: 2px 0;
    outline: none;
  }
  .skin-name-input:focus {
    border-bottom-color: #FFD700;
  }

  /* ── Tab bar ────────────────────────────────────────────────────────────── */
  .tab-bar {
    display: flex;
    gap: 0;
    border-bottom: 1px solid #1e293b;
    padding: 0 6px;
  }
  .tab-btn {
    flex: 1;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: #475569;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 6px 2px;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .tab-btn:hover {
    color: #94a3b8;
  }
  .tab-btn.active {
    color: #FFD700;
    border-bottom-color: #FFD700;
  }

  /* ── Tab content ────────────────────────────────────────────────────────── */
  .tab-content {
    flex: 1;
    overflow: hidden;
  }
  .tab-scroll {
    height: 100%;
    overflow-y: auto;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tab-scroll::-webkit-scrollbar {
    width: 3px;
  }
  .tab-scroll::-webkit-scrollbar-thumb {
    background: #1e293b;
    border-radius: 2px;
  }

  /* ── Color rows ─────────────────────────────────────────────────────────── */
  .color-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 24px;
  }
  .color-badge {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .color-label {
    flex: 1;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    color: #94a3b8;
    text-transform: lowercase;
  }
  .color-picker {
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid #1e293b;
    border-radius: 3px;
    background: none;
    cursor: pointer;
    flex-shrink: 0;
  }
  .color-picker::-webkit-color-swatch-wrapper {
    padding: 1px;
  }
  .color-picker::-webkit-color-swatch {
    border: none;
    border-radius: 2px;
  }

  /* ── Add custom ─────────────────────────────────────────────────────────── */
  .add-custom-row {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid #1e293b;
  }
  .add-custom-input {
    flex: 1;
    background: #0d1117;
    border: 1px solid #1e293b;
    color: #94a3b8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    padding: 3px 6px;
    border-radius: 3px;
    outline: none;
  }
  .add-custom-input:focus {
    border-color: #334155;
  }
  .add-custom-btn {
    background: #1e293b;
    border: 1px solid #334155;
    color: #94a3b8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    width: 24px;
    height: 24px;
    border-radius: 3px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .add-custom-btn:hover {
    color: #FFD700;
    border-color: #FFD700;
  }

  /* ── Slider rows ────────────────────────────────────────────────────────── */
  .slider-group-label {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    color: #475569;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin-top: 6px;
    margin-bottom: 2px;
  }
  .slider-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 20px;
  }
  .slider-label {
    width: 72px;
    flex-shrink: 0;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    color: #94a3b8;
  }
  .slider {
    flex: 1;
    height: 4px;
    accent-color: #FFD700;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    background: #1e293b;
    border-radius: 2px;
    outline: none;
  }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #FFD700;
    cursor: pointer;
  }
  .slider::-moz-range-thumb {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #FFD700;
    border: none;
    cursor: pointer;
  }
  .slider-value {
    width: 32px;
    text-align: right;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    color: #475569;
    flex-shrink: 0;
  }

  /* ── Palette rows ───────────────────────────────────────────────────────── */
  .palette-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    margin-bottom: 4px;
  }
  .palette-swatch-wrap {
    position: relative;
  }
  .palette-swatch {
    display: block;
    width: 20px;
    height: 20px;
    border-radius: 3px;
    border: 1px solid #1e293b;
  }
  .palette-remove {
    position: absolute;
    top: -4px;
    right: -4px;
    width: 12px;
    height: 12px;
    background: #0d1117;
    border: 1px solid #334155;
    border-radius: 50%;
    color: #64748b;
    font-size: 7px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .palette-swatch-wrap:hover .palette-remove {
    opacity: 1;
  }
  .palette-remove:hover {
    color: #ef4444;
    border-color: #ef4444;
  }
  .palette-add {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  /* ── Bottom bar ─────────────────────────────────────────────────────────── */
  .bottom-bar {
    border-top: 1px solid #1e293b;
    padding: 6px 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
  }
  .preset-row {
    display: flex;
    gap: 3px;
    flex-wrap: wrap;
  }
  .preset-btn {
    background: #1e293b;
    border: 1px solid #334155;
    color: #64748b;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 3px 6px;
    border-radius: 3px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .preset-btn:hover {
    color: #94a3b8;
  }
  .preset-btn.active {
    border-color: #FFD700;
    color: #FFD700;
    background: rgba(255, 215, 0, 0.06);
  }
  .action-row {
    display: flex;
    gap: 4px;
  }
  .action-btn {
    flex: 1;
    background: #0d1117;
    border: 1px solid #1e293b;
    color: #64748b;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 0;
    border-radius: 3px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .action-btn:hover {
    color: #94a3b8;
    border-color: #334155;
  }
  .action-btn.reset:hover {
    color: #ef4444;
    border-color: #ef4444;
  }
  .hidden-file {
    display: none;
  }
</style>
