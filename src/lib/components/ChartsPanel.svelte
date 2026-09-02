<script lang="ts">
  import type { EntryView } from "$lib/api";
  import { formatBytes } from "$lib/api";
  import {
    formatYear,
    heatStep,
    hourHistogram,
    midnightShare,
    niceTicks,
    providerCounts,
    readingCounts,
    span,
    topFolders,
    PROVIDER_COLOUR,
    PROVIDER_LABEL,
    READING_COLOUR,
    READING_LABEL,
    READING_NOTE,
    HEAT_STEPS,
  } from "$lib/viz/charts";
  import {
    heatGrid,
    planRange,
    rangeLabel,
    selectionRange,
    CELL_UNIT,
    type GridLevel,
    type HeatCell,
    type HeatRow,
    type TimeRange,
  } from "$lib/viz/range";

  interface Props {
    entries: EntryView[];
    range: TimeRange | null;
    onDrill: (range: TimeRange) => void;
  }

  let { entries, range, onDrill }: Props = $props();

  let showCounts = $state(false);
  let dragCell = $state<HeatCell | null>(null);
  let dragFrom = $state<number | null>(null);
  let dragTo = $state<number | null>(null);

  const HEAT_CAPTION: Record<GridLevel, string> = {
    years:
      "One square per month, darker to lighter as the count grows. Each row is a year, running January to December.",
    months: "One square per day. Each row is a month, running the 1st to the 31st.",
    days: "One square per hour. Each row is a day, running midnight to 11pm.",
  };

  const scope = $derived(range ?? planRange(entries));
  const grid = $derived(scope === null ? null : heatGrid(entries, scope));
  const dragging = $derived(dragFrom !== null && dragTo !== null);
  const dragLow = $derived(Math.min(dragFrom ?? 0, dragTo ?? 0));
  const dragHigh = $derived(Math.max(dragFrom ?? 0, dragTo ?? 0));

  function startDrag(cell: HeatCell) {
    dragCell = cell;
    dragFrom = cell.index;
    dragTo = cell.index;
  }

  function extendDrag(cell: HeatCell) {
    if (dragFrom !== null) dragTo = cell.index;
  }

  function endDrag() {
    const started = dragCell;
    const from = dragFrom;
    const to = dragTo;
    dragCell = null;
    dragFrom = null;
    dragTo = null;

    if (grid === null || from === null || to === null) return;
    if (from === to) {
      if (started !== null && started.count > 0) onDrill({ from: started.from, to: started.to });
      return;
    }

    const next = selectionRange(grid, from, to);
    if (next !== null) onDrill(next);
  }

  function drillRow(row: HeatRow) {
    if (row.count > 0) onDrill({ from: row.from, to: row.to });
  }

  const hours = $derived(hourHistogram(entries));
  const hourMax = $derived(Math.max(1, ...hours.map((h) => h.files)));
  const hourTicks = $derived(niceTicks(hourMax, 4));
  const midnight = $derived(midnightShare(hours));
  const providers = $derived(providerCounts(entries));
  const providerMax = $derived(Math.max(1, ...providers.map((p) => p.files)));
  const readings = $derived(readingCounts(entries));
  const readingMax = $derived(Math.max(1, ...readings.map((r) => r.files)));
  const folders = $derived(topFolders(entries, 12));
  const folderMax = $derived(Math.max(1, ...folders.map((f) => f.files)));
  const reach = $derived(span(entries));
  const bytes = $derived(entries.reduce((sum, entry) => sum + entry.size, 0));
  const wrong = $derived(readings.find((r) => r.reading === "wrong")?.files ?? 0);
  const midnightPercent = $derived(Math.round(midnight * 100));
</script>

<svelte:window onpointerup={endDrag} />

<div class="charts scroll">
  {#if entries.length === 0 && range !== null}
    <p class="placeholder faint">
      Nothing was taken in {rangeLabel(range)}. Step back up to a wider range.
    </p>
  {:else if entries.length === 0}
    <p class="placeholder faint">Run a scan and these charts will describe what it found.</p>
  {:else}
    <div class="tiles">
      <div class="tile">
        <span class="value">{entries.length.toLocaleString()}</span>
        <span class="label">{range === null ? "files planned" : "files in this range"}</span>
      </div>
      <div class="tile">
        <span class="value">{formatYear(reach.first)}–{formatYear(reach.last)}</span>
        <span class="label">{reach.years} {reach.years === 1 ? "year" : "years"} covered</span>
      </div>
      <div class="tile">
        <span class="value">{formatBytes(bytes)}</span>
        <span class="label">to copy</span>
      </div>
      <div class="tile" class:alarm={wrong > 0}>
        <span class="value">{wrong.toLocaleString()}</span>
        <span class="label">dates look wrong</span>
      </div>
    </div>

    {#if grid}
      <figure>
        <figcaption>
          <h3>When your files were made</h3>
          <p>{HEAT_CAPTION[grid.level]}</p>
        </figcaption>

        <div class="heat" style:--columns={grid.columns.length}>
          <div class="band">
            <span></span>
            {#each grid.columns as column, index (index)}
              <span>{grid.level === "years" ? column.charAt(0) : column}</span>
            {/each}
          </div>
          {#each grid.rows as row (row.from)}
            <div class="band">
              <button
                class="stub"
                disabled={row.count === 0}
                onclick={() => drillRow(row)}
                title="{row.label}: {row.count.toLocaleString()} {row.count === 1
                  ? 'file'
                  : 'files'}"
              >
                {row.label}
              </button>
              {#each row.cells as cell, column (column)}
                {@const fill = cell === null ? null : heatStep(cell.count, grid.max)}
                {#if cell === null}
                  <span class="cell gone"></span>
                {:else}
                  <button
                    class="cell"
                    class:empty={fill === null}
                    class:picked={dragging && cell.index >= dragLow && cell.index <= dragHigh}
                    style:background={fill ?? "transparent"}
                    title="{cell.label}: {cell.count.toLocaleString()} {cell.count === 1
                      ? 'file'
                      : 'files'}"
                    onpointerdown={(event) => {
                      event.preventDefault();
                      startDrag(cell);
                    }}
                    onpointerenter={() => extendDrag(cell)}
                  >
                    {#if showCounts && cell.count > 0}<b>{cell.count}</b>{/if}
                  </button>
                {/if}
              {/each}
            </div>
          {/each}
        </div>

        <div class="foot">
          <div class="ramp">
            <span class="faint">fewer</span>
            {#each HEAT_STEPS as step (step)}<i style:background={step}></i>{/each}
            <span class="faint">more ({grid.max.toLocaleString()})</span>
          </div>
          <button class="ghost" onclick={() => (showCounts = !showCounts)}>
            {showCounts ? "Hide numbers" : "Show numbers"}
          </button>
        </div>

        <p class="note">
          <strong>Click a square to dig into it</strong>, drag across squares to take a range, or
          click the label on the left for a whole row. Everything below, and the Timeline, Gallery
          and Scene tabs with it, follows what you pick.
        </p>

        <p class="note">
          Look for <strong>gaps</strong>. A run of empty {CELL_UNIT[grid.level]} is either a stretch
          when you took no pictures, or a batch whose dates went missing. A single square far from the
          rest is usually a wrong date: {grid.emptyCells} of the {grid.cellCount}
          {CELL_UNIT[grid.level]} in this range are empty.
        </p>
      </figure>
    {/if}

    <figure>
      <figcaption>
        <h3>Time of day</h3>
        <p>How many files carry each hour, from midnight on the left to 11pm on the right.</p>
      </figcaption>

      <div class="columns" style:--rows={hourTicks.length - 1}>
        <div class="ticks">
          {#each [...hourTicks].reverse() as tick (tick)}<span>{tick.toLocaleString()}</span>{/each}
        </div>
        <div class="bars">
          {#each hours as bar (bar.hour)}
            <span
              class="column"
              style:height="{(bar.files / hourTicks[hourTicks.length - 1]) * 100}%"
              title="{String(bar.hour).padStart(2, '0')}:00, {bar.files} {bar.files === 1
                ? 'file'
                : 'files'}"
            ></span>
          {/each}
        </div>
        <div class="hours faint">
          <span>00</span><span>06</span><span>12</span><span>18</span><span>23</span>
        </div>
      </div>

      <p class="note">
        A normal camera roll bulges through daylight hours. A <strong
          >spike at midnight</strong
        > means those files carry a date with no time, typically read out of the file name, so they
        are placed by day, not by moment. Here that is {midnightPercent}% of the files.
      </p>
    </figure>

    <figure>
      <figcaption>
        <h3>Where each date came from</h3>
        <p>The source eonsort ended up trusting for each file.</p>
      </figcaption>

      <div class="rows">
        {#each providers as bar (bar.provider)}
          <div class="row">
            <span class="key">{PROVIDER_LABEL[bar.provider]}</span>
            <span class="track">
              <span
                class="fill"
                style:width="{(bar.files / providerMax) * 100}%"
                style:background={PROVIDER_COLOUR[bar.provider]}
              ></span>
            </span>
            <span class="value">{bar.files.toLocaleString()}</span>
          </div>
        {/each}
      </div>

      <p class="note">
        EXIF and media metadata are written by the camera and are the ones to trust. A large
        <strong>file system</strong> share is a warning: those dates are when the file was last copied
        onto this disk, not when the picture was taken, so they are only used when nothing better exists.
      </p>
    </figure>

    <figure>
      <figcaption>
        <h3>How sure eonsort is</h3>
        <p>The same four colours the timeline uses for its points.</p>
      </figcaption>

      <div class="rows">
        {#each readings as bar (bar.reading)}
          <div class="row">
            <span class="key"><i class="dot" style:background={READING_COLOUR[bar.reading]}></i
              >{READING_LABEL[bar.reading]}</span
            >
            <span class="track">
              <span
                class="fill"
                style:width="{(bar.files / readingMax) * 100}%"
                style:background={READING_COLOUR[bar.reading]}
              ></span>
            </span>
            <span class="value">{bar.files.toLocaleString()}</span>
          </div>
        {/each}
      </div>

      <dl class="glossary">
        {#each readings as bar (bar.reading)}
          <div>
            <dt>{READING_LABEL[bar.reading]}</dt>
            <dd>{READING_NOTE[bar.reading]}</dd>
          </div>
        {/each}
      </dl>
    </figure>

    <figure>
      <figcaption>
        <h3>Where the bulk lands</h3>
        <p>The twelve destination folders that receive the most files.</p>
      </figcaption>

      <div class="rows">
        {#each folders as bar (bar.folder)}
          <div class="row">
            <span class="key mono truncate" title={bar.folder}>{bar.folder}</span>
            <span class="track">
              <span class="fill plain" style:width="{(bar.files / folderMax) * 100}%"></span>
            </span>
            <span class="value">{bar.files.toLocaleString()}</span>
          </div>
        {/each}
      </div>

      <p class="note">
        One folder holding far more than the rest usually means a month everything fell into because
        its real date was unknown. Open it from the Folders view to check.
      </p>
    </figure>
  {/if}
</div>

<style>
  .charts {
    padding: 16px 18px 28px;
    display: flex;
    flex-direction: column;
    gap: 22px;
    height: 100%;
    background: var(--bg-panel);
  }

  .placeholder {
    padding: 40px 0;
    font-size: 12px;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 8px;
  }

  .tile {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: grid;
    gap: 2px;
  }

  .tile .value {
    font-size: 19px;
    color: var(--text);
  }

  .tile .label {
    font-size: 11px;
    color: var(--text-dim);
  }

  .tile.alarm .value {
    color: var(--danger);
  }

  figure {
    margin: 0;
    display: grid;
    gap: 10px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px;
  }

  figcaption {
    display: grid;
    gap: 3px;
  }

  h3 {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    font-weight: 600;
  }

  figcaption p {
    margin: 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.45;
  }

  .note {
    margin: 0;
    font-size: 11px;
    line-height: 1.55;
    color: var(--text-dim);
    border-left: 2px solid var(--border-strong);
    padding-left: 9px;
  }

  .note strong {
    color: var(--text);
    font-weight: 600;
  }

  .heat {
    display: grid;
    gap: 2px;
    overflow-x: auto;
    user-select: none;
  }

  .band {
    display: grid;
    grid-template-columns: 62px repeat(var(--columns), minmax(11px, 1fr));
    gap: 2px;
    align-items: center;
  }

  .band > span {
    font-size: 9px;
    color: var(--text-faint);
    text-align: center;
  }

  .stub {
    font-size: 10px;
    color: var(--text-dim);
    text-align: right;
    padding: 0 4px 0 0;
    background: transparent;
    border-color: transparent;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .stub:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--border-strong);
  }

  .stub:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .cell {
    height: 17px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }

  .cell.empty {
    box-shadow: inset 0 0 0 1px var(--border);
  }

  .cell.gone {
    background: transparent;
    cursor: default;
  }

  .cell:hover:not(.gone) {
    box-shadow: inset 0 0 0 1px var(--text-dim);
  }

  .cell.picked,
  .cell.picked:hover {
    box-shadow:
      inset 0 0 0 1px var(--accent),
      0 0 0 1px var(--accent);
  }

  .cell b {
    font-size: 8px;
    font-weight: 600;
    color: #06121f;
    mix-blend-mode: luminosity;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .ramp {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
  }

  .ramp i {
    width: 13px;
    height: 9px;
    border-radius: 1px;
  }

  .columns {
    display: grid;
    grid-template-columns: 34px 1fr;
    grid-template-areas: "ticks bars" ". hours";
    gap: 4px 6px;
  }

  .ticks {
    grid-area: ticks;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    font-size: 9px;
    color: var(--text-faint);
    text-align: right;
    height: 120px;
  }

  .bars {
    grid-area: bars;
    height: 120px;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    border-bottom: 1px solid var(--border-strong);
    background:
      repeating-linear-gradient(
        to top,
        transparent,
        transparent calc(100% / var(--rows) - 1px),
        var(--border) calc(100% / var(--rows) - 1px),
        var(--border) calc(100% / var(--rows))
      );
  }

  .column {
    flex: 1;
    min-height: 1px;
    background: var(--accent);
    border-radius: 3px 3px 0 0;
  }

  .hours {
    grid-area: hours;
    display: flex;
    justify-content: space-between;
    font-size: 9px;
  }

  .rows {
    display: grid;
    gap: 5px;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(80px, 150px) 1fr 52px;
    align-items: center;
    gap: 8px;
  }

  .key {
    font-size: 11px;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .track {
    height: 11px;
    background: var(--bg-hover);
    border-radius: 3px;
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    border-radius: 3px;
    min-width: 2px;
  }

  .fill.plain {
    background: var(--accent-dim);
  }

  .row .value {
    font-size: 11px;
    color: var(--text);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .glossary {
    margin: 0;
    display: grid;
    gap: 5px;
  }

  .glossary div {
    display: grid;
    grid-template-columns: minmax(80px, 150px) 1fr;
    gap: 8px;
  }

  dt {
    font-size: 11px;
    color: var(--text);
  }

  dd {
    margin: 0;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-dim);
  }
</style>
