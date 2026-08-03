<script lang="ts">
  import { formatBytes, type EntryView } from "$lib/api";

  interface Props {
    entries: EntryView[];
    folder: string | null;
    selected: EntryView | null;
    onSelect: (entry: EntryView) => void;
    onOpen: (entry: EntryView) => void;
  }

  let { entries, folder, selected, onSelect, onOpen }: Props = $props();

  function status(entry: EntryView): { label: string; tone: string } | null {
    if (entry.outcome === "failed") return { label: "failed", tone: "danger" };
    if (entry.outcome === "duplicate") return { label: "kept as copy", tone: "warn" };
    if (entry.outcome === "already present") return { label: "already there", tone: "ok" };
    if (entry.outcome === "copied") return { label: "copied", tone: "ok" };
    if (entry.destination_exists) return { label: "name taken", tone: "warn" };
    return null;
  }
</script>

<div class="list scroll">
  {#if folder === null}
    <p class="placeholder faint">Pick a folder on the left to see what lands in it.</p>
  {:else if entries.length === 0}
    <p class="placeholder faint">No files land directly in this folder.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Date</th>
          <th>From</th>
          <th class="right">Size</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each entries as entry (entry.source)}
          {@const tag = status(entry)}
          <tr
            class:selected={selected?.source === entry.source}
            onclick={() => onSelect(entry)}
            ondblclick={() => onOpen(entry)}
          >
            <td class="truncate name" title={entry.destination}>{entry.name}</td>
            <td class="mono nowrap dim">{entry.taken}</td>
            <td class="dim nowrap" title={entry.provider_info ?? entry.provider}>
              {entry.provider}
            </td>
            <td class="right mono nowrap dim">{formatBytes(entry.size)}</td>
            <td class="right">
              {#if tag}
                <span class="badge {tag.tone}">{tag.label}</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .list {
    height: 100%;
    background: var(--bg-base);
  }

  .placeholder {
    padding: 24px 16px;
    font-size: 12px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--bg-panel);
    text-align: left;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    font-weight: 600;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }

  th:nth-child(2),
  th:nth-child(3),
  th:nth-child(4) {
    width: 130px;
  }

  th:nth-child(3) {
    width: 90px;
  }

  th:nth-child(5) {
    width: 110px;
  }

  td {
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }

  tr {
    cursor: pointer;
  }

  tbody tr:hover {
    background: var(--bg-hover);
  }

  tbody tr.selected {
    background: var(--bg-active);
  }

  .name {
    color: var(--text);
  }

  .right {
    text-align: right;
  }

  .nowrap {
    white-space: nowrap;
  }
</style>
