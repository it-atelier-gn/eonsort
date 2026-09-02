<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import {
    formatBytes,
    probeRotation,
    type DateChoice,
    type EntryView,
    type NameCount,
    type Preview,
    type Spot,
  } from "$lib/api";
  import { boxOf as faceBox, pickable, sureFaces } from "$lib/faces";
  import { CONFIDENCE_LABEL, CONFIDENCE_TONE, fromInputValue, toInputValue } from "$lib/dates";
  import { labelOf } from "$lib/tags";
  import {
    VISUAL_PANE,
    VISUAL_PANE_KEY,
    clampVisual,
    cleanVisual,
  } from "$lib/columns";
  import {
    TRANSFORM_CSS,
    canTurn,
    describeRotation,
    forOrientation,
    swapsAxes,
    undone,
  } from "$lib/rotate";
  import { fittedTo } from "$lib/fit";
  import {
    isResting,
    pannedBy,
    steppedIn,
    steppedOut,
    transformOf,
    wheelFactor,
    zoomedAt,
    RESTING,
    type Zoom,
  } from "$lib/zoom";

  interface Props {
    entry: EntryView | null;
    preview: Preview | null;
    loading: boolean;
    busy: boolean;
    onOpen: (path: string) => void;
    onReveal: (path: string) => void;
    onLike?: (path: string) => void;
    onChoose: (choice: DateChoice) => void;
    onRevert: () => void;
    onTurn: (quarterTurns: number) => void;
    onResetTurn: () => void;
    onReencode: () => void;
    faces?: Spot[];
    showFaces?: boolean;
    onShowFaces?: (show: boolean) => void;
    onName?: (ord: number, label: string | null, spread: boolean) => void;
    names?: NameCount[];
  }

  let {
    entry,
    preview,
    loading,
    busy,
    onOpen,
    onReveal,
    onLike,
    onChoose,
    onRevert,
    onTurn,
    onResetTurn,
    onReencode,
    faces = [],
    showFaces = true,
    onShowFaces,
    onName,
    names = [],
  }: Props = $props();

  const foundFaces = $derived(sureFaces(faces));
  const shownFaces = $derived(showFaces ? foundFaces : []);
  let shot = $state<HTMLImageElement | null>(null);
  let visualHeight = $state<number>(rememberedVisual());
  let visualSplitting = $state<boolean>(false);

  const VISUAL_STEP = 8;

  function rememberedVisual(): number {
    if (typeof localStorage === "undefined") return VISUAL_PANE;
    try {
      const held = localStorage.getItem(VISUAL_PANE_KEY);
      return cleanVisual(held === null ? null : JSON.parse(held));
    } catch {
      return VISUAL_PANE;
    }
  }

  function sizeVisual(height: number) {
    visualHeight = clampVisual(height);
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(VISUAL_PANE_KEY, JSON.stringify(visualHeight));
    } catch {
      // a browser that refuses to remember is not a reason to stop
    }
  }

  function grabVisualSplit(event: PointerEvent) {
    const bar = event.currentTarget as HTMLElement;
    event.preventDefault();
    visualSplitting = true;
    bar.setPointerCapture(event.pointerId);
  }

  function slideVisualSplit(event: PointerEvent) {
    if (!visualSplitting) return;
    const bar = event.currentTarget as HTMLElement;
    const top = bar.previousElementSibling?.getBoundingClientRect().top ?? 0;
    sizeVisual(event.clientY - top);
  }

  function releaseVisualSplit(event: PointerEvent) {
    if (!visualSplitting) return;
    const bar = event.currentTarget as HTMLElement;
    if (bar.hasPointerCapture(event.pointerId)) bar.releasePointerCapture(event.pointerId);
    visualSplitting = false;
  }

  function stretchVisualSplit(event: KeyboardEvent) {
    const by = event.key === "ArrowUp" ? -VISUAL_STEP : event.key === "ArrowDown" ? VISUAL_STEP : 0;
    if (by === 0) return;
    event.preventDefault();
    sizeVisual(visualHeight + by);
  }
  let stage = $state<HTMLElement | null>(null);
  let natural = $state<{ width: number; height: number } | null>(null);
  let room = $state({ width: 0, height: 0 });

  function measure() {
    const it = shot;
    natural =
      it && it.naturalWidth > 0 && it.naturalHeight > 0
        ? { width: it.naturalWidth, height: it.naturalHeight }
        : null;
  }

  $effect(() => {
    preview;
    const it = shot;
    if (it && it.complete) {
      measure();
    } else {
      natural = null;
    }
  });

  $effect(() => {
    const it = stage;
    if (!it) return;
    const note = () => (room = { width: it.clientWidth, height: it.clientHeight });
    note();
    if (typeof ResizeObserver === "undefined") return;
    const watching = new ResizeObserver(note);
    watching.observe(it);
    return () => watching.disconnect();
  });
  let naming = $state<number | null>(null);
  let typed = $state("");
  let spread = $state(true);

  function startNaming(ord: number) {
    if (!onName) return;
    naming = ord;
    typed = shownFaces[ord]?.label ?? "";
  }

  const known = $derived(
    naming === null ? [] : pickable(names, shownFaces[naming]?.label ?? null),
  );

  function settle(keep: boolean) {
    if (naming !== null && onName && keep) {
      onName(naming, typed.trim() === "" ? null : typed.trim(), spread);
    }
    naming = null;
    typed = "";
  }

  let zoom = $state<Zoom>(RESTING);
  let frame = $state<HTMLElement | null>(null);
  let holding = $state(false);
  let manual = $state("");
  let anchored = $state("");
  let lossless = $state<boolean | null>(null);
  let losslessReason = $state<string | null>(null);

  $effect(() => {
    if (entry && entry.source !== anchored) {
      anchored = entry.source;
      zoom = RESTING;
      manual = toInputValue(entry.taken_epoch);
      lossless = null;
      losslessReason = null;
    }
  });

  const turnable = $derived(canTurn(entry));
  const turned = $derived(entry !== null && entry.rotate !== "none");
  const painted = $derived(
    entry === null || entry.rotate === "none" ? "none" : forOrientation(entry.orientation),
  );
  const asLaid = $derived(
    natural === null
      ? null
      : swapsAxes(painted)
        ? { width: natural.height, height: natural.width }
        : natural,
  );
  const framed = $derived(
    asLaid === null ? null : fittedTo(asLaid, room, entry !== null && swapsAxes(entry.rotate)),
  );
  const turnStyle = $derived(
    entry === null || entry.rotate === "none"
      ? ""
      : `image-orientation: none; transform: ${TRANSFORM_CSS[entry.rotate]};`,
  );
  const imageStyle = $derived(
    turnStyle +
      (framed === null
        ? entry !== null && swapsAxes(entry.rotate)
          ? " max-width: var(--visual-height); max-height: 100%;"
          : ""
        : ` width: ${framed.width}px; height: ${framed.height}px;`),
  );
  const labelStyle = $derived(
    entry === null || entry.rotate === "none"
      ? ""
      : `transform: ${TRANSFORM_CSS[undone(entry.rotate)]}`,
  );

  async function checkLossless(source: string) {
    const probe = await probeRotation(source);
    lossless = probe.lossless;
    losslessReason = probe.reason;
  }

  $effect(() => {
    if (entry && turnable && lossless === null && !entry.rotate_lossless) {
      lossless = false;
      losslessReason = "only JPEG pictures can be turned without re-encoding them";
    }
  });

  const manualIsValid = $derived(fromInputValue(manual) !== null);
  const zoomable = $derived(preview?.kind === "image" || preview?.kind === "video");

  function boxOf() {
    const box = frame?.getBoundingClientRect();
    return { width: box?.width ?? 0, height: box?.height ?? 0 };
  }

  function pointerIn(event: { clientX: number; clientY: number }) {
    const box = frame?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    return { x: event.clientX - (box.left + box.width / 2), y: event.clientY - (box.top + box.height / 2) };
  }

  function wheel(event: WheelEvent) {
    if (!zoomable || !event.ctrlKey) return;
    event.preventDefault();
    zoom = zoomedAt(zoom, wheelFactor(event.deltaY), pointerIn(event), boxOf());
  }

  function grab(event: PointerEvent) {
    if (!zoomable || isResting(zoom)) return;
    holding = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function drag(event: PointerEvent) {
    if (!holding) return;
    zoom = pannedBy(zoom, event.movementX, event.movementY, boxOf());
  }

  function drop(event: PointerEvent) {
    const element = event.currentTarget as HTMLElement;
    if (element.hasPointerCapture(event.pointerId)) element.releasePointerCapture(event.pointerId);
    holding = false;
  }
</script>

<aside class="pane" style="--visual-pane: {visualHeight}px">
  {#if !entry}
    <div class="empty faint">
      <p>Select a file to preview it.</p>
      <p class="hint">Double-click a row to open it in your usual application.</p>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="visual"
      class:zoomed={!isResting(zoom)}
      class:holding
      bind:this={frame}
      onwheel={wheel}
      onpointerdown={grab}
      onpointermove={drag}
      onpointerup={drop}
      onpointerleave={drop}
    >
      <div
        class="stage"
        bind:this={stage}
        style={transformOf(zoom) ? `transform: ${transformOf(zoom)}` : ""}
      >
      {#if loading}
        <p class="faint">Loading preview…</p>
      {:else if preview?.kind === "image"}
        <div class="framed" class:sized={framed !== null} style={imageStyle}>
          <img
            bind:this={shot}
            src="data:{preview.mime};base64,{preview.data}"
            alt={entry.name}
            onload={measure}
          />
          {#each shownFaces as face, at (at)}
            {@const box = faceBox(face, painted)}
            <button
              type="button"
              class="face"
              class:named={!!face.label}
              class:naming={naming === at}
              style="left:{box.left}; top:{box.top}; width:{box.width}; height:{box.height}"
              title={face.label ? `${face.label} — click to rename` : "Click to say who this is"}
              onclick={() => startNaming(at)}
            >
              {#if face.label}<span class="who" style={labelStyle}>{face.label}</span>{/if}
            </button>
          {/each}
        </div>
      {:else if preview?.kind === "video"}
        <video src={convertFileSrc(entry.source)} controls>
          <track kind="captions" />
        </video>
      {:else if preview?.kind === "audio"}
        <audio src={convertFileSrc(entry.source)} controls>
          <track kind="captions" />
        </audio>
      {:else if preview?.kind === "pdf"}
        <iframe src={convertFileSrc(entry.source)} title={entry.name}></iframe>
      {:else if preview?.kind === "text"}
        <pre class="mono">{preview.head}{preview.truncated ? "\n…" : ""}</pre>
      {:else if preview?.kind === "missing"}
        <p class="faint">The source file is no longer there.</p>
      {:else}
        <p class="faint">No preview for this file type.</p>
      {/if}
      </div>

      {#if naming !== null}
        <div class="naming-box">
          <input
            type="text"
            list="known-faces"
            placeholder="Who is this?"
            bind:value={typed}
            onkeydown={(e) => {
              if (e.key === "Enter") settle(true);
              if (e.key === "Escape") settle(false);
            }}
          />
          {#if known.length > 0}
            <datalist id="known-faces">
              {#each known as person (person.name)}
                <option value={person.name}></option>
              {/each}
            </datalist>
            <div class="known-names">
              {#each known as person (person.name)}
                <button
                  class="known-name"
                  class:picked={typed.trim() === person.name}
                  title="{person.name} is on {person.count} other {person.count === 1
                    ? 'face'
                    : 'faces'}"
                  onclick={() => (typed = person.name)}
                >
                  {person.name}
                </button>
              {/each}
            </div>
          {/if}
          <label class="pair tiny">
            <input type="checkbox" bind:checked={spread} />
            <span>name every face that matches</span>
          </label>
          <div class="naming-row">
            <button class="primary" onclick={() => settle(true)}>Save</button>
            <button class="ghost" onclick={() => settle(false)}>Cancel</button>
          </div>
        </div>
      {/if}

      {#if foundFaces.length > 0 && onShowFaces}
        <div class="face-switch">
          <button
            class:off={!showFaces}
            title={showFaces
              ? "Hide the boxes around the faces"
              : "Show the boxes around the faces"}
            aria-pressed={showFaces}
            onclick={() => onShowFaces?.(!showFaces)}
          >
            {showFaces ? "Faces on" : "Faces off"}
          </button>
        </div>
      {/if}

      {#if zoomable}
        <div class="zoomer">
          <button
            title="Zoom out"
            aria-label="Zoom out"
            disabled={isResting(zoom)}
            onclick={() => (zoom = steppedOut(zoom, boxOf()))}
          >
            −
          </button>
          <button
            title="Zoom in, or hold Ctrl and turn the wheel"
            aria-label="Zoom in"
            onclick={() => (zoom = steppedIn(zoom, boxOf()))}
          >
            +
          </button>
          <button
            title="Back to the whole picture"
            aria-label="Back to the whole picture"
            disabled={isResting(zoom)}
            onclick={() => (zoom = RESTING)}
          >
            ⟲
          </button>
        </div>
      {/if}
    </div>

    <button
      type="button"
      class="lift"
      class:lifting={visualSplitting}
      aria-label="Resize the picture"
      title="Drag to resize, or double click to reset"
      onpointerdown={grabVisualSplit}
      onpointermove={slideVisualSplit}
      onpointerup={releaseVisualSplit}
      onpointercancel={releaseVisualSplit}
      ondblclick={() => sizeVisual(VISUAL_PANE)}
      onkeydown={stretchVisualSplit}
    ></button>

    <div class="details scroll">
      <h2 class="truncate" title={entry.name}>{entry.name}</h2>

      {#if turnable}
        <section class="turning">
          <div class="verdict">
            <span class="micro flush">Which way up</span>
            {#if turned}
              <span class="badge info">{entry.rotate_by_hand ? "you turned this" : "upright"}</span>
            {/if}
          </div>

          <div class="turns">
            <button title="Turn left ( [ )" disabled={busy} onclick={() => onTurn(-1)}>↺</button>
            <button title="Turn right ( ] )" disabled={busy} onclick={() => onTurn(1)}>↻</button>
            <button title="Turn upside down ( \ )" disabled={busy} onclick={() => onTurn(2)}>
              ⤡
            </button>
          </div>

          {#if turned}
            <p class="faint note flush">{describeRotation(entry)}</p>
          {:else}
            <p class="faint note flush">This picture is copied exactly as it is.</p>
          {/if}

          {#if lossless === null && entry.rotate_lossless}
            <button class="ghost check" disabled={busy} onclick={() => checkLossless(entry.source)}>
              Can this be turned without losing quality?
            </button>
          {:else if lossless === false}
            <p class="faint note problem">
              {losslessReason ?? "this picture cannot be turned without re-encoding it"}
            </p>
            {#if !entry.reencode}
              <button class="ghost warn" disabled={busy} onclick={onReencode}>
                Turn it anyway. Re-encodes and drops the metadata
              </button>
            {/if}
          {:else if lossless === true}
            <p class="faint note">Turns here cost nothing: the pixels are never re-encoded.</p>
          {/if}

          {#if entry.rotate_by_hand}
            <button class="ghost revert" disabled={busy} onclick={onResetTurn}>
              Back to the detected orientation
            </button>
          {/if}
        </section>
      {/if}

      <section class="dates">
        <div class="verdict">
          <span class="mono taken">{entry.taken}</span>
          {#if entry.override_origin}
            <span class="badge info">you decided</span>
          {:else}
            <span class="badge {CONFIDENCE_TONE[entry.confidence]}">
              {CONFIDENCE_LABEL[entry.confidence]}
            </span>
          {/if}
        </div>

        {#if entry.override_origin}
          <p class="faint note">{entry.override_origin}</p>
        {:else if entry.flags.length > 0}
          <ul class="flags">
            {#each entry.flags as flag (flag.kind)}
              <li class:hard={flag.hard}>This date {flag.description}.</li>
            {/each}
          </ul>
        {/if}

        {#if entry.candidates.length > 0}
          <p class="micro">Take the date from</p>
          <div class="choices">
            {#each entry.candidates as candidate (candidate.provider)}
              <button
                class="choice"
                class:active={entry.taken === candidate.taken}
                disabled={busy}
                onclick={() => onChoose({ kind: "candidate", provider: candidate.provider })}
              >
                <span class="who">{candidate.provider}</span>
                <span class="mono when">{candidate.taken}</span>
                {#if candidate.provider_info}
                  <span class="faint note truncate">{candidate.provider_info}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}

        <p class="micro">Or set it by hand</p>
        <div class="manual">
          <input type="datetime-local" step="1" bind:value={manual} disabled={busy} />
          <button
            disabled={busy || !manualIsValid}
            onclick={() => onChoose({ kind: "manual", taken: manual })}
          >
            Use
          </button>
        </div>

        {#if entry.override_origin}
          <button class="ghost revert" disabled={busy} onclick={onRevert}>
            Back to the detected date
          </button>
        {/if}

      </section>

      <dl>
        <dt>Read from</dt>
        <dd>{entry.provider}{entry.provider_info ? ` · ${entry.provider_info}` : ""}</dd>

        {#if entry.subject || entry.tags.length > 0}
          <dt>Tags</dt>
          <dd class="chips">
            {#if entry.subject}
              <span class="chip subject">{entry.subject}</span>
            {/if}
            {#each entry.tags as tag (tag)}
              <span class="chip" title={tag}>{labelOf(tag)}</span>
            {/each}
          </dd>
        {/if}

        {#if entry.quality != null}
          <dt>Rated</dt>
          <dd class="chips">
            <span class="chip rated" title="How the model scored this picture, 1 to 10">
              {entry.quality.toFixed(1)}
            </span>
          </dd>
        {/if}

        <dt>Size</dt>
        <dd>{formatBytes(entry.size)}</dd>

        <dt>Source</dt>
        <dd class="mono break" title={entry.source}>{entry.source}</dd>

        <dt>Goes to</dt>
        <dd class="mono break" title={entry.destination}>{entry.destination}</dd>

        {#if entry.outcome}
          <dt>Result</dt>
          <dd>{entry.outcome}</dd>
        {/if}
      </dl>

      <div class="actions">
        <button onclick={() => onOpen(entry.source)}>Open source</button>
        <button onclick={() => onReveal(entry.source)}>Show in folder</button>
        {#if entry.outcome && entry.outcome !== "failed"}
          <button onclick={() => onOpen(entry.destination)}>Open copy</button>
        {/if}
      </div>

      {#if onLike}
        <div class="actions">
          <button
            disabled={busy}
            onclick={() => onLike(entry.source)}
            title="Looks for pictures showing something like this one. Nothing is deleted or marked."
          >
            Pictures like this one
          </button>
        </div>
        <p class="faint hint">
          Matched on what is in the picture, not on its bytes, so the answers are not copies of it.
        </p>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    padding: 2px 9px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--bg-raised);
    font-size: 11px;
    line-height: 1.6;
    white-space: nowrap;
  }

  .chip.subject {
    border-color: var(--accent-dim);
    color: var(--accent);
  }

  .chip.rated {
    font-variant-numeric: tabular-nums;
  }

  .pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-panel);
    border-left: 1px solid var(--border);
  }

  .empty {
    padding: 24px 16px;
    font-size: 12px;
  }

  .lift {
    position: relative;
    height: 6px;
    flex-shrink: 0;
    padding: 0;
    border: none;
    background: none;
    appearance: none;
    cursor: row-resize;
    touch-action: none;
  }

  .lift::after {
    content: "";
    position: absolute;
    inset-inline: 0;
    top: 2px;
    height: 2px;
    background: transparent;
  }

  .lift:hover::after,
  .lift.lifting::after,
  .lift:focus-visible::after {
    background: var(--accent);
  }

  .lift:focus-visible {
    outline: none;
  }

  .empty .hint {
    margin-top: 8px;
    font-size: 11px;
  }

  .visual {
    position: relative;
    --visual-height: 220px;
    flex-shrink: 0;
    height: var(--visual-pane, 240px);
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    padding: 10px;
  }

  .visual .stage {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    transform-origin: center;
  }

  .visual.zoomed {
    cursor: grab;
  }

  .visual.holding {
    cursor: grabbing;
  }

  .visual img,
  .visual video {
    cursor: grab;
  }

  .visual.holding img,
  .visual.holding video {
    cursor: grabbing;
  }

  .zoomer {
    position: absolute;
    right: 8px;
    bottom: 8px;
    display: flex;
    gap: 3px;
    opacity: 0.35;
    transition: opacity 120ms ease;
  }

  .visual:hover .zoomer {
    opacity: 1;
  }

  .zoomer button {
    width: 22px;
    height: 22px;
    padding: 0;
    font-size: 13px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .visual img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-sm);
  }

  .framed {
    position: relative;
    display: inline-block;
    max-width: 100%;
    max-height: 100%;
    line-height: 0;
  }

  .framed img {
    display: block;
    max-width: 100%;
    max-height: var(--visual-height);
  }

  .framed.sized {
    max-width: none;
    max-height: none;
  }

  .framed.sized img {
    width: 100%;
    height: 100%;
    max-width: none;
    max-height: none;
  }

  .face-switch {
    position: absolute;
    left: 8px;
    bottom: 8px;
    opacity: 0.35;
    transition: opacity 120ms ease;
  }

  .visual:hover .face-switch {
    opacity: 1;
  }

  .face-switch button {
    padding: 2px 8px;
    font-size: 11px;
    line-height: 1.5;
  }

  .face-switch button.off {
    opacity: 0.7;
  }

  .face {
    position: absolute;
    border: 2px solid var(--accent, #4bb3fd);
    border-radius: 2px;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.55);
    background: transparent;
    padding: 0;
    cursor: pointer;
  }

  .face.named {
    border-color: #8cf29e;
  }

  .face.naming {
    border-color: #f2b840;
    border-width: 3px;
  }

  .face .who {
    position: absolute;
    left: 0;
    top: 100%;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 1px 5px;
    font-size: 11px;
    line-height: 1.5;
    background: rgba(0, 0, 0, 0.7);
    color: #eef4f8;
    border-radius: 0 0 3px 3px;
  }

  .naming-box {
    position: absolute;
    left: 50%;
    bottom: 12px;
    transform: translateX(-50%);
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: rgba(8, 20, 34, 0.94);
    border: 1px solid var(--border, rgba(180, 210, 230, 0.2));
    box-shadow: 0 8px 26px -10px rgba(0, 0, 0, 0.8);
  }

  .naming-box input[type="text"] {
    min-width: 220px;
  }

  .known-names {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    max-width: 260px;
    max-height: 96px;
    overflow-y: auto;
  }

  .known-name {
    padding: 2px 8px;
    font-size: 11px;
    border-radius: 999px;
  }

  .known-name.picked {
    border-color: var(--accent);
    color: var(--accent);
  }

  .naming-row {
    display: flex;
    gap: 6px;
  }

  .visual video {
    max-width: 100%;
    max-height: 100%;
    border-radius: var(--radius-sm);
  }

  .visual audio {
    width: 100%;
  }

  .visual iframe {
    width: 100%;
    height: 100%;
    border: none;
    background: #fff;
    border-radius: var(--radius-sm);
  }

  .visual pre {
    width: 100%;
    height: 100%;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 11px;
    color: var(--text-dim);
  }

  .details {
    padding: 14px;
    flex: 1;
    min-height: 0;
  }

  h2 {
    font-size: 14px;
    margin-bottom: 12px;
  }

  .dates,
  .turning {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-raised);
    padding: 10px;
    margin-bottom: 6px;
  }

  .turns {
    display: flex;
    gap: 5px;
    margin: 8px 0 6px;
  }

  .turns button {
    flex: 1;
    font-size: 15px;
    line-height: 1;
    padding: 6px 0;
  }

  .flush {
    grid-column: 1;
    margin: 0;
  }

  .micro.flush {
    margin: 0;
  }

  .check,
  .warn {
    margin-top: 8px;
    width: 100%;
    text-align: left;
  }

  .warn {
    color: var(--danger);
  }

  .verdict {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .taken {
    font-size: 12px;
  }

  .flags {
    list-style: none;
    margin-top: 8px;
    display: grid;
    gap: 3px;
  }

  .flags li {
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-dim);
    padding-left: 10px;
    border-left: 2px solid var(--border-strong);
  }

  .flags li.hard {
    color: var(--text);
    border-left-color: var(--danger);
  }

  .micro {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin: 12px 0 5px;
  }

  .choices {
    display: grid;
    gap: 4px;
  }

  .choice {
    display: grid;
    grid-template-columns: 74px 1fr;
    gap: 2px 8px;
    text-align: left;
    padding: 5px 8px;
    width: 100%;
  }

  .choice.active {
    border-color: var(--accent);
    background: var(--bg-active);
  }

  .who {
    font-size: 11px;
    color: var(--text-dim);
  }

  .when {
    font-size: 11px;
  }

  .note {
    grid-column: 2;
    font-size: 10px;
  }

  .manual {
    display: flex;
    gap: 5px;
  }

  .manual input {
    flex: 1;
    min-width: 0;
  }

  .revert,
  .ask {
    margin-top: 10px;
    width: 100%;
  }

  .problem {
    color: var(--danger);
    margin-top: 8px;
  }

  dl {
    display: grid;
    grid-template-columns: 1fr;
    gap: 2px;
  }

  dt {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin-top: 10px;
  }

  dd {
    font-size: 12px;
  }

  .break {
    word-break: break-all;
    line-height: 1.4;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 18px;
  }
</style>
