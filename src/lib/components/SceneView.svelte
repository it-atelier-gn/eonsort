<script lang="ts">
  import { onDestroy } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import {
    cancelDepthInstall,
    cancelDiffuseInstall,
    clearSceneFit,
    depthModelStatus,
    diffuseModelStatus,
    estimateDepth,
    fillHere,
    fillWithService,
    fitSceneWithModel,
    formatBytes,
    getSceneFit,
    installDepthModel,
    installDiffuseModel,
    setSceneFit,
    thumbnailFor,
    type DepthModelStatus,
    type DepthProgress,
    type DiffuseModelStatus,
    type EntryView,
    type SceneFitView,
  } from "$lib/api";
  import { decodeGrid, reliefOf, type DepthGrid } from "$lib/viz/scene3d/depth";
  import { healDepth, healPixels, BAND, type Heal } from "$lib/viz/scene3d/heal";
  import {
    applyFill,
    bandImage,
    letterbox,
    sideOf,
    FILL_SIZES,
    type Box,
  } from "$lib/viz/scene3d/fill";
  import { createScene, preparePhoto } from "$lib/viz/scene3d/scene";
  import {
    buildScene,
    clampFit,
    defaultFit,
    fitAround,
    flatFit,
    isPhoto,
    moveCorner,
    moveVp,
    photoAspect,
    setFocal,
    stripWindow,
    FOCAL_MAX,
    FOCAL_MIN,
    type Corner,
    type SceneFit,
  } from "$lib/viz/scene3d";

  type Filler = "none" | "nearest" | "service" | "local";

  interface Mend {
    grid: DepthGrid;
    image: HTMLCanvasElement;
    filled: number;
    from: Filler;
  }

  interface Service {
    endpoint: string;
    key: string;
    model: string;
    size: string;
    prompt: string;
  }

  interface Props {
    entries: EntryView[];
    selected: EntryView | null;
    onSelect: (entry: EntryView) => void;
    modelReady: boolean;
  }

  const STRIP_EDGE = 128;
  const FULL_EDGE = 1024;
  const ITEM_WIDTH = 76;
  const DECODE_WAIT = 6000;
  const THUMB_WAIT = 15000;
  const SERVICE_KEY = "eonsort.scene.fill";
  const PAINT_SIDE = 512;
  const PAINT_STEPS = 20;
  const PAINT_WAIT = 30000;
  const BLANK_SERVICE: Service = {
    endpoint: "http://localhost:8080",
    key: "",
    model: "",
    size: "1024x1024",
    prompt: "",
  };

  let { entries, selected, onSelect, modelReady }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let frame = $state<HTMLElement | null>(null);
  let strip = $state<HTMLElement | null>(null);
  let renderer = $state<ReturnType<typeof createScene> | null>(null);

  let failure = $state<string | null>(null);
  let trouble = $state<string | null>(null);
  let loading = $state(false);
  let mode = $state<"fit" | "walk">("fit");
  let fit = $state<SceneFit>(defaultFit());
  let aspect = $state(1.5);
  let display = $state<string | null>(null);
  let opened = $state<EntryView | null>(null);
  let dragging = $state<Corner | "vp" | null>(null);
  let touched = $state(false);
  let asking = $state(false);
  let guess = $state<string | null>(null);
  let boards = $state(false);
  let depthModel = $state<DepthModelStatus | null>(null);
  let paintModel = $state<DiffuseModelStatus | null>(null);
  let painting = $state(false);
  let painted = $state<DepthProgress | null>(null);
  let grid = $state<DepthGrid | null>(null);
  let filler = $state<Filler>("nearest");
  let mended = $state<Mend | null>(null);
  let service = $state<Service>(rememberedService());
  let filling = $state(false);
  let served = $state<string | null>(null);
  let picture: TexImageSource | null = null;
  let healed: Heal | null = null;
  let sounding = $state(false);
  let strength = $state(1);
  let fetching = $state(false);
  let fetched = $state<DepthProgress | null>(null);
  let window0 = $state({ from: 0, to: 24 });
  let thumbs = $state(new Map<string, string>());

  const held = new Set<string>();
  let queue: EntryView[] = [];
  let pumping = false;
  let token = 0;

  const photos = $derived(entries.filter((entry) => isPhoto(entry.name)));
  const shown = $derived(photos.slice(window0.from, window0.to));
  const lead = $derived(Math.min(window0.from, photos.length) * ITEM_WIDTH);
  const tail = $derived(Math.max(0, photos.length - Math.max(window0.to, window0.from)) * ITEM_WIDTH);
  const wanted = $derived(selected && isPhoto(selected.name) ? selected : null);
  const photo = $derived(opened ?? wanted);
  const scene = $derived(buildScene(fit, aspect));
  const corners = $derived<[Corner, number, number][]>([
    ["tl", scene.fit.rect.u0, scene.fit.rect.v0],
    ["tr", scene.fit.rect.u1, scene.fit.rect.v0],
    ["br", scene.fit.rect.u1, scene.fit.rect.v1],
    ["bl", scene.fit.rect.u0, scene.fit.rect.v1],
  ]);

  $effect(() => {
    if (!canvas || renderer) return;
    try {
      renderer = createScene(canvas);
    } catch (e) {
      failure = String(e);
    }
  });

  $effect(() => {
    if (renderer) renderer.setScene(scene);
  });

  $effect(() => {
    renderer?.setBillboards(boards);
  });

  $effect(() => {
    if (!renderer) return;
    renderer.setRelief(grid ? reliefOf(scene, grid, strength) : null);
  });

  $effect(() => {
    if (!renderer) return;
    renderer.setBehind(mended ? reliefOf(scene, mended.grid, strength) : null, null);
  });

  $effect(() => {
    void refreshDepthModel();
    void refreshPaintModel();

    const stops = [
      listen<DepthProgress>("depth:progress", (event) => (fetched = event.payload)),
      listen<number>("depth:done", () => {
        fetching = false;
        fetched = null;
        void refreshDepthModel();
      }),
      listen<string>("depth:error", (event) => {
        fetching = false;
        fetched = null;
        guess = event.payload;
        void refreshDepthModel();
      }),
      listen<DepthProgress>("diffuse:progress", (event) => (painted = event.payload)),
      listen<number>("diffuse:done", () => {
        painting = false;
        painted = null;
        void refreshPaintModel();
      }),
      listen<string>("diffuse:error", (event) => {
        painting = false;
        painted = null;
        served = event.payload;
        void refreshPaintModel();
      }),
    ];

    return () => {
      for (const stop of stops) void stop.then((off) => off());
    };
  });

  async function refreshDepthModel() {
    try {
      depthModel = await depthModelStatus();
    } catch {
      depthModel = null;
    }
  }

  async function refreshPaintModel() {
    try {
      paintModel = await diffuseModelStatus();
    } catch {
      paintModel = null;
    }
  }

  async function getPaintModel() {
    served = null;
    painting = true;
    try {
      await installDiffuseModel();
    } catch (e) {
      painting = false;
      served = String(e);
    }
  }

  async function getDepthModel() {
    guess = null;
    fetching = true;
    try {
      await installDepthModel();
    } catch (e) {
      fetching = false;
      guess = String(e);
    }
  }

  $effect(() => {
    const entry = wanted;
    if (!entry || !renderer || entry.source === opened?.source) return;

    const mine = ++token;
    loading = true;
    trouble = null;

    void (async () => {
      try {
        if (!(await show(entry, false, mine))) {
          await show(entry, true, mine);
        }
        if (mine === token) await restore(entry, mine);
      } catch (e) {
        if (mine === token) {
          trouble = `This system could not open ${entry.name}: ${e}`;
          display = null;
          opened = null;
        }
      } finally {
        if (mine === token) loading = false;
      }
    })();
  });

  function within<T>(work: Promise<T>, ms: number, what: string): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${what} took longer than ${ms} ms`)), ms);
      work.then(
        (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        (error) => {
          clearTimeout(timer);
          reject(error);
        },
      );
    });
  }

  async function show(entry: EntryView, safely: boolean, mine: number): Promise<boolean> {
    const image = new Image();
    if (safely) {
      const found = await within(
        thumbnailFor(entry.source, FULL_EDGE, entry.rotate),
        THUMB_WAIT,
        "reading the picture",
      );
      if (found.kind !== "image") throw new Error("there is no picture inside that file");
      image.src = `data:image/jpeg;base64,${found.data}`;
    } else {
      image.crossOrigin = "anonymous";
      image.src = convertFileSrc(entry.source);
    }

    try {
      await within(image.decode(), DECODE_WAIT, "opening the picture");
    } catch (e) {
      if (safely) throw e;
      image.src = "";
      return false;
    }
    if (mine !== token) return true;

    const transform = safely ? "none" : entry.rotate;
    const prepared = preparePhoto(image, transform);

    try {
      renderer?.setPhoto(prepared.source);
      display =
        prepared.source === image
          ? image.src
          : (prepared.source as HTMLCanvasElement).toDataURL("image/jpeg", 0.92);
    } catch (e) {
      if (safely) throw e;
      return false;
    }

    opened = entry;
    picture = prepared.source;
    mended = null;
    aspect = photoAspect(image.width, image.height, transform);
    fit = defaultFit();
    touched = false;
    guess = null;
    boards = false;
    grid = null;
    renderer?.reset();
    return true;
  }

  async function restore(entry: EntryView, mine: number) {
    let saved: SceneFitView | null = null;
    try {
      saved = await getSceneFit(entry.source);
    } catch {
      return;
    }
    if (!saved || mine !== token) return;

    fit = clampFit({
      vp: { u: saved.vp[0], v: saved.vp[1] },
      rect: { u0: saved.rect[0], v0: saved.rect[1], u1: saved.rect[2], v1: saved.rect[3] },
      focal: saved.focal,
      objects: saved.objects.map((object) => ({
        label: object.label,
        u0: object.bounds[0],
        v0: object.bounds[1],
        u1: object.bounds[2],
        v1: object.bounds[3],
      })),
    });
  }

  function asView(value: SceneFit): SceneFitView {
    return {
      vp: [value.vp.u, value.vp.v],
      rect: [value.rect.u0, value.rect.v0, value.rect.u1, value.rect.v1],
      focal: value.focal,
      objects: value.objects.map((object) => ({
        label: object.label,
        bounds: [object.u0, object.v0, object.u1, object.v1],
      })),
    };
  }

  function edit(next: SceneFit) {
    fit = next;
    touched = true;
  }

  async function askModel() {
    const entry = photo;
    if (!entry || asking) return;

    asking = true;
    guess = null;
    try {
      const answer = await fitSceneWithModel(entry.source);
      const seeded = fitAround(
        { u: answer.vanishing_point[0], v: answer.vanishing_point[1] },
        undefined,
        fit.focal,
        answer.objects.map((object) => ({
          label: object.label,
          u0: object.bounds[0],
          v0: object.bounds[1],
          u1: object.bounds[2],
          v1: object.bounds[3],
        })),
      );

      edit(
        answer.flat
          ? flatFit(seeded)
          : clampFit({
              ...seeded,
              rect: {
                u0: answer.back_wall[0],
                v0: answer.back_wall[1],
                u1: answer.back_wall[2],
                v1: answer.back_wall[3],
              },
            }),
      );

      boards = seeded.objects.length > 0;
      guess = answer.flat
        ? `The model saw ${answer.scene_type ?? "no perspective"} here, so this is a picture wall. Drag the handles if you disagree.`
        : `The model's guess${answer.scene_type ? ` — ${answer.scene_type}` : ""}. Check it against the picture and drag the handles.`;
    } catch (e) {
      guess = String(e);
    } finally {
      asking = false;
    }
  }

  async function sound() {
    const entry = photo;
    if (!entry || sounding) return;

    sounding = true;
    guess = null;
    try {
      const answer = await estimateDepth(entry.source, 256);
      grid = decodeGrid(answer.width, answer.height, answer.data);
      mend();
    } catch (e) {
      guess = String(e);
      grid = null;
      mended = null;
    } finally {
      sounding = false;
    }
  }

  function mend() {
    served = null;
    healed = null;

    if (!grid || !picture || filler === "none") {
      mended = null;
      return;
    }

    const { width, height } = grid;
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;

    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) {
      mended = null;
      return;
    }

    context.drawImage(picture as CanvasImageSource, 0, 0, width, height);
    const heal = healDepth(grid, BAND);
    if (heal.filled === 0) {
      mended = null;
      return;
    }

    const seen = context.getImageData(0, 0, width, height);
    context.putImageData(new ImageData(healPixels(seen.data, heal.source), width, height), 0, 0);
    healed = heal;
    mended = { grid: heal.healed, image: canvas, filled: heal.filled, from: "nearest" };
    renderer?.setBehind(reliefOf(scene, heal.healed, strength), canvas);
  }

  function rememberedService(): Service {
    if (typeof localStorage === "undefined") return { ...BLANK_SERVICE };
    try {
      const stored = localStorage.getItem(SERVICE_KEY);
      if (!stored) return { ...BLANK_SERVICE };
      const held = JSON.parse(stored) as Partial<Service>;
      return { ...BLANK_SERVICE, ...held };
    } catch {
      return { ...BLANK_SERVICE };
    }
  }

  function keepService() {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(SERVICE_KEY, JSON.stringify(service));
    } catch {
      served = "this fill service could not be remembered for next time";
    }
  }

  function asPng(source: CanvasImageSource, box: Box, side: number, band: boolean): string | null {
    const sheet = document.createElement("canvas");
    sheet.width = side;
    sheet.height = side;
    const context = sheet.getContext("2d");
    if (!context) return null;

    context.fillStyle = "#000";
    context.fillRect(0, 0, side, side);
    context.imageSmoothingEnabled = !band;
    if (band) context.globalCompositeOperation = "destination-out";
    context.drawImage(source, box.x, box.y, box.width, box.height);
    return sheet.toDataURL("image/png");
  }

  async function serve() {
    const heal = healed;
    const here = filler === "local";
    if (!grid || !picture || !heal || filling) return;
    if (here && !paintModel?.present) {
      served = "the painting model is not downloaded yet";
      return;
    }

    filling = true;
    served = null;
    try {
      const { width, height } = grid;
      const side = here ? PAINT_SIDE : sideOf(service.size);
      const box = letterbox(width, height, side);
      if (box.width === 0) throw new Error("the depth grid has no size");

      const stencil = document.createElement("canvas");
      stencil.width = width;
      stencil.height = height;
      const inside = stencil.getContext("2d");
      if (!inside) throw new Error("this browser gave no drawing surface");
      inside.putImageData(new ImageData(bandImage(heal.mask), width, height), 0, 0);

      const image = asPng(picture as CanvasImageSource, box, side, false);
      const mask = asPng(stencil, box, side, true);
      if (!image || !mask) throw new Error("this browser gave no drawing surface");

      const answer = here
        ? await fillHere({ prompt: service.prompt, steps: PAINT_STEPS, image, mask })
        : await fillWithService({
            endpoint: service.endpoint,
            key: service.key,
            model: service.model,
            prompt: service.prompt,
            size: service.size,
            image,
            mask,
          });

      if (!here) keepService();

      const fresh = new Image();
      fresh.src = `data:image/png;base64,${answer}`;
      await within(fresh.decode(), PAINT_WAIT, "opening the filled picture");

      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d", { willReadFrequently: true });
      if (!context) throw new Error("this browser gave no drawing surface");

      context.drawImage(picture as CanvasImageSource, 0, 0, width, height);
      const base = context.getImageData(0, 0, width, height).data;

      context.clearRect(0, 0, width, height);
      context.drawImage(fresh, box.x, box.y, box.width, box.height, 0, 0, width, height);
      const filledPixels = context.getImageData(0, 0, width, height).data;

      context.putImageData(
        new ImageData(applyFill(base, filledPixels, heal.mask), width, height),
        0,
        0,
      );

      mended = { grid: heal.healed, image: canvas, filled: heal.filled, from: here ? "local" : "service" };
      renderer?.setBehind(reliefOf(scene, heal.healed, strength), canvas);
    } catch (e) {
      served = String(e);
    } finally {
      filling = false;
    }
  }

  function forget() {
    const entry = photo;
    fit = defaultFit();
    touched = false;
    if (entry) void clearSceneFit(entry.source).catch(() => undefined);
  }

  $effect(() => {
    const entry = photo;
    const current = fit;
    if (!entry || !touched) return;

    const timer = setTimeout(() => {
      void setSceneFit(entry.source, asView(current)).catch(() => undefined);
    }, 400);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    if (!strip || photos.length === 0) return;
    scrolled();
  });

  $effect(() => {
    queue = photos.slice(window0.from, window0.to);
    void pump();
  });

  async function pump() {
    if (pumping) return;
    pumping = true;
    try {
      while (queue.length > 0) {
        const entry = queue.shift();
        if (!entry || thumbs.has(entry.source)) continue;

        let found = "";
        try {
          const answer = await thumbnailFor(entry.source, STRIP_EDGE, entry.rotate);
          if (answer.kind === "image") found = `data:image/jpeg;base64,${answer.data}`;
        } catch {
          found = "";
        }

        thumbs.set(entry.source, found);
        thumbs = new Map(thumbs);
      }
    } finally {
      pumping = false;
    }
  }

  function scrolled() {
    if (!strip) return;
    window0 = stripWindow(photos.length, strip.scrollLeft, strip.clientWidth, ITEM_WIDTH);
  }

  function pointAt(event: PointerEvent): { u: number; v: number } {
    const box = frame?.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return { u: 0.5, v: 0.5 };
    return {
      u: (event.clientX - box.left) / box.width,
      v: (event.clientY - box.top) / box.height,
    };
  }

  function grab(event: PointerEvent, what: Corner | "vp") {
    event.preventDefault();
    dragging = what;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function drag(event: PointerEvent) {
    if (!dragging) return;
    const { u, v } = pointAt(event);
    edit(dragging === "vp" ? moveVp(fit, u, v) : moveCorner(fit, dragging, u, v));
  }

  function drop(event: PointerEvent) {
    if (!dragging) return;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    dragging = null;
  }

  function push() {
    renderer?.setIntent({
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
    if (mode !== "walk") return;
    const key = keyName(event);
    if (["w", "a", "s", "d", "shift"].includes(key)) {
      event.preventDefault();
      held.add(key);
      push();
    }
  }

  function up(event: KeyboardEvent) {
    held.delete(keyName(event));
    push();
  }

  function move(event: MouseEvent) {
    if (mode !== "walk") return;
    renderer?.turn(event.movementX, event.movementY);
  }

  function enter() {
    renderer?.reset();
    canvas?.requestPointerLock();
  }

  function lockChanged() {
    const walking = document.pointerLockElement === canvas;
    mode = walking ? "walk" : "fit";
    if (!walking) {
      held.clear();
      push();
    }
  }

  $effect(() => {
    document.addEventListener("pointerlockchange", lockChanged);
    return () => document.removeEventListener("pointerlockchange", lockChanged);
  });

  onDestroy(() => {
    renderer?.dispose();
    if (document.pointerLockElement) document.exitPointerLock();
  });
</script>

<svelte:window onkeydown={down} onkeyup={up} onmousemove={move} onresize={scrolled} />

<div class="scene">
  {#if failure}
    <div class="fallback faint">
      <p>The scene needs WebGL2, which this system did not provide.</p>
      <p class="mono small">{failure}</p>
    </div>
  {:else}
    <div class="stage" class:walking={mode === "walk"}>
      <canvas bind:this={canvas}></canvas>

      {#if mode === "fit"}
        <div class="veil">
          {#if !photo}
            <div class="prompt faint">
              {#if selected}
                <strong>{selected.name} is not a photograph</strong>
                <span>Pick a picture below and it becomes a room you can walk into.</span>
              {:else}
                <strong>Pick a photograph</strong>
                <span>Every picture with perspective in it can be turned into a room.</span>
              {/if}
            </div>
          {:else if trouble}
            <div class="prompt faint">
              <strong>That picture would not open</strong>
              <span class="mono small">{trouble}</span>
            </div>
          {:else if !display}
            <div class="prompt faint">
              <strong>Opening {photo.name}…</strong>
            </div>
          {:else}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="frame"
              bind:this={frame}
              onpointermove={drag}
              onpointerup={drop}
              onpointercancel={drop}
            >
              <img src={display} alt={photo.name} draggable="false" />

              <svg viewBox="0 0 1 1" preserveAspectRatio="none" aria-hidden="true">
                {#each corners as [key, u, v] (key)}
                  <line
                    x1={key === "tl" || key === "bl" ? 0 : 1}
                    y1={key === "tl" || key === "tr" ? 0 : 1}
                    x2={u}
                    y2={v}
                    class="guide"
                    vector-effect="non-scaling-stroke"
                  />
                {/each}
                <rect
                  x={scene.fit.rect.u0}
                  y={scene.fit.rect.v0}
                  width={scene.fit.rect.u1 - scene.fit.rect.u0}
                  height={scene.fit.rect.v1 - scene.fit.rect.v0}
                  class="wall"
                  vector-effect="non-scaling-stroke"
                />
                {#each scene.fit.objects as object (object.label + object.u0)}
                  <rect
                    x={object.u0}
                    y={object.v0}
                    width={object.u1 - object.u0}
                    height={object.v1 - object.v0}
                    class="object"
                    vector-effect="non-scaling-stroke"
                  />
                {/each}
              </svg>

              {#each corners as [key, u, v] (key)}
                <button
                  class="handle corner"
                  style="left: {u * 100}%; top: {v * 100}%"
                  aria-label="back wall corner"
                  onpointerdown={(event) => grab(event, key)}
                ></button>
              {/each}

              <button
                class="handle vp"
                style="left: {scene.fit.vp.u * 100}%; top: {scene.fit.vp.v * 100}%"
                aria-label="vanishing point"
                onpointerdown={(event) => grab(event, "vp")}
              ></button>
            </div>
          {/if}
        </div>
      {/if}

      {#if mode === "walk"}
        <div class="crosshair"></div>
      {/if}
    </div>

    {#if mode === "fit"}
      <div class="tools">
        <div class="plaque" data-depth={scene.depth.toFixed(2)}>
          {#if photo}
            <strong class="truncate">{photo.name}</strong>
            <span class="faint">
              {scene.depth.toFixed(1)} m deep ·
              {(scene.bounds.x1 - scene.bounds.x0).toFixed(1)} m wide ·
              {scene.bounds.y1.toFixed(1)} m high
            </span>
          {:else}
            <strong>No picture yet</strong>
          {/if}
        </div>

        {#if photo}
          <label class="lens">
            <span class="faint tiny">lens</span>
            <input
              type="range"
              min={FOCAL_MIN}
              max={FOCAL_MAX}
              step="0.05"
              value={scene.fit.focal}
              oninput={(event) => edit(setFocal(fit, Number(event.currentTarget.value)))}
            />
          </label>

          {#if modelReady}
            <button onclick={askModel} disabled={asking}>
              {asking ? "Looking…" : "Ask the model"}
            </button>
          {/if}
          {#if depthModel?.built_in && depthModel.present}
            <button onclick={sound} disabled={sounding}>
              {sounding ? "Sounding…" : grid ? "Re-read depth" : "Read depth"}
            </button>
          {:else if depthModel?.built_in && fetching}
            <button onclick={() => void cancelDepthInstall()}>
              {fetched
                ? `Stop (${formatBytes(fetched.completed)} of ${formatBytes(fetched.total)})`
                : "Stop"}
            </button>
          {:else if depthModel?.built_in}
            <button onclick={getDepthModel} title="Downloads about 190 MB of model weights">
              Get depth model
            </button>
          {/if}
          {#if grid}
            <label class="lens">
              <span class="faint tiny">fill</span>
              <select
                value={filler}
                onchange={(event) => {
                  filler = event.currentTarget.value as Filler;
                  mend();
                }}
              >
                <option value="nearest">nearest</option>
                <option value="service">service</option>
                <option value="local">local</option>
                <option value="none">none</option>
              </select>
            </label>
            {#if filler === "service" || filler === "local"}
              <button onclick={serve} disabled={filling || !mended}>
                {filling
                  ? "Filling…"
                  : mended?.from === "service" || mended?.from === "local"
                    ? "Fill again"
                    : "Fill"}
              </button>
            {/if}
            {#if filler === "local"}
              {#if paintModel?.built_in && paintModel.present}
                <span class="faint tiny">painting model ready</span>
              {:else if paintModel?.built_in && painting}
                <button onclick={() => void cancelDiffuseInstall()}>
                  {painted
                    ? `Stop (${formatBytes(painted.completed)} of ${formatBytes(painted.total)})`
                    : "Stop"}
                </button>
              {:else if paintModel?.built_in}
                <button onclick={getPaintModel} title="Downloads about 2.1 GB of model weights">
                  Get painting model
                </button>
              {/if}
            {/if}
            <label class="lens">
              <span class="faint tiny">relief</span>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={strength}
                oninput={(event) => (strength = Number(event.currentTarget.value))}
              />
            </label>
          {/if}
          {#if scene.billboards.length > 0}
            <button class:active={boards} onclick={() => (boards = !boards)}>
              Cut-outs ({scene.billboards.length})
            </button>
          {/if}
          <button onclick={forget}>Reset</button>
          <button onclick={() => edit(flatFit(fit))}>Flat</button>
          <button class="go" onclick={enter} disabled={loading}>Walk in</button>
        {/if}
      </div>

      {#if photo && mode === "fit" && grid && filler === "local"}
        <div class="service">
          <label class="wide">
            <span class="faint tiny">prompt</span>
            <input
              type="text"
              value={service.prompt}
              onchange={(event) => {
                service.prompt = event.currentTarget.value;
                keepService();
              }}
            />
          </label>
        </div>
      {/if}

      {#if photo && mode === "fit" && grid && filler === "service"}
        <div class="service">
          <label>
            <span class="faint tiny">endpoint</span>
            <input
              type="url"
              placeholder="http://localhost:8080"
              value={service.endpoint}
              onchange={(event) => {
                service.endpoint = event.currentTarget.value;
                keepService();
              }}
            />
          </label>
          <label>
            <span class="faint tiny">key</span>
            <input
              type="password"
              placeholder="none for a local one"
              value={service.key}
              onchange={(event) => {
                service.key = event.currentTarget.value;
                keepService();
              }}
            />
          </label>
          <label>
            <span class="faint tiny">model</span>
            <input
              type="text"
              placeholder="gpt-image-1"
              value={service.model}
              onchange={(event) => {
                service.model = event.currentTarget.value;
                keepService();
              }}
            />
          </label>
          <label>
            <span class="faint tiny">size</span>
            <select
              value={service.size}
              onchange={(event) => {
                service.size = event.currentTarget.value;
                keepService();
              }}
            >
              {#each FILL_SIZES as size (size)}
                <option value={size}>{size}</option>
              {/each}
            </select>
          </label>
        </div>
      {/if}

      {#if photo && (scene.warnings.length > 0 || guess || served || grid || (selected && !wanted))}
        <div class="warnings faint tiny">
          {#if selected && !wanted}
            <p>Scene needs a photograph; {selected.name} is not one, so {photo.name} stays open.</p>
          {/if}
          {#if guess}
            <p>{guess}</p>
          {/if}
          {#if served}
            <p>{served}</p>
          {/if}
          {#if filler === "local" && paintModel && !paintModel.built_in}
            <p>This build was made without the painting model, so local filling cannot run.</p>
          {/if}
          {#each scene.warnings as warning (warning)}
            <p>{warning}</p>
          {/each}
          {#if boards}
            <p>
              A cut-out is a rectangle of the picture, so its subject also stays painted on the wall
              behind it.
            </p>
          {/if}
          {#if grid && (mended?.from === "service" || mended?.from === "local")}
            <p>
              Depth is relative, not measured. The band behind each near edge was painted by the
              {mended?.from === "local" ? "painting model" : "fill service"}, which invented it; the
              distances there are still the nearest real background, and stepping far enough
              sideways shows the flat room again.
            </p>
          {:else if grid && mended}
            <p>
              Depth is relative, not measured. The background is carried a little way behind each
              near edge to cover what the photograph never saw; step far enough sideways and the
              flat room shows through again.
            </p>
          {:else if grid}
            <p>
              Depth is relative, not measured. Edges tear where the photograph has no information
              behind them, and the flat room shows through the gaps.
            </p>
          {/if}
        </div>
      {:else if photo}
        <p class="hint faint tiny">
          This fit is a guess. Drag the cross onto the point where the picture recedes, and the
          corners onto the furthest flat wall.
        </p>
      {/if}

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="strip" bind:this={strip} onscroll={scrolled}>
        {#if lead > 0}
          <div class="gap" style="width:{lead}px"></div>
        {/if}
        {#each shown as entry (entry.source)}
          <button
            class="shot"
            class:on={entry.source === photo?.source}
            title={entry.name}
            onclick={() => onSelect(entry)}
          >
            {#if thumbs.get(entry.source)}
              <img src={thumbs.get(entry.source)} alt={entry.name} draggable="false" />
            {/if}
          </button>
        {/each}
        {#if tail > 0}
          <div class="gap" style="width:{tail}px"></div>
        {/if}
      </div>
    {:else}
      <div class="keys faint tiny">
        <kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> move · <kbd>Shift</kbd> run · mouse to look
        · <kbd>Esc</kbd> to step back out
      </div>
    {/if}
  {/if}
</div>

<style>
  .scene {
    position: relative;
    height: 100%;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto auto auto;
    background: #07090d;
    overflow: hidden;
  }

  .stage {
    position: relative;
    min-height: 0;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }

  .stage.walking canvas {
    cursor: none;
  }

  .veil {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 14px;
    background: rgba(7, 9, 13, 0.82);
  }

  .prompt {
    display: grid;
    gap: 5px;
    justify-items: center;
    text-align: center;
    font-size: 12px;
  }

  .prompt strong {
    font-size: 16px;
  }

  .frame {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    line-height: 0;
    touch-action: none;
    box-shadow: 0 0 0 1px var(--border);
  }

  .frame img {
    display: block;
    width: auto;
    height: auto;
    max-width: 100%;
    max-height: 100%;
    user-select: none;
  }

  .frame svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }

  .guide {
    stroke: rgba(57, 197, 243, 0.35);
    stroke-width: 1;
    stroke-dasharray: 4 4;
  }

  .wall {
    fill: rgba(57, 197, 243, 0.08);
    stroke: var(--accent);
    stroke-width: 1.5;
  }

  .object {
    fill: rgba(255, 212, 121, 0.08);
    stroke: #ffd479;
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }

  .tools button.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  .handle {
    position: absolute;
    width: 15px;
    height: 15px;
    margin: -7.5px 0 0 -7.5px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid #06080c;
    background: var(--accent);
    cursor: grab;
    touch-action: none;
  }

  .handle:active {
    cursor: grabbing;
  }

  .handle.vp {
    width: 19px;
    height: 19px;
    margin: -9.5px 0 0 -9.5px;
    border-radius: 3px;
    background: #ffd479;
    transform: rotate(45deg);
  }

  .crosshair {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 7px;
    height: 7px;
    margin: -3.5px 0 0 -3.5px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.5);
    pointer-events: none;
  }

  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-top: 1px solid var(--border);
  }

  .plaque {
    display: grid;
    gap: 1px;
    margin-right: auto;
    min-width: 0;
    font-size: 11px;
  }

  .plaque strong {
    font-size: 13px;
  }

  .lens {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .lens input {
    width: 96px;
  }

  .go {
    border-color: var(--accent);
  }

  .warnings,
  .hint {
    margin: 0;
    padding: 0 10px 7px;
    display: grid;
    gap: 2px;
  }

  .warnings p {
    margin: 0;
  }

  .service {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 0 10px 7px;
  }

  .service label {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .service input,
  .service select {
    min-width: 90px;
    max-width: 220px;
  }

  .service input[type="url"] {
    min-width: 180px;
  }

  .service label.wide {
    flex: 1 1 100%;
  }

  .service label.wide input {
    max-width: none;
  }

  .strip {
    display: flex;
    gap: 4px;
    padding: 7px 10px;
    overflow-x: auto;
    border-top: 1px solid var(--border);
  }

  .gap {
    flex: 0 0 auto;
    height: 54px;
  }

  .shot {
    flex: 0 0 auto;
    width: 72px;
    height: 54px;
    padding: 0;
    overflow: hidden;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }

  .shot.on {
    border-color: var(--accent);
  }

  .shot img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .keys {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 9px;
    border-top: 1px solid var(--border);
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

  .fallback {
    padding: 40px;
    font-size: 12px;
    display: grid;
    gap: 8px;
  }

  .small {
    font-size: 10px;
  }

  .tiny {
    font-size: 9px;
  }
</style>
