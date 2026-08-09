<script lang="ts">
  import { formatBytes, type EntryView } from "$lib/api";
  import { CONFIDENCE_LABEL, CONFIDENCE_TONE, hardFlags, isSuspect } from "$lib/dates";

  interface Props {
    entries: EntryView[];
    folder: string | null;
    selected: EntryView | null;
    marked: string[];
    onSelect: (entry: EntryView) => void;
    onMark: (sources: string[]) => void;
    onOpen: (entry: EntryView) => void;
  }

  let { entries, folder, selected, marked, onSelect, onMark, onOpen }: Props = $props();

  let anchor = $state(-1);

  function status(entry: EntryView): { label: string; tone: string } | null {
    if (entry.outcome === "failed") return { label: "failed", tone: "danger" };
    if (entry.outcome === "duplicate") return { label: "kept as copy", tone: "warn" };
    if (entry.outcome === "already present") return { label: "already there", tone: "ok" };
    if (entry.outcome === "copied") return { label: "copied", tone: "ok" };
    if (entry.destination_exists) return { label: "name taken", tone: "warn" };
    return null;
  }

  function dateTitle(entry: EntryView): string {
    if (entry.override_origin) return `You decided: ${entry.override_origin}`;
    const flags = hardFlags(entry);
    if (flags.length === 0) return CONFIDENCE_LABEL[entry.confidence];
    return flags.map((flag) => `This date ${flag.description}.`).join("\n");
  }

  function click(event: MouseEvent, entry: EntryView, index: number) {
    onSelect(entry);

    if (event.shiftKey && anchor >= 0) {
      const [from, to] = anchor < index ? [anchor, index] : [index, anchor];
      onMark(entries.slice(from, to + 1).map((e) => e.source));
      return;
    }

    anchor = index;

    if (event.ctrlKey || event.metaKey) {
      const next = marked.includes(entry.source)
        ? marked.filter((source) => source !== entry.source)
        : [...marked, entry.source];
      onMark(next);
      return;
    }

    onMark([entry.source]);
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
        {#each entries as entry, index (entry.source)}
          {@const tag = status(entry)}
          <tr
            class:selected={selected?.source === entry.source}
            class:marked={marked.includes(entry.source)}
            onclick={(event) => click(event, entry, index)}
            ondblclick={() => onOpen(entry)}
          >
            <td class="truncate name" title={entry.destination}>{entry.name}</td>
            <td class="mono nowrap dim date" title={dateTitle(entry)}>
              <span
                class="dot {entry.override_origin ? 'info' : CONFIDENCE_TONE[entry.confidence]}"
                class:alarm={isSuspect(entry)}
              ></span>
              {entry.taken}
            </td>
            <td class="dim nowrap" title={entry.provider_info ?? entry.provider}>
              {entry.override_origin ? "you" : entry.provider}
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

  tbody tr.marked {
    background: var(--bg-hover);
  }

  tbody tr.selected {
    background: var(--bg-active);
  }

  .name {
    color: var(--text);
  }

  .date {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-faint);
  }

  .dot.ok {
    background: var(--ok);
  }

  .dot.warn {
    background: var(--warn);
  }

  .dot.danger {
    background: var(--danger);
  }

  .dot.info {
    background: var(--accent);
  }

  .dot.alarm {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 25%, transparent);
  }

  .right {
    text-align: right;
  }

  .nowrap {
    white-space: nowrap;
  }
</style>
