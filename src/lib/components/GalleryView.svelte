<script lang="ts">
  import { onDestroy } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { thumbnailFor, type EntryView } from "$lib/api";
  import { createGallery } from "$lib/viz/gallery/scene";
  import type { Room } from "$lib/viz/gallery";

  interface Props {
    entries: EntryView[];
    onSelect: (entry: EntryView) => void;
  }

  const THUMBNAIL_EDGE = 512;
  const MAX_IN_FLIGHT = 4;
  const MAX_VIDEOS = 4;

  let { entries, onSelect }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let gallery = $state<ReturnType<typeof createGallery> | null>(null);
  let failure = $state<string | null>(null);
  let walking = $state(false);
  let room = $state<Room | null>(null);
  let aimed = $state<number | null>(null);
  let hung = $state(0);

  const held = new Set<string>();
  const loaded = new Set<number>();
  const inFlight = new Set<number>();
  const videos = new Map<number, HTMLVideoElement>();
  let pending: number[] = [];

  const aimedEntry = $derived(aimed === null ? null : (entries[aimed] ?? null));

  $effect(() => {
    if (!canvas || gallery) return;
    try {
      gallery = createGallery(canvas, {
        onRoom: (next) => (room = next),
        onNear: (near) => {
          pending = near;
          pump();
        },
        onLook: (entry) => (aimed = entry),
      });
    } catch (e) {
      failure = String(e);
    }
  });

  $effect(() => {
    if (!gallery) return;
    loaded.clear();
    inFlight.clear();
    for (const video of videos.values()) video.pause();
    videos.clear();
    gallery.setEntries(entries);
    hung = gallery.rooms().reduce((sum, r) => sum + r.hung, 0);
  });

  async function pump() {
    if (!gallery) return;
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
          gallery?.setImage(index, image);
        } else if (found.kind === "playable" && videos.size < MAX_VIDEOS) {
          const video = document.createElement("video");
          video.src = convertFileSrc(entry.source);
          video.muted = true;
          video.loop = true;
          video.playsInline = true;
          video.crossOrigin = "anonymous";
          await video.play().catch(() => undefined);
          videos.set(index, video);
          gallery?.setVideo(index, video);
        }
      } catch {
        loaded.add(index);
      } finally {
        inFlight.delete(index);
      }
    }
  }

  function push() {
    gallery?.setIntent({
      forward: (held.has("w") ? 1 : 0) - (held.has("s") ? 1 : 0),
      strafe: (held.has("d") ? 1 : 0) - (held.has("a") ? 1 : 0),
      running: held.has("shift"),
    });
  }

  function keyName(event: KeyboardEvent): string {
    if (event.key === "Shift") return "shift";
    if (event.key === "ArrowUp") return "w";
    if (event.key === "ArrowDown") return "s";
    if (event.key === "ArrowLeft") return "a";
    if (event.key === "ArrowRight") return "d";
    return event.key.toLowerCase();
  }

  function down(event: KeyboardEvent) {
    if (!walking) return;
    const key = keyName(event);
    if (["w", "a", "s", "d", "shift"].includes(key)) {
      event.preventDefault();
      held.add(key);
      push();
    }
    if (event.key === "e" && aimedEntry) onSelect(aimedEntry);
  }

  function up(event: KeyboardEvent) {
    held.delete(keyName(event));
    push();
  }

  function move(event: MouseEvent) {
    if (!walking) return;
    gallery?.turn(event.movementX, event.movementY);
  }

  function enter() {
    canvas?.requestPointerLock();
  }

  function lockChanged() {
    walking = document.pointerLockElement === canvas;
    if (!walking) {
      held.clear();
      push();
    }
  }

  function click() {
    if (!walking) {
      enter();
      return;
    }
    if (aimedEntry) onSelect(aimedEntry);
  }

  $effect(() => {
    document.addEventListener("pointerlockchange", lockChanged);
    return () => document.removeEventListener("pointerlockchange", lockChanged);
  });

  onDestroy(() => {
    for (const video of videos.values()) video.pause();
    gallery?.dispose();
    if (document.pointerLockElement) document.exitPointerLock();
  });
</script>

<svelte:window onkeydown={down} onkeyup={up} onmousemove={move} />

<div class="gallery">
  {#if failure}
    <div class="fallback faint">
      <p>The gallery needs WebGL2, which this system did not provide.</p>
      <p class="mono small">{failure}</p>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <canvas bind:this={canvas} onclick={click}></canvas>

    {#if walking}
      <div class="crosshair" class:live={aimedEntry !== null}></div>
    {/if}

    <div class="plaque">
      {#if room}
        <strong>{room.label}</strong>
        <span class="faint">
          {room.hung} of {room.files}
          {room.files === 1 ? "file" : "files"} on the walls
        </span>
      {:else if entries.length === 0}
        <strong>Nothing to hang</strong>
        <span class="faint">Run a scan and the rooms fill themselves.</span>
      {:else}
        <strong>Between rooms</strong>
      {/if}
    </div>

    {#if aimedEntry}
      <div class="label">
        <strong class="truncate">{aimedEntry.name}</strong>
        <span class="faint">{aimedEntry.taken}</span>
        <span class="faint tiny">click or press E to open it</span>
      </div>
    {/if}

    {#if !walking}
      <button class="invite" onclick={enter}>
        <strong>Walk the gallery</strong>
        <span>
          {entries.length.toLocaleString()} files ·
          {gallery?.rooms().length ?? 0} rooms · {hung} hung
        </span>
        <span class="keys">
          <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> move ·
          <kbd>Shift</kbd> run · mouse to look · <kbd>Esc</kbd> to let go
        </span>
      </button>
    {/if}
  {/if}
</div>

<style>
  .gallery {
    position: relative;
    height: 100%;
    background: #0a0c11;
    overflow: hidden;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: pointer;
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

  .crosshair {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 7px;
    height: 7px;
    margin: -3.5px 0 0 -3.5px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.55);
    pointer-events: none;
    transition: transform 120ms ease;
  }

  .crosshair.live {
    border-color: var(--accent);
    background: rgba(57, 197, 243, 0.35);
    transform: scale(1.6);
  }

  .plaque {
    position: absolute;
    top: 12px;
    left: 12px;
    display: grid;
    gap: 2px;
    padding: 8px 11px;
    font-size: 11px;
    background: rgba(10, 12, 17, 0.72);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    pointer-events: none;
  }

  .plaque strong {
    font-size: 15px;
    letter-spacing: 0.02em;
  }

  .label {
    position: absolute;
    left: 50%;
    bottom: 46px;
    transform: translateX(-50%);
    display: grid;
    justify-items: center;
    gap: 1px;
    padding: 7px 14px;
    max-width: 60%;
    font-size: 11px;
    text-align: center;
    background: rgba(10, 12, 17, 0.78);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    pointer-events: none;
  }

  .tiny {
    font-size: 9px;
  }

  .invite {
    position: absolute;
    inset: 0;
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 7px;
    background: rgba(6, 8, 12, 0.55);
    border: 0;
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
  }

  .invite strong {
    font-size: 21px;
    letter-spacing: 0.03em;
  }

  .invite span {
    color: var(--text-dim);
  }

  .keys {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
  }

  kbd {
    background: var(--bg-raised);
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: 3px;
    padding: 1px 5px;
    font-family: var(--mono);
    font-size: 10px;
  }
</style>
