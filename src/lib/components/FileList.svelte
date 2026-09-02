<script lang="ts">
  import { formatBytes, thumbnailFor, type EntryView } from "$lib/api";
  import { CONFIDENCE_LABEL, CONFIDENCE_TONE, hardFlags, isSuspect } from "$lib/dates";
  import { labelOf } from "$lib/tags";
  import { nextRow, nextTile } from "$lib/rows";
  import { perRow, THUMBNAIL_EDGE, TILE_GAP, TILE_SIZES, type Look } from "$lib/look";
  import ColumnHead from "./ColumnHead.svelte";
  import { sortedEntries, type Sorted } from "$lib/ordering";
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
    look: Look;
    tile: number;
    onLook: (look: Look) => void;
    sorted?: Sorted | null;
    onSort?: (id: FileColumnId) => void;
    onTile: (edge: number) => void;
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
    look,
    tile,
    onLook,
    onTile,
    sorted = null,
    onSort,
  }: Props = $props();

  const shown = $derived(sortedEntries(entries, sorted));

  let anchor = $state(-1);
  let rows: HTMLElement[] = $state([]);
  let tiles: HTMLElement[] = $state([]);
  let shelf = $state<HTMLElement | null>(null);
  let across = $state(1);
  let shots = $state<Record<string, string>>({});

  const AT_ONCE = 4;
  let asked = new Set<string>();
  let queue: string[] = [];
  let busy = 0;

  $effect(() => {
    folder;
    shots = {};
    asked = new Set();
    queue = [];
  });

  $effect(() => {
    const it = shelf;
    if (look !== "thumbnails" || it === null) return;
    const note = () => (across = perRow(it.clientWidth, tile));
    note();
    if (typeof ResizeObserver === "undefined") return;
    const watching = new ResizeObserver(note);
    watching.observe(it);
    return () => watching.disconnect();
  });

  $effect(() => {
    if (look !== "thumbnails" || shelf === null) return;
    if (typeof IntersectionObserver === "undefined") {
      for (const entry of entries) want(entry.source);
      return;
    }
    const seen = new IntersectionObserver(
      (items) => {
        for (const item of items) {
          if (!item.isIntersecting) continue;
          const source = (item.target as HTMLElement).dataset.source;
          if (source) want(source);
        }
      },
      { root: shelf, rootMargin: "300px" },
    );
    for (const node of tiles) {
      if (node) seen.observe(node);
    }
    return () => seen.disconnect();
  });

  function want(source: string) {
    if (asked.has(source)) return;
    asked.add(source);
    queue.push(source);
    pump();
  }

  function pump() {
    while (busy < AT_ONCE && queue.length > 0) {
      const source = queue.shift();
      if (source === undefined) return;
      const entry = entries.find((one) => one.source === source);
      if (entry === undefined) continue;
      busy += 1;
      void fetch(entry).finally(() => {
        busy -= 1;
        pump();
      });
    }
  }

  async function fetch(entry: EntryView) {
    try {
      const found = await thumbnailFor(entry.source, THUMBNAIL_EDGE, entry.rotate);
      shots[entry.source] =
        found.kind === "image" ? `data:image/jpeg;base64,${found.data}` : "";
    } catch {
      shots[entry.source] = "";
    }
  }

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
      onOpen(shown[index]);
      return;
    }

    const target = nextRow(event.key, index, shown.length);
    go(event, target, rows);
  }

  function step(event: KeyboardEvent, index: number) {
    if (event.key === "Enter") {
      event.preventDefault();
      onOpen(shown[index]);
      return;
    }

    const target = nextTile(event.key, index, shown.length, across);
    go(event, target, tiles);
  }

  function go(event: KeyboardEvent, target: number | null, held: HTMLElement[]) {
    if (target === null) return;

    const entry = shown[target];
    event.preventDefault();
    anchor = target;
    onSelect(entry);
    onMark([entry.source]);
    held[target]?.focus();
    held[target]?.scrollIntoView({ block: "nearest" });
  }

  function reachable(index: number, entry: EntryView): boolean {
    return selected === null ? index === 0 : selected.source === entry.source;
  }
</script>

<div class="list scroll" bind:this={shelf}>
  {#if folder !== null}
    <div class="look-bar">
      <div class="ways">
        <button class="ghost" class:on={look === "details"} onclick={() => onLook("details")}>
          Details
        </button>
        <button
          class="ghost"
          class:on={look === "thumbnails"}
          onclick={() => onLook("thumbnails")}
        >
          Thumbnails
        </button>
      </div>
      {#if look === "thumbnails"}
        <div class="ways">
          {#each TILE_SIZES as size (size.id)}
            <button
              class="ghost"
              class:on={tile === size.edge}
              title="{size.label} thumbnails"
              onclick={() => onTile(size.edge)}
            >
              {size.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if folder === null}
    <p class="placeholder faint">Pick a folder on the left to see what lands in it.</p>
  {:else if entries.length === 0}
    <p class="placeholder faint">No files land directly in this folder.</p>
  {:else if look === "thumbnails"}
    <div
      class="tiles"
      role="listbox"
      aria-label="Files in this folder"
      style="--tile: {tile}px; --gap: {TILE_GAP}px"
    >
      {#each shown as entry, index (entry.source)}
        {@const shot = shots[entry.source]}
        {@const tag = status(entry)}
        <div
          bind:this={tiles[index]}
          data-source={entry.source}
          class="tile"
          role="option"
          aria-selected={selected?.source === entry.source}
          tabindex={reachable(index, entry) ? 0 : -1}
          title={entry.destination}
          class:selected={selected?.source === entry.source}
          class:marked={marked.includes(entry.source)}
          onclick={(event) => click(event, entry, index)}
          ondblclick={() => onOpen(entry)}
          onkeydown={(event) => step(event, index)}
        >
          <div class="shot">
            {#if shot}
              <img src={shot} alt={entry.name} />
            {:else if shot === ""}
              <span class="faint tiny nothing">no picture</span>
            {/if}
            {#if tag}
              <span class="badge {tag.tone} stamp">{tag.label}</span>
            {/if}
          </div>
          <span class="caption truncate">{entry.name}</span>
          <span class="under faint mono truncate">{entry.taken}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="grid" role="grid" aria-label="Files in this folder">
      <ColumnHead set={FILE_COLUMNS} {order} {grid} {onReorder} {onResize} {sorted} {onSort} />
      {#each shown as entry, index (entry.source)}
        {@const tag = status(entry)}
        <div
          bind:this={rows[index]}
          class="row"
          role="row"
          tabindex={reachable(index, entry) ? 0 : -1}
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

  .look-bar {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border);
  }

  .ways {
    display: flex;
    gap: 3px;
  }

  .ways button {
    padding: 2px 8px;
    font-size: 11px;
  }

  .ways button.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--tile), 1fr));
    gap: var(--gap);
    padding: var(--gap);
    align-content: start;
  }

  .tile {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 5px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
  }

  .tile:hover {
    background: var(--bg-hover);
  }

  .tile.marked {
    background: var(--bg-hover);
  }

  .tile.selected {
    background: var(--bg-active);
    border-color: var(--accent);
  }

  .shot {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    aspect-ratio: 1;
    overflow: hidden;
    border-radius: var(--radius-sm);
    background: var(--bg-panel);
  }

  .shot img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .stamp {
    position: absolute;
    left: 3px;
    bottom: 3px;
  }

  .nothing {
    padding: 0 4px;
    text-align: center;
  }

  .caption {
    color: var(--text);
  }

  .under {
    font-size: 10px;
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
