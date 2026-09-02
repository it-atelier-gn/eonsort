<script lang="ts">
  import { onDestroy } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { thumbnailFor, type EntryView } from "$lib/api";
  import { MODES, MODE_EXPLAINER, MODE_LABEL, type Mode } from "$lib/viz/layouts";
  import { READINGS, READING_COLOUR, READING_LABEL, READING_NOTE } from "$lib/viz/charts";
  import { createScene, type Tile } from "$lib/viz/scene";
  import type { TimeAxis } from "$lib/viz/timeaxis";

  interface Props {
    entries: EntryView[];
    selected: EntryView | null;
    onSelect: (entry: EntryView) => void;
  }

  type Media =
    | { kind: "image"; url: string; ratio: number }
    | { kind: "video"; url: string }
    | { kind: "none" };

  const THUMBNAIL_EDGE = 512;
  const PLAY_SIZE = 0.16;
  const MAX_IN_FLIGHT = 6;

  let { entries, selected, onSelect }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let scene = $state<ReturnType<typeof createScene> | null>(null);
  let mode = $state<Mode>("field");
  let axis = $state<TimeAxis | null>(null);
  let failure = $state<string | null>(null);
  let explain = $state(true);
  let cruising = $state(false);
  let detail = $state(0);

  const DETAIL_STEPS = [
    {
      name: "Overview",
      note: "One dot per file, everything else muted. Only dates that look wrong keep their colour and their threads.",
    },
    {
      name: "Coming closer",
      note: "The other sources' readings are fading in, and the colours are coming back.",
    },
    {
      name: "Close up",
      note: "Every reading from every source, in full colour, and the files themselves on the nearest points.",
    },
  ];
  const detailStep = $derived(detail <= 0.02 ? 0 : detail >= 0.98 ? 2 : 1);
  let tiles = $state<Tile[]>([]);
  let media = $state(new Map<string, Media>());
  let inFlight = new Set<string>();

  let dragging = $state(false);
  let panning = $state(false);
  let moved = $state(0);
  let last = { x: 0, y: 0 };

  const flagged = $derived(entries.filter((e) => !e.override_origin && e.flags.some((f) => f.hard)));
  const shown = $derived(
    tiles
      .map((tile) => ({ tile, entry: entries[tile.entry] }))
      .filter((pair) => pair.entry !== undefined),
  );

  $effect(() => {
    if (!canvas || scene) return;
    try {
      scene = createScene(canvas, {
        onAxis: (next) => (axis = next),
        onTiles: (next) => (tiles = next),
        onFlight: (on) => (cruising = on),
        onDetail: (next) => (detail = next),
      });
    } catch (e) {
      failure = String(e);
    }
  });

  async function load(path: string) {
    inFlight.add(path);
    try {
      const found = await thumbnailFor(path, THUMBNAIL_EDGE);
      const next = new Map(media);
      if (found.kind === "image") {
        next.set(path, {
          kind: "image",
          url: `data:image/jpeg;base64,${found.data}`,
          ratio: found.height === 0 ? 1 : found.width / found.height,
        });
      } else if (found.kind === "playable") {
        next.set(path, { kind: "video", url: convertFileSrc(path) });
      } else {
        next.set(path, { kind: "none" });
      }
      media = next;
    } catch {
      media = new Map(media).set(path, { kind: "none" });
    } finally {
      inFlight.delete(path);
    }
  }

  $effect(() => {
    entries;
    media = new Map();
    inFlight.clear();
  });

  $effect(() => {
    const cached = media;
    for (const { entry } of shown) {
      if (inFlight.size >= MAX_IN_FLIGHT) break;
      if (cached.has(entry.source) || inFlight.has(entry.source)) continue;
      load(entry.source);
    }
  });

  $effect(() => {
    scene?.setEntries(entries);
  });

  $effect(() => {
    scene?.setMode(mode);
  });

  $effect(() => {
    const index = selected ? entries.findIndex((e) => e.source === selected.source) : -1;
    scene?.setSelected(index < 0 ? null : index);
  });

  onDestroy(() => scene?.dispose());

  function down(event: PointerEvent) {
    if (!canvas) return;
    canvas.setPointerCapture(event.pointerId);
    dragging = true;
    panning = event.button === 1 || event.shiftKey;
    moved = 0;
    last = { x: event.clientX, y: event.clientY };
  }

  function move(event: PointerEvent) {
    if (!dragging || !scene) return;
    const dx = event.clientX - last.x;
    const dy = event.clientY - last.y;
    last = { x: event.clientX, y: event.clientY };
    moved += Math.abs(dx) + Math.abs(dy);
    if (panning) scene.panBy(dx, dy);
    else scene.orbitBy(dx, dy);
  }

  function up(event: PointerEvent) {
    if (!canvas || !scene) return;
    canvas.releasePointerCapture(event.pointerId);
    dragging = false;

    if (moved > 4) return;
    const box = canvas.getBoundingClientRect();
    const hit = scene.pickAt(event.clientX - box.left, event.clientY - box.top);
    if (hit !== null && entries[hit]) onSelect(entries[hit]);
  }

  function wheel(event: WheelEvent) {
    event.preventDefault();
    scene?.zoomBy(event.deltaY);
  }
</script>

<div class="scape">
  {#if failure}
    <div class="fallback faint">
      <p>The timeline needs WebGL2, which this system did not provide.</p>
      <p class="mono small">{failure}</p>
    </div>
  {:else}
    <canvas
      bind:this={canvas}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onwheel={wheel}
    ></canvas>

    <div class="tiles">
      {#each shown as { tile, entry } (entry.source)}
        {@const found = media.get(entry.source)}
        {#if found && found.kind !== "none"}
          <button
            class="tile"
            class:current={selected?.source === entry.source}
            style:left="{tile.x * 100}%"
            style:top="{tile.y * 100}%"
            style:height="{tile.size * 100}%"
            style:aspect-ratio={found.kind === "image" ? found.ratio : 4 / 3}
            style:opacity={tile.fade}
            style:z-index={Math.max(0, Math.round(400 - tile.depth * 8))}
            title={`${entry.name} · ${entry.taken}`}
            onclick={() => onSelect(entry)}
          >
            {#if found.kind === "image"}
              <img src={found.url} alt={entry.name} draggable="false" />
            {:else if tile.size >= PLAY_SIZE}
              <video src={found.url} autoplay muted loop playsinline></video>
            {:else}
              <span class="film">▶</span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>

    <div class="modes">
      {#each MODES as option (option)}
        <button class:active={mode === option} onclick={() => (mode = option)}>
          {MODE_LABEL[option]}
        </button>
      {/each}
      <button
        class:active={cruising}
        title="Fly through the whole archive at speed"
        onclick={() => scene?.cruise(!cruising)}
      >
        {cruising ? "Stop flight" : "Auto-fly"}
      </button>
      <button class="ghost" onclick={() => scene?.reset()}>Recentre</button>
    </div>

    <div class="side">
      <div class="card explain">
        <button class="head" onclick={() => (explain = !explain)}>
          <strong>{MODE_LABEL[mode]}</strong>
          <span class="faint">{explain ? "−" : "?"}</span>
        </button>
        {#if explain}
          <p>{MODE_EXPLAINER[mode].shows}</p>
          <p class="axes">{MODE_EXPLAINER[mode].axes}</p>
          <p><em>What to look for:</em> {MODE_EXPLAINER[mode].look}</p>
        {/if}
      </div>

      <div class="card detail">
        <div class="rungs">
          {#each DETAIL_STEPS as level, index (level.name)}
            <i class:on={index <= detailStep} class:now={index === detailStep}></i>
          {/each}
        </div>
        <strong>{DETAIL_STEPS[detailStep].name}</strong>
        <p>{DETAIL_STEPS[detailStep].note}</p>
        <span class="faint tiny">Scroll to change how much detail is shown.</span>
      </div>

      <div class="card legend">
        <span class="title faint">Colour = how sure the date is</span>
        {#each READINGS as reading (reading)}
          <span title={READING_NOTE[reading]}>
            <i class="dot" style:background={READING_COLOUR[reading]}></i>{READING_LABEL[reading]}
          </span>
        {/each}
        <span class="title faint">Lines join one file's readings</span>
      </div>
    </div>

    <div class="readout faint">
      {#if entries.length === 0}
        Run a scan to fill the timeline.
      {:else}
        {entries.length} files · {flagged.length} with a date that looks wrong
        {#if axis && axis.breaks.length > 0}
          · {axis.breaks.length} empty {axis.breaks.length === 1 ? "stretch" : "stretches"} compressed
        {/if}
      {/if}
    </div>

    <p class="hint faint">
      Drag to orbit · shift-drag to pan · wheel to zoom · click a point to open it · zoom in far
      enough and the points become the pictures themselves · Auto-fly cruises the whole archive
      (any drag takes the controls back)
    </p>
  {/if}
</div>

<style>
  .scape {
    position: relative;
    height: 100%;
    background: #08090f;
    overflow: hidden;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    touch-action: none;
    cursor: grab;
  }

  canvas:active {
    cursor: grabbing;
  }

  .tiles {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .tile {
    position: absolute;
    transform: translate(-50%, -50%);
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.22);
    border-radius: 2px;
    background: rgba(8, 9, 15, 0.6);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.55);
    overflow: hidden;
    pointer-events: auto;
    cursor: pointer;
    line-height: 0;
  }

  .tile.current {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent);
  }

  .tile img,
  .tile video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .film {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.75);
    line-height: 1;
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

  .modes {
    position: absolute;
    z-index: 600;
    top: 10px;
    left: 10px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    max-width: calc(100% - 270px);
  }

  .modes button.active {
    border-color: var(--accent);
    background: var(--bg-active);
    color: var(--text);
  }

  .side {
    position: absolute;
    z-index: 600;
    top: 10px;
    right: 10px;
    width: 236px;
    display: grid;
    gap: 6px;
  }

  .card {
    font-size: 10px;
    color: var(--text-dim);
    background: rgba(13, 17, 23, 0.82);
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .explain {
    display: grid;
    gap: 5px;
  }

  .explain .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    gap: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
  }

  .explain p {
    margin: 0;
    line-height: 1.5;
  }

  .explain .axes {
    color: var(--text-faint);
  }

  .explain em {
    color: var(--text);
    font-style: normal;
  }

  .detail {
    display: grid;
    gap: 4px;
  }

  .detail strong {
    font-size: 11px;
    color: var(--text);
  }

  .detail p {
    margin: 0;
    line-height: 1.5;
  }

  .tiny {
    font-size: 9px;
  }

  .rungs {
    display: flex;
    gap: 3px;
  }

  .rungs i {
    flex: 1;
    height: 3px;
    border-radius: 2px;
    background: var(--bg-hover);
  }

  .rungs i.on {
    background: var(--accent-dim);
  }

  .rungs i.now {
    background: var(--accent);
  }

  .legend {
    display: grid;
    gap: 3px;
  }

  .legend span {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .legend .title {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-top: 2px;
  }

  .legend .title:first-child {
    margin-top: 0;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }





  .readout {
    position: absolute;
    z-index: 600;
    left: 10px;
    bottom: 26px;
    font-size: 11px;
  }

  .hint {
    position: absolute;
    z-index: 600;
    left: 10px;
    bottom: 8px;
    font-size: 10px;
  }
</style>
