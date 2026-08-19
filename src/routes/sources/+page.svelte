<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getSettings, saveSettings, type Provider, type Settings } from "$lib/api";
  import {
    atDefaults,
    enabledIn,
    isProvider,
    listing,
    moveSource,
    sourceOf,
    toggled,
    weightOf,
    withWeight,
    MAX_WEIGHT,
    MIN_WEIGHT,
    SOURCES,
  } from "$lib/sources";

  let settings = $state<Settings | null>(null);
  let order = $state<Provider[]>(listing([]));
  let error = $state<string | null>(null);
  let dragging = $state<Provider | null>(null);
  let over = $state<Provider | null>(null);
  let stop: UnlistenFn | null = null;

  onMount(async () => {
    await load();
    stop = await listen<Settings>("settings:changed", (event) => {
      settings = event.payload;
      order = listing(event.payload.providers);
    });
  });

  onDestroy(() => stop?.());

  async function load() {
    try {
      const held = await getSettings();
      settings = held;
      order = listing(held.providers);
    } catch (e) {
      error = String(e);
    }
  }

  async function keep(next: Settings) {
    settings = next;
    try {
      await saveSettings(next);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  function reorder(from: Provider, to: Provider) {
    if (settings === null) return;
    order = moveSource(order, from, to);
    keep({ ...settings, providers: enabledIn(order, settings.providers) });
  }

  function nudge(id: Provider, by: number) {
    const target = order[order.indexOf(id) + by];
    if (target) reorder(id, target);
  }

  function toggle(id: Provider) {
    if (settings === null) return;
    keep({ ...settings, providers: toggled(settings.providers, order, id) });
  }

  function weigh(id: Provider, weight: number) {
    if (settings === null) return;
    keep({ ...settings, weights: withWeight(settings.weights ?? {}, id, weight) });
  }

  function reset() {
    if (settings === null) return;
    keep({ ...settings, weights: {} });
  }

  function drop(event: DragEvent, id: Provider) {
    event.preventDefault();
    const held = dragging ?? event.dataTransfer?.getData("text/plain");
    dragging = null;
    over = null;
    if (isProvider(held) && held !== id) reorder(held, id);
  }
</script>

<div class="page">
  <header>
    <h1>Where dates come from</h1>
    <p class="faint">
      Every source is asked in the order below. The order decides who wins under
      <strong>First match wins</strong>, and the weight decides who wins under
      <strong>Weigh the evidence</strong>. A source with no tick is never asked.
    </p>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if settings === null}
    <p class="faint">Reading the settings…</p>
  {:else}
    <ol class="sources">
      {#each order as id (id)}
        {@const source = sourceOf(id)}
        {@const on = settings.providers.includes(id)}
        <li
          class="source"
          class:off={!on}
          class:over={over === id && dragging !== id}
          class:held={dragging === id}
          draggable="true"
          ondragstart={(e) => {
            dragging = id;
            e.dataTransfer?.setData("text/plain", id);
            if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
          }}
          ondragend={() => {
            dragging = null;
            over = null;
          }}
          ondragover={(e) => {
            e.preventDefault();
            over = id;
          }}
          ondragleave={() => (over = over === id ? null : over)}
          ondrop={(e) => drop(e, id)}
        >
          <span class="grip" title="Drag to reorder" aria-hidden="true"
            >⠿</span
          >

          <label class="pick">
            <input type="checkbox" checked={on} onchange={() => toggle(id)} />
            <span>
              <span class="label">{source.label}</span>
              <span class="hint faint">{source.hint}</span>
            </span>
          </label>

          <span class="weigh">
            <input
              type="range"
              min={MIN_WEIGHT}
              max={MAX_WEIGHT}
              value={weightOf(settings.weights ?? {}, id)}
              disabled={!on}
              aria-label="Weight for {source.label}"
              oninput={(e) => weigh(id, e.currentTarget.valueAsNumber)}
            />
            <input
              type="number"
              class="number"
              min={MIN_WEIGHT}
              max={MAX_WEIGHT}
              value={weightOf(settings.weights ?? {}, id)}
              disabled={!on}
              aria-label="Weight for {source.label}, as a number"
              onchange={(e) => weigh(id, e.currentTarget.valueAsNumber)}
            />
          </span>

          <span class="move">
            <button
              type="button"
              aria-label="Move {source.label} up"
              disabled={order.indexOf(id) === 0}
              onclick={() => nudge(id, -1)}>↑</button
            >
            <button
              type="button"
              aria-label="Move {source.label} down"
              disabled={order.indexOf(id) === order.length - 1}
              onclick={() => nudge(id, 1)}>↓</button
            >
          </span>
        </li>
      {/each}
    </ol>

    <footer>
      <button
        type="button"
        onclick={reset}
        disabled={atDefaults(settings.weights ?? {})}
        title="Put every weight back where it started"
      >
        Reset the weights
      </button>
      <span class="faint count">
        {settings.providers.length} of {SOURCES.length} sources in use
      </span>
    </footer>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100vh;
    padding: 16px;
    background: var(--bg-base);
    color: var(--text);
    overflow-y: auto;
  }

  h1 {
    font-size: 15px;
    margin: 0 0 4px;
  }

  header p {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
  }

  .error {
    margin: 0;
    font-size: 12px;
    color: var(--danger);
  }

  .sources {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .source {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-panel);
  }

  .source.off {
    opacity: 0.55;
  }

  .source.held {
    opacity: 0.4;
  }

  .source.over {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .grip {
    cursor: grab;
    color: var(--text-faint);
    user-select: none;
  }

  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    cursor: pointer;
  }

  .label {
    display: block;
    font-size: 12px;
  }

  .hint {
    display: block;
    font-size: 11px;
  }

  .weigh {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .weigh input[type="range"] {
    width: 120px;
  }

  .number {
    width: 56px;
    font-variant-numeric: tabular-nums;
  }

  .move {
    display: flex;
    gap: 2px;
  }

  .move button {
    padding: 2px 6px;
    line-height: 1;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .count {
    font-size: 11px;
  }
</style>
