<script lang="ts">
  import { formatBytes, type EntryView } from "$lib/api";
  import { CONFIDENCE_LABEL, CONFIDENCE_TONE, hardFlags, isSuspect } from "$lib/dates";
  import { labelOf } from "$lib/tags";
  import { nextRow } from "$lib/rows";
  import ColumnHead from "./ColumnHead.svelte";
  import {
    FILE_COLUMNS,
    template,
    widthOf,
    type ColumnWidths,
    type FileColumnId,
  } from "$lib/columns";

  interface Props {
    entries: EntryView[];
    folder: string | null;
    selected: EntryView | null;
    marked: string[];
    order: FileColumnId[];
    widths: ColumnWidths<FileColumnId>;
    onSelect: (entry: EntryView) => void;
    onMark: (sources: string[]) => void;
    onOpen: (entry: EntryView) => void;
    onReorder: (order: FileColumnId[]) => void;
    onResize: (id: FileColumnId, width: number | null) => void;
  }

  let {
    entries,
    folder,
    selected,
    marked,
    order,
    widths,
    onSelect,
    onMark,
    onOpen,
    onReorder,
    onResize,
  }: Props = $props();

  let anchor = $state(-1);
  let rows: HTMLElement[] = $state([]);

  const grid = $derived(
    template(FILE_COLUMNS, order, {
      name: widths.name ?? 0,
      date: widths.date ?? widthOf(FILE_COLUMNS, "date", entries.map((entry) => entry.taken)),
      from: widths.from ?? widthOf(FILE_COLUMNS, "from", entries.map(provider)),
      tags: widths.tags ?? widthOf(FILE_COLUMNS, "tags", entries.map(worn)),
      rated: widths.rated ?? widthOf(FILE_COLUMNS, "rated", entries.map(rated)),
      size:
        widths.size ??
        widthOf(
          FILE_COLUMNS,
          "size",
          entries.map((entry) => formatBytes(entry.size)),
        ),
      status:
        widths.status ??
        widthOf(
          FILE_COLUMNS,
          "status",
          entries.map((entry) => status(entry)?.label ?? ""),
        ),
    }),
  );

  function status(entry: EntryView): { label: string; tone: string } | null {
    if (entry.outcome === "failed") return { label: "failed", tone: "danger" };
    if (entry.outcome === "duplicate") return { label: "kept as copy", tone: "warn" };
    if (entry.outcome === "already present") return { label: "already there", tone: "ok" };
    if (entry.outcome === "copied") return { label: "copied", tone: "ok" };
    if (entry.destination_exists) return { label: "name taken", tone: "warn" };
    return null;
  }

  function provider(entry: EntryView): string {
    return entry.override_origin ? "you" : entry.provider;
  }

  function worn(entry: EntryView): string {
    return entry.tags.map(labelOf).join(", ");
  }

  function rated(entry: EntryView): string {
    return entry.quality == null ? "" : entry.quality.toFixed(1);
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

  function walk(event: KeyboardEvent, index: number) {
    if (event.key === "Enter") {
      event.preventDefault();
      onOpen(entries[index]);
      return;
    }

    const target = nextRow(event.key, index, entries.length);
    if (target === null) return;

    const entry = entries[target];
    event.preventDefault();
    anchor = target;
    onSelect(entry);
    onMark([entry.source]);
    rows[target]?.focus();
    rows[target]?.scrollIntoView({ block: "nearest" });
  }
</script>

<div class="list scroll">
  {#if folder === null}
    <p class="placeholder faint">Pick a folder on the left to see what lands in it.</p>
  {:else if entries.length === 0}
    <p class="placeholder faint">No files land directly in this folder.</p>
  {:else}
    <div class="grid" role="grid" aria-label="Files in this folder">
      <ColumnHead set={FILE_COLUMNS} {order} {grid} {onReorder} {onResize} />
      {#each entries as entry, index (entry.source)}
        {@const tag = status(entry)}
        <div
          bind:this={rows[index]}
          class="row"
          role="row"
          tabindex={(selected === null ? index === 0 : selected.source === entry.source) ? 0 : -1}
          style="grid-template-columns: {grid}"
          class:selected={selected?.source === entry.source}
          class:marked={marked.includes(entry.source)}
          onclick={(event) => click(event, entry, index)}
          ondblclick={() => onOpen(entry)}
          onkeydown={(event) => walk(event, index)}
        >
          {#each order as id (id)}
            {#if id === "name"}
              <span class="cell truncate name" role="gridcell" title={entry.destination}>
                {entry.name}
              </span>
            {:else if id === "date"}
              <span class="cell mono nowrap dim date" role="gridcell" title={dateTitle(entry)}>
                <span
                  class="dot {entry.override_origin ? 'info' : CONFIDENCE_TONE[entry.confidence]}"
                  class:alarm={isSuspect(entry)}
                ></span>
                <span class="truncate">{entry.taken}</span>
              </span>
            {:else if id === "from"}
              <span
                class="cell truncate dim nowrap"
                role="gridcell"
                title={entry.provider_info ?? entry.provider}
              >
                {provider(entry)}
              </span>
            {:else if id === "tags"}
              <span class="cell truncate dim nowrap" role="gridcell" title={worn(entry)}>
                {worn(entry)}
              </span>
            {:else if id === "rated"}
              <span class="cell right mono nowrap dim" role="gridcell">
                {rated(entry)}
              </span>
            {:else if id === "size"}
              <span class="cell right mono nowrap dim" role="gridcell">
                {formatBytes(entry.size)}
              </span>
            {:else}
              <span class="cell right" role="gridcell">
                {#if tag}
                  <span class="badge {tag.tone}">{tag.label}</span>
                {/if}
              </span>
            {/if}
          {/each}
        </div>
      {/each}
    </div>
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

  .row {
    display: grid;
    align-items: center;
    gap: 6px;
    padding-block: 5px;
    padding-inline: 8px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    cursor: pointer;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .row.marked {
    background: var(--bg-hover);
  }

  .row.selected {
    background: var(--bg-active);
  }

  .cell {
    min-width: 0;
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
