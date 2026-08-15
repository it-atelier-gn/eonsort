<script lang="ts">
  import { rangeLabel, type TimeRange } from "$lib/viz/range";

  interface Props {
    scopes: TimeRange[];
    shown: number;
    total: number;
    onPop: (depth: number) => void;
  }

  let { scopes, shown, total, onPop }: Props = $props();
</script>

<div class="scope">
  <nav aria-label="Selected time range">
    <button class="crumb" onclick={() => onPop(0)}>All time</button>
    {#each scopes as range, depth (depth)}
      <span class="sep" aria-hidden="true">›</span>
      <button
        class="crumb"
        class:current={depth === scopes.length - 1}
        onclick={() => onPop(depth + 1)}
      >
        {rangeLabel(range)}
      </button>
    {/each}
  </nav>

  <span class="count">
    {shown.toLocaleString()} of {total.toLocaleString()}
    {total === 1 ? "file" : "files"}
  </span>

  <button class="ghost" onclick={() => onPop(scopes.length - 1)}>Back</button>
  <button class="ghost" onclick={() => onPop(0)}>Show all</button>
</div>

<style>
  .scope {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 14px;
    background: var(--bg-raised);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }

  nav {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
  }

  .crumb {
    padding: 2px 7px;
    font-size: 12px;
    color: var(--text-dim);
    background: transparent;
    border-color: transparent;
    white-space: nowrap;
  }

  .crumb:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }

  .crumb.current {
    color: var(--text);
    border-color: var(--accent);
    background: var(--bg-active);
  }

  .sep {
    color: var(--text-faint);
  }

  .count {
    color: var(--text-dim);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
