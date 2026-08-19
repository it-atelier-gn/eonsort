<script lang="ts">
  import { onDestroy } from "svelte";
  import { thumbnailFor, type EntryView } from "$lib/api";
  import { createRings, type Label, type Look } from "$lib/viz/rings/scene";
  import {
    monthCss,
    MAX_FLY,
    MIN_FLY,
    MODE_LABEL,
    MONTH_NAMES,
    RING_MODES,
    type Ring,
    type RingMode,
  } from "$lib/viz/rings/layout";

  interface Props {
    entries: EntryView[];
    onSelect: (entry: EntryView) => void;
  }

  const THUMBNAIL_EDGE = 512;
  const MAX_IN_FLIGHT = 4;

  let { entries, onSelect }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let scene = $state<ReturnType<typeof createRings> | null>(null);
  let failure = $state<string | null>(null);
  let mode = $state<RingMode>("rings");
  let labels = $state<Label[]>([]);
  let hovered = $state<number | null>(null);
  let rings = $state<Ring[]>([]);
  let dragging = $state(false);
  let look = $state<Look>({
    height: 0,
    lowest: 0,
    highest: 0,
    tilt: 0.5,
    flying: false,
    speed: 1,
  });

  const loaded = new Set<number>();
  const inFlight = new Set<number>();
  let pending: number[] = [];
  let moved = false;

  const topFirst = $derived([...rings].sort((a, b) => b.year - a.year));
  const hoveredEntry = $derived(hovered === null ? null : (entries[hovered] ?? null));
  const months = $derived(new Set(entries.map((entry) => new Date(entry.taken_epoch * 1000).getUTCMonth())));

  $effect(() => {
    if (!canvas || scene) return;
    try {
      scene = createRings(canvas, {
        onHover: (entry) => (hovered = entry),
        onLook: (next) => (look = next),
        onNear: (near) => {
          pending = near;
          pump();
        },
        onLabels: (next) => (labels = next),
      });
    } catch (e) {
      failure = String(e);
    }
  });

  $effect(() => {
    if (!scene) return;
    loaded.clear();
    inFlight.clear();
    scene.setEntries(entries);
    rings = scene.rings().rings;
  });

  $effect(() => {
    scene?.setMode(mode);
  });

  async function pump() {
    if (!scene) return;
    for (const index of pending) {
      if (inFlight.size >= MAX_IN_FLIGHT) return;
      if (loaded.has(index) || inFlight.has(index)) continue;
      const entry = entries[index];
      if (!entry) continue;

      inFlight.add(index);
      try {
        const found = await thumbnailFor(entry.source, THUMBNAIL_EDGE, entry.rotate);
        loaded.add(index);
        if (found.kind === "image") {
          const image = new Image();
          image.src = `data:image/jpeg;base64,${found.data}`;
          await image.decode();
          scene?.setImage(index, image);
        }
      } catch {
        loaded.add(index);
      } finally {
        inFlight.delete(index);
      }
    }
  }

  function at(event: PointerEvent): { x: number; y: number } {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return {
      x: (event.clientX - box.left) / Math.max(1, box.width),
      y: (event.clientY - box.top) / Math.max(1, box.height),
    };
  }

  function down(event: PointerEvent) {
    dragging = true;
    moved = false;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function drag(event: PointerEvent) {
    if (!dragging) {
      scene?.setPointer(at(event));
      return;
    }
    if (Math.abs(event.movementX) + Math.abs(event.movementY) > 2) moved = true;
    if (event.shiftKey) scene?.raise(event.movementY);
    else scene?.turn(event.movementX, event.movementY);
  }

  function up(event: PointerEvent) {
    const element = event.currentTarget as HTMLElement;
    if (element.hasPointerCapture(event.pointerId)) element.releasePointerCapture(event.pointerId);
    dragging = false;
    if (moved) return;

    const point = at(event);
    const found = scene?.entryAt(point.x, point.y) ?? null;
    if (found !== null && entries[found]) onSelect(entries[found]);
  }

  function wheel(event: WheelEvent) {
    event.preventDefault();
    if (event.shiftKey) scene?.raise(-event.deltaY * 0.4);
    else scene?.zoom(event.deltaY);
  }

  function rise(event: Event) {
    scene?.setHeight(Number((event.currentTarget as HTMLInputElement).value));
  }

  function tilt(event: Event) {
    scene?.setTilt(Number((event.currentTarget as HTMLInputElement).value));
  }

  function pace(event: Event) {
    scene?.setSpeed(Number((event.currentTarget as HTMLInputElement).value));
  }

  onDestroy(() => scene?.dispose());
</script>

<div class="rings">
  {#if failure}
    <div class="fallback faint">
      <p>The rings need WebGL2, which this system did not provide.</p>
      <p class="mono small">{failure}</p>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <canvas
      bind:this={canvas}
      class:dragging
      onpointerdown={down}
      onpointermove={drag}
      onpointerup={up}
      onpointerleave={() => scene?.setPointer(null)}
      onwheel={wheel}
    ></canvas>

    {#each labels as label (label.year)}
      <span class="year" style="left: {label.x * 100}%; top: {label.y * 100}%">
        <strong>{label.year}</strong>
        <span class="faint">
          {label.shown < label.count
            ? `${label.shown.toLocaleString()} of ${label.count.toLocaleString()}`
            : label.count.toLocaleString()}
        </span>
      </span>
    {/each}

    <div class="modes">
      {#each RING_MODES as option (option)}
        <button class:active={mode === option} onclick={() => (mode = option)}>
          {MODE_LABEL[option]}
        </button>
      {/each}
    </div>

    <div class="rise">
      <span class="cap faint">up</span>
      <input
        type="range"
        min={look.lowest}
        max={look.highest}
        step="0.02"
        value={look.height}
        oninput={rise}
        aria-label="How high you are looking from"
        title="Slide to move your eye up and down the stack"
      />
      <span class="cap faint">down</span>
      <div class="years">
        {#each topFirst as ring (ring.year)}
          <button onclick={() => scene?.goTo(ring)} title="Look at {ring.year}">{ring.year}</button>
        {/each}
      </div>
    </div>

    <div class="drifts">
      <button
        class:active={look.flying}
        onclick={() => scene?.setFlying(!look.flying)}
        title="Let the stack turn by itself"
      >
        Turn
      </button>
      <input
        type="range"
        min={MIN_FLY}
        max={MAX_FLY}
        step="0.01"
        value={look.speed}
        oninput={pace}
        aria-label="How fast it drifts"
        title="How fast it drifts ({look.speed.toFixed(2)}×)"
      />
    </div>

    <div class="tilt">
      <span class="cap faint">tilt</span>
      <input
        type="range"
        min="0"
        max="1"
        step="0.005"
        value={look.tilt}
        oninput={tilt}
        aria-label="How steeply you are looking"
        title="Slide to look from above or from below"
      />
    </div>

    <div class="legend">
      {#each MONTH_NAMES as month, index (month)}
        <span class="month" class:absent={!months.has(index)}>
          <span class="swatch" style="background: {monthCss(index)}"></span>
          {month.slice(0, 3)}
        </span>
      {/each}
    </div>

    <div class="plaque">
      {#if entries.length === 0}
        <strong>Nothing to stack</strong>
        <span class="faint">Run a scan and the years pile up.</span>
      {:else if hoveredEntry}
        <strong class="truncate">{hoveredEntry.name}</strong>
        <span class="faint">{hoveredEntry.taken} · click to open it</span>
      {:else}
        <strong>
          {rings.length}
          {rings.length === 1 ? "year" : "years"} · {entries.length.toLocaleString()} files
        </strong>
        <span class="faint">drag to turn · sliders lift and tilt · Turn drifts on its own</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .rings {
    position: relative;
    height: 100%;
    background: #0a0c11;
    overflow: hidden;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
    touch-action: none;
  }

  canvas.dragging {
    cursor: grabbing;
  }

  .fallback {
    padding: 40px;
    font-size: 12px;
    display: grid;
    gap: 8px;
  }

  .small {
    font-size: 10px;
  }

  .year {
    position: absolute;
    transform: translate(-50%, -50%);
    display: flex;
    gap: 6px;
    align-items: baseline;
    padding: 2px 6px;
    border-radius: 3px;
    background: rgba(10, 12, 17, 0.6);
    font-size: 11px;
    color: #e8ecf5;
    pointer-events: none;
    white-space: nowrap;
  }

  .modes {
    position: absolute;
    top: 12px;
    left: 12px;
    display: flex;
    gap: 4px;
  }

  .modes button {
    font-size: 11px;
    padding: 4px 10px;
    background: rgba(10, 12, 17, 0.72);
    color: #cfd6e4;
    border: 1px solid rgba(160, 180, 220, 0.22);
    border-radius: 3px;
    cursor: pointer;
  }

  .modes button.active {
    color: #0a0c11;
    background: #cfd6e4;
  }

  .rise {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 8px;
    border-radius: 4px;
    background: rgba(10, 12, 17, 0.72);
    flex-direction: column;
  }

  .rise input {
    writing-mode: vertical-lr;
    direction: rtl;
    width: 16px;
    height: 180px;
    accent-color: #cfd6e4;
    cursor: ns-resize;
  }

  .cap {
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #8c96a8;
  }

  .years {
    position: absolute;
    right: 100%;
    margin-right: 6px;
    display: grid;
    gap: 2px;
    justify-items: end;
    max-height: 220px;
    overflow: auto;
  }

  .years button {
    padding: 1px 6px;
    font-size: 10px;
    background: rgba(10, 12, 17, 0.72);
    color: #cfd6e4;
    border: 1px solid rgba(160, 180, 220, 0.22);
    border-radius: 3px;
    cursor: pointer;
    white-space: nowrap;
  }

  .years button:hover {
    background: rgba(160, 180, 220, 0.3);
  }

  .drifts {
    position: absolute;
    top: 12px;
    left: 12px;
    margin-top: 30px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px;
    border-radius: 4px;
    background: rgba(10, 12, 17, 0.72);
  }

  .drifts button {
    font-size: 11px;
    padding: 3px 9px;
    background: transparent;
    color: #cfd6e4;
    border: 1px solid rgba(160, 180, 220, 0.22);
    border-radius: 3px;
    cursor: pointer;
  }

  .drifts button.active {
    color: #0a0c11;
    background: #cfd6e4;
  }

  .drifts input {
    width: 82px;
    accent-color: #cfd6e4;
    cursor: ew-resize;
  }

  .tilt {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 4px;
    background: rgba(10, 12, 17, 0.72);
  }

  .tilt input {
    width: 120px;
    accent-color: #cfd6e4;
    cursor: ew-resize;
  }

  .legend {
    position: absolute;
    top: 12px;
    right: 12px;
    display: grid;
    grid-template-columns: repeat(2, auto);
    gap: 2px 10px;
    padding: 8px 10px;
    border-radius: 4px;
    background: rgba(10, 12, 17, 0.72);
    font-size: 10px;
    color: #cfd6e4;
  }

  .month {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .month.absent {
    opacity: 0.32;
  }

  .swatch {
    width: 9px;
    height: 9px;
    border-radius: 2px;
  }

  .plaque {
    position: absolute;
    left: 12px;
    bottom: 12px;
    display: grid;
    gap: 2px;
    max-width: 60%;
    padding: 8px 12px;
    border-radius: 4px;
    background: rgba(10, 12, 17, 0.72);
    font-size: 12px;
    color: #e8ecf5;
  }

  .plaque .faint {
    font-size: 11px;
  }
</style>
