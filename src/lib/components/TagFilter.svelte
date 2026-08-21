<script lang="ts">
  import {
    allPicked,
    labelOf,
    matching,
    pickedLabel,
    withTag,
    type TagCount,
    MAX_QUALITY,
    MIN_QUALITY,
  } from "$lib/tags";

  interface Props {
    counts: TagCount[];
    picked: string[] | null;
    least: number;
    rated: boolean;
    onPick: (picked: string[] | null) => void;
    onRate: (least: number) => void;
  }

  let { counts, picked, least, rated, onPick, onRate }: Props = $props();

  let open = $state(false);
  let needle = $state("");
  let panel = $state<HTMLElement | null>(null);

  const shown = $derived(matching(counts, needle));
  const every = $derived(allPicked(picked, counts));
  const label = $derived(pickedLabel(picked, counts));

  function away(event: MouseEvent) {
    if (!open || panel === null) return;
    if (!panel.contains(event.target as Node)) open = false;
  }
</script>

<svelte:window
  onpointerdown={away}
  onkeydown={(e) => {
    if (e.key === "Escape") open = false;
  }}
/>

<div class="filter" bind:this={panel}>
  <button
    class:narrowed={!every || least > MIN_QUALITY}
    aria-expanded={open}
    title="Show only the pictures you tick"
    onclick={() => (open = !open)}
  >
    {label}{least > MIN_QUALITY ? ` · ${least.toFixed(1)}+` : ""} ▾
  </button>

  {#if open}
    <div class="sheet">
      <input
        type="search"
        class="needle"
        placeholder="Find a tag"
        bind:value={needle}
        onkeydown={(e) => {
          if (e.key === "Escape") needle === "" ? (open = false) : (needle = "");
        }}
      />

      <div class="both">
        <button class="ghost" onclick={() => onPick(null)} disabled={every}>All</button>
        <button class="ghost" onclick={() => onPick([])} disabled={picked?.length === 0}>
          None
        </button>
        <span class="faint tiny count">{counts.length} tags</span>
      </div>

      <div class="rows">
        {#each shown as count (count.tag)}
          <label class="row">
            <input
              type="checkbox"
              checked={picked === null || picked.includes(count.tag)}
              onchange={() => onPick(withTag(picked, counts, count.tag))}
            />
            <span class="truncate name">{labelOf(count.tag)}</span>
            <span class="faint tally">{count.count.toLocaleString()}</span>
          </label>
        {:else}
          <p class="faint tiny empty">
            {counts.length === 0 ? "No pictures have been looked at yet." : "No tag by that name."}
          </p>
        {/each}
      </div>

      {#if rated}
        <div class="rating">
          <div class="both">
            <span class="faint tiny">Rated at least</span>
            <span class="tiny score">
              {least > MIN_QUALITY ? least.toFixed(1) : "any"}
            </span>
            <button
              class="ghost"
              onclick={() => onRate(MIN_QUALITY)}
              disabled={least <= MIN_QUALITY}
            >
              Clear
            </button>
          </div>
          <input
            type="range"
            min={MIN_QUALITY}
            max={MAX_QUALITY}
            step="0.1"
            value={least}
            aria-label="Lowest rating to show"
            oninput={(e) => onRate(e.currentTarget.valueAsNumber)}
          />
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .filter {
    position: relative;
  }

  .filter > button {
    white-space: nowrap;
  }

  .narrowed {
    border-color: var(--accent);
    color: var(--accent);
  }

  .sheet {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 20;
    width: 260px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    background: var(--bg-panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.45);
  }

  .needle {
    width: 100%;
  }

  .both {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .count {
    margin-left: auto;
  }

  .rows {
    display: flex;
    flex-direction: column;
    max-height: 280px;
    overflow-y: auto;
  }

  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 7px;
    padding: 3px 4px;
    margin: 0;
    border-radius: var(--radius-sm);
    text-transform: none;
    letter-spacing: normal;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .tally {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .empty {
    margin: 6px 4px;
  }

  .rating {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }

  .rating input[type="range"] {
    width: 100%;
  }

  .score {
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }
</style>
