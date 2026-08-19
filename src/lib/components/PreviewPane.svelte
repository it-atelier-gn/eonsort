<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import {
    formatBytes,
    probeRotation,
    type DateChoice,
    type EntryView,
    type Preview,
  } from "$lib/api";
  import { CONFIDENCE_LABEL, CONFIDENCE_TONE, fromInputValue, toInputValue } from "$lib/dates";
  import { TRANSFORM_CSS, canTurn, describeRotation, swapsAxes } from "$lib/rotate";
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
    onChoose: (choice: DateChoice) => void;
    onRevert: () => void;
    onTurn: (quarterTurns: number) => void;
    onResetTurn: () => void;
    onReencode: () => void;
  }

  let {
    entry,
    preview,
    loading,
    busy,
    onOpen,
    onReveal,
    onChoose,
    onRevert,
    onTurn,
    onResetTurn,
    onReencode,
  }: Props = $props();

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
  const imageStyle = $derived(
    entry === null || entry.rotate === "none"
      ? ""
      : `image-orientation: none; transform: ${TRANSFORM_CSS[entry.rotate]};` +
          (swapsAxes(entry.rotate) ? " max-width: var(--visual-height); max-height: 100%;" : ""),
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

<aside class="pane">
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
      <div class="stage" style={transformOf(zoom) ? `transform: ${transformOf(zoom)}` : ""}>
      {#if loading}
        <p class="faint">Loading preview…</p>
      {:else if preview?.kind === "image"}
        <img src="data:{preview.mime};base64,{preview.data}" alt={entry.name} style={imageStyle} />
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
            title="Zoom in — or hold Ctrl and turn the wheel"
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
                Turn it anyway — re-encodes and drops the metadata
              </button>
            {/if}
          {:else if lossless === true}
            <p class="faint note">Turns here cost nothing — the pixels are never re-encoded.</p>
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
          <dd>{[entry.subject, ...entry.tags].filter(Boolean).join(" · ")}</dd>
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
    </div>
  {/if}
</aside>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-panel);
    border-left: 1px solid var(--border);
  }

  .empty {
    padding: 24px 16px;
    font-size: 12px;
  }

  .empty .hint {
    margin-top: 8px;
    font-size: 11px;
  }

  .visual {
    position: relative;
    --visual-height: 220px;
    flex-shrink: 0;
    height: 240px;
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
