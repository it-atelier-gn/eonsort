<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    cancelQualityInstall,
    cancelTagInstall,
    formatBytes,
    installQualityModel,
    installTagModel,
    qualityModelStatus,
    tagModelStatus,
    type TagModelStatus,
  } from "$lib/api";
  import type { Provider, Settings, Strategy } from "$lib/api";
  import {
    atDefaults,
    enabledIn,
    listing,
    moveBy,
    moveSource,
    rowAt,
    sourceOf,
    toggled,
  } from "$lib/sources";

  interface Props {
    settings: Settings;
    busy: boolean;
    patternError: string | null;
    tagging: boolean;
    looked: number;
    pictures: boolean;
    onChange: (settings: Settings) => void;
    onTag: (afresh: boolean) => void;
    onStopTagging: () => void;
  }

  let {
    settings,
    busy,
    patternError,
    tagging,
    looked,
    pictures,
    onChange,
    onTag,
    onStopTagging,
  }: Props = $props();

  let tagModel = $state<TagModelStatus | null>(null);
  let fetchingTags = $state(false);
  let fetched = $state<{ completed: number; total: number } | null>(null);
  let tagError = $state<string | null>(null);
  let qualityModel = $state<TagModelStatus | null>(null);
  let fetchingQuality = $state(false);
  let rated = $state<{ completed: number; total: number } | null>(null);
  let qualityError = $state<string | null>(null);
  let stops: UnlistenFn[] = [];

  onMount(async () => {
    await refreshTagModel();
    await refreshQualityModel();
    stops = await Promise.all([
      listen<{ completed: number; total: number }>("tags:fetch", (e) => (fetched = e.payload)),
      listen<number>("tags:fetched", async () => {
        fetchingTags = false;
        fetched = null;
        await refreshTagModel();
      }),
      listen<string>("tags:error", async (e) => {
        if (!fetchingTags) return;
        fetchingTags = false;
        fetched = null;
        tagError = e.payload === "cancelled" ? "Download stopped." : e.payload;
        await refreshTagModel();
      }),
      listen<{ completed: number; total: number }>("quality:fetch", (e) => (rated = e.payload)),
      listen<number>("quality:fetched", async () => {
        fetchingQuality = false;
        rated = null;
        await refreshQualityModel();
      }),
      listen<string>("quality:error", async (e) => {
        if (!fetchingQuality) return;
        fetchingQuality = false;
        rated = null;
        qualityError = e.payload === "cancelled" ? "Download stopped." : e.payload;
        await refreshQualityModel();
      }),
    ]);
  });

  onDestroy(() => stops.forEach((stop) => stop()));

  async function refreshTagModel() {
    try {
      tagModel = await tagModelStatus();
    } catch {
      tagModel = null;
    }
  }

  async function refreshQualityModel() {
    try {
      qualityModel = await qualityModelStatus();
    } catch {
      qualityModel = null;
    }
  }

  async function getQualityModel() {
    qualityError = null;
    fetchingQuality = true;
    try {
      await installQualityModel();
    } catch (e) {
      fetchingQuality = false;
      qualityError = String(e);
    }
  }

  async function getTagModel() {
    tagError = null;
    fetchingTags = true;
    try {
      await installTagModel();
    } catch (e) {
      fetchingTags = false;
      tagError = String(e);
    }
  }

  const STRATEGIES: { id: Strategy; label: string; hint: string }[] = [
    {
      id: "smart",
      label: "Weigh the evidence",
      hint: "Rejects camera reset dates and impossible dates, then prefers what two sources agree on",
    },
    { id: "oldest", label: "Oldest date wins", hint: "Ask every source, keep the earliest" },
    { id: "priority", label: "First match wins", hint: "Stop at the first source with a date" },
  ];

  async function addSources() {
    const picked = await open({ directory: true, multiple: true, title: "Add source folders" });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    const merged = [...new Set([...settings.sources, ...paths])];
    onChange({ ...settings, sources: merged });
  }

  async function chooseDestination() {
    const picked = await open({ directory: true, multiple: false, title: "Choose destination" });
    if (typeof picked === "string") {
      onChange({ ...settings, destination: picked });
    }
  }

  function clearDestination() {
    onChange({ ...settings, destination: null });
  }

  function removeSource(path: string) {
    onChange({ ...settings, sources: settings.sources.filter((s) => s !== path) });
  }

  let order = $state<Provider[]>(untrack(() => listing(settings.providers)));
  let dragging = $state<Provider | null>(null);
  let list = $state<HTMLElement | null>(null);

  $effect(() => {
    const enabled = settings.providers;
    if (enabledIn(untrack(() => order), enabled).join() !== enabled.join()) {
      order = listing(enabled);
    }
  });

  function reorder(next: Provider[]) {
    order = next;
    onChange({ ...settings, providers: enabledIn(next, settings.providers) });
  }

  function nudge(id: Provider, by: number) {
    reorder(moveBy(order, id, by));
  }

  function resetWeights() {
    onChange({ ...settings, weights: {} });
  }

  function toggleProvider(id: Provider) {
    onChange({ ...settings, providers: toggled(settings.providers, order, id) });
  }

  function grab(event: PointerEvent, id: Provider) {
    if (busy || event.button !== 0) return;
    event.preventDefault();
    const grip = event.currentTarget as HTMLElement;
    grip.setPointerCapture(event.pointerId);
    dragging = id;
  }

  function slide(event: PointerEvent) {
    if (dragging === null || list === null) return;
    const rows = [...list.children].map((row) => row.getBoundingClientRect());
    const target = order[rowAt(rows, event.clientY)];
    if (target === undefined || target === dragging) return;
    order = moveSource(order, dragging, target);
  }

  function release(event: PointerEvent) {
    if (dragging === null) return;
    const grip = event.currentTarget as HTMLElement;
    if (grip.hasPointerCapture(event.pointerId)) grip.releasePointerCapture(event.pointerId);
    dragging = null;
    reorder(order);
  }
</script>

<div class="panel scroll">
  <section>
    <div class="head">
      <label for="source-list">Source folders</label>
      <button class="ghost" onclick={addSources} disabled={busy}>+ Add</button>
    </div>
    <div id="source-list" class="list">
      {#each settings.sources as source (source)}
        <div class="item">
          <span class="mono truncate" title={source}>{source}</span>
          <button class="ghost" onclick={() => removeSource(source)} disabled={busy}>×</button>
        </div>
      {:else}
        <p class="faint empty">No sources yet. Add the folders you want to sort.</p>
      {/each}
    </div>
  </section>

  <section>
    <label for="destination">Destination</label>
    <div class="row">
      <input
        id="destination"
        type="text"
        class="mono"
        readonly
        value={settings.destination ?? ""}
        placeholder="Not chosen yet"
      />
      <button onclick={chooseDestination} disabled={busy}>Browse</button>
      {#if settings.destination}
        <button class="ghost" onclick={clearDestination} disabled={busy} title="Forget it">×</button>
      {/if}
    </div>
    {#if !settings.destination}
      <p class="faint hint">
        Only needed to copy. Scan without one to see where everything would land.
      </p>
    {/if}
  </section>

  <section>
    <label for="pattern">Folder layout</label>
    <input
      id="pattern"
      type="text"
      class="mono"
      value={settings.folder_pattern}
      disabled={busy}
      oninput={(e) => onChange({ ...settings, folder_pattern: e.currentTarget.value })}
    />
    {#if patternError}
      <p class="error">{patternError}</p>
    {:else}
      <p class="faint hint">%Y year, %m month, %d day &mdash; e.g. %Y/%m gives 2023/05</p>
    {/if}
  </section>

  <section>
    <div class="head">
      <label for="providers">Where dates come from</label>
      <button
        class="ghost"
        onclick={resetWeights}
        disabled={busy || atDefaults(settings.weights ?? {})}
        title="Put every source back to the weight it counts for by default"
      >
        Reset weights
      </button>
    </div>
    <ol id="providers" class="sources" bind:this={list}>
      {#each order as id (id)}
        {@const source = sourceOf(id)}
        {@const on = settings.providers.includes(id)}
        <li class="source" class:off={!on} class:held={dragging === id} title={source.hint}>
          <span
            class="grip"
            role="button"
            tabindex={busy ? -1 : 0}
            aria-label="Move {source.label}, with the arrow keys or by dragging"
            onpointerdown={(e) => grab(e, id)}
            onpointermove={slide}
            onpointerup={release}
            onpointercancel={release}
            onkeydown={(e) => {
              if (busy || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
              e.preventDefault();
              nudge(id, e.key === "ArrowUp" ? -1 : 1);
            }}>⠿</span
          >
          <label class="check pick">
            <input
              type="checkbox"
              checked={on}
              disabled={busy}
              onchange={() => toggleProvider(id)}
            />
            <span class="truncate">{source.label}</span>
          </label>
        </li>
      {/each}
    </ol>
  </section>


  <section>
    <label for="strategy">When sources disagree</label>
    <select
      id="strategy"
      disabled={busy}
      value={settings.strategy}
      onchange={(e) => onChange({ ...settings, strategy: e.currentTarget.value as Strategy })}
    >
      {#each STRATEGIES as strategy (strategy.id)}
        <option value={strategy.id}>{strategy.label}</option>
      {/each}
    </select>
    <p class="faint hint">{STRATEGIES.find((s) => s.id === settings.strategy)?.hint}</p>
  </section>

  <section>
    <label for="copy-settings">Copy settings</label>
    <label class="check">
      <input
        id="copy-settings"
        type="checkbox"
        checked={settings.preserve_times}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, preserve_times: e.currentTarget.checked })}
      />
      <span>Keep original timestamps</span>
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.follow_symlinks}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, follow_symlinks: e.currentTarget.checked })}
      />
      <span>Follow symbolic links</span>
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.auto_rotate}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, auto_rotate: e.currentTarget.checked })}
      />
      <span>Turn pictures upright when copying</span>
    </label>
    <p class="faint hint">
      Decided during the scan from each picture's own orientation tag, and correctable per file in
      the preview. JPEGs are turned without any loss of quality.
    </p>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.stamp_date}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, stamp_date: e.currentTarget.checked })}
      />
      <span>Write the chosen date into the copy</span>
    </label>
    <p class="faint hint">
      A JPEG that already carries a date keeps the one eonsort settled on, corrections included, so
      the sorted copy no longer disagrees with its own folder. Sources are never written to.
    </p>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.pair_companions}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, pair_companions: e.currentTarget.checked })}
      />
      <span>Keep files that belong together on one date</span>
    </label>
    <p class="faint hint">
      A live photo's video, a RAW beside its JPEG, and <code>.xmp</code>, <code>.aae</code> or
      Takeout <code>.json</code> sidecars take the date of the picture they belong to, so a pair is
      never split across two folders.
    </p>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.tag_pictures}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, tag_pictures: e.currentTarget.checked })}
      />
      <span>Look at pictures and tag them after a scan</span>
    </label>
    <p class="faint hint">
      Runs in the background once the scan has finished, so it never holds the scan up. Tags show in
      the preview, and the search box finds pictures by what is in them.
    </p>
    {#if settings.tag_pictures}
      <div class="model-line">
        {#if tagModel && !tagModel.built_in}
          <span class="faint tiny">This build was made without the tagging model.</span>
        {:else if tagModel?.present}
          <span class="faint tiny">
            Tagging model ready · {formatBytes(tagModel.total)}
          </span>
        {:else if fetchingTags}
          <button class="ghost" onclick={() => void cancelTagInstall()}>
            {fetched
              ? `Stop (${formatBytes(fetched.completed)} of ${formatBytes(fetched.total)})`
              : "Stop"}
          </button>
        {:else if tagModel}
          <button
            disabled={busy}
            onclick={getTagModel}
            title="Downloads about 780 MB of model weights, once"
          >
            Get the tagging model
          </button>
        {/if}
      </div>
      {#if tagError}
        <p class="hint error">{tagError}</p>
      {/if}

      {#if tagModel?.present}
        <div class="look-line">
          {#if tagging}
            <button class="ghost" onclick={onStopTagging}>Stop looking</button>
          {:else}
            <button disabled={!pictures} onclick={() => onTag(false)}>
              {looked > 0 ? "Look at the rest" : "Look at the pictures"}
            </button>
            {#if looked > 0}
              <button
                class="ghost"
                disabled={!pictures}
                onclick={() => onTag(true)}
                title="Forget what is known and look at every picture again"
              >
                Look again at all
              </button>
            {/if}
          {/if}
        </div>
        <p class="faint hint">
          {pictures
            ? `${looked.toLocaleString()} pictures looked at so far.`
            : "Scan first, then the pictures can be looked at."}
        </p>
      {/if}

      <label class="check">
        <input
          type="checkbox"
          checked={settings.rate_quality}
          disabled={busy}
          onchange={(e) => onChange({ ...settings, rate_quality: e.currentTarget.checked })}
        />
        <span>Also judge how good each picture looks</span>
      </label>
      <p class="faint hint">
        A second model scores each picture the way people rated photographs, and the best of them
        pick up <em>a good picture</em> and <em>a beautiful picture</em> as tags you can pick from
        the tag list.
      </p>

      {#if settings.rate_quality}
        <div class="model-line">
          {#if qualityModel && !qualityModel.built_in}
            <span class="faint tiny">This build was made without the quality model.</span>
          {:else if qualityModel?.present}
            <span class="faint tiny">
              Quality model ready · {formatBytes(qualityModel.total)}
            </span>
          {:else if fetchingQuality}
            <button class="ghost" onclick={() => void cancelQualityInstall()}>
              {rated
                ? `Stop (${formatBytes(rated.completed)} of ${formatBytes(rated.total)})`
                : "Stop"}
            </button>
          {:else if qualityModel}
            <button
              disabled={busy}
              onclick={getQualityModel}
              title="Downloads about 335 MB of model weights, once"
            >
              Get the quality model
            </button>
          {/if}
        </div>
        {#if qualityError}
          <p class="hint error">{qualityError}</p>
        {/if}
      {/if}
    {/if}
    <label class="check">
      <input
        type="checkbox"
        checked={settings.compare_hashes}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, compare_hashes: e.currentTarget.checked })}
      />
      <span>Compare contents when checking</span>
    </label>
  </section>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 14px;
    height: 100%;
    background: var(--bg-panel);
    border-right: 1px solid var(--border);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--bg-raised);
    border-radius: var(--radius-sm);
    padding: 4px 4px 4px 8px;
  }

  .item span {
    flex: 1;
    min-width: 0;
  }

  .empty {
    font-size: 12px;
    line-height: 1.4;
  }

  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .sources {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .source {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 6px;
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
  }

  .source.off {
    opacity: 0.55;
  }

  .source.held {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .grip {
    padding: 0 2px;
    color: var(--text-faint);
    cursor: grab;
    line-height: 1;
    user-select: none;
    touch-action: none;
  }

  .source.held .grip {
    cursor: grabbing;
  }

  .source .pick {
    margin: 0;
    font-size: 12px;
    min-width: 0;
  }












  @keyframes sweep {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  .row button {
    flex-shrink: 0;
  }

  .model-line,
  .look-line {
    margin-top: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 7px;
    text-transform: none;
    letter-spacing: normal;
    font-size: 13px;
    color: var(--text);
    margin-bottom: 0;
    margin-top: 6px;
    cursor: pointer;
  }

  .hint,
  .error {
    font-size: 11px;
    margin-top: 4px;
    line-height: 1.4;
  }

  .error {
    color: var(--danger);
  }

</style>
