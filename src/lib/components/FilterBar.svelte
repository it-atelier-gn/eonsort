<script lang="ts">
  import TagFilter from "$lib/components/TagFilter.svelte";
  import { pickedLabel, type TagCount, MAX_QUALITY, MIN_QUALITY } from "$lib/tags";
  import { dayValue, rangeLabel, rangeOfDays, type TimeRange } from "$lib/viz/range";

  interface Props {
    range: TimeRange | null;
    counts: TagCount[];
    picked: string[] | null;
    least: number;
    rated: boolean;
    shown: number;
    total: number;
    onRange: (range: TimeRange | null) => void;
    onPick: (picked: string[] | null) => void;
    onRate: (least: number) => void;
  }

  let {
    range,
    counts,
    picked,
    least,
    rated,
    shown,
    total,
    onRange,
    onPick,
    onRate,
  }: Props = $props();

  const from = $derived(range === null ? "" : dayValue(range.from));
  const to = $derived(range === null ? "" : dayValue(range.to - 1));
  const narrowed = $derived(range !== null || picked !== null || least > MIN_QUALITY);

  function reday(first: string, last: string) {
    onRange(rangeOfDays(first, last));
  }

  function clear() {
    onRange(null);
    onPick(null);
    onRate(MIN_QUALITY);
  }
</script>

<div class="filters">
  <span class="what">Showing</span>

  <label class="pair">
    <span class="faint tiny">from</span>
    <input type="date" value={from} onchange={(e) => reday(e.currentTarget.value, to)} />
  </label>
  <label class="pair">
    <span class="faint tiny">to</span>
    <input type="date" value={to} onchange={(e) => reday(from, e.currentTarget.value)} />
  </label>
  {#if range}
    <button class="ghost" onclick={() => onRange(null)} title="Every date again">
      {rangeLabel(range)} ×
    </button>
  {/if}

  {#if counts.length > 0}
    <TagFilter {counts} {picked} {onPick} />
    {#if picked !== null}
      <button class="ghost" onclick={() => onPick(null)} title="Every tag again">
        {pickedLabel(picked, counts)} ×
      </button>
    {/if}
  {/if}

  {#if rated}
    <label class="pair rating">
      <span class="faint tiny">rated</span>
      <input
        type="range"
        min={MIN_QUALITY}
        max={MAX_QUALITY}
        step="0.1"
        value={least}
        aria-label="Lowest rating to show"
        oninput={(e) => onRate(e.currentTarget.valueAsNumber)}
      />
      <span class="mark" class:on={least > MIN_QUALITY}>
        {least > MIN_QUALITY ? `${least.toFixed(1)}+` : "any"}
      </span>
    </label>
    {#if least > MIN_QUALITY}
      <button class="ghost" onclick={() => onRate(MIN_QUALITY)} title="Any rating again">×</button>
    {/if}
  {/if}

  <span class="count faint">
    {shown.toLocaleString()} of {total.toLocaleString()}
    {total === 1 ? "file" : "files"}
  </span>

  <button class="ghost" onclick={clear} disabled={!narrowed}>Reset</button>
</div>

<style>
  .filters {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 6px 14px;
    background: var(--bg-raised);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }

  .what {
    color: var(--text-dim);
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.04em;
  }

  .pair {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 0;
    text-transform: none;
    letter-spacing: normal;
    font-size: 12px;
    color: var(--text);
  }

  .pair input[type="date"] {
    width: auto;
    padding: 3px 6px;
    font-size: 12px;
  }

  .rating input[type="range"] {
    width: 110px;
  }

  .mark {
    font-variant-numeric: tabular-nums;
    color: var(--text-dim);
    min-width: 34px;
  }

  .mark.on {
    color: var(--accent);
  }

  .count {
    margin-left: auto;
    white-space: nowrap;
  }
</style>
