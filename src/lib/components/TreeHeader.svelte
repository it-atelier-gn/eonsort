<script lang="ts">
  import { COLUMNS, isColumnId, moveColumn, type ColumnId } from "$lib/columns";

  interface Props {
    order: ColumnId[];
    grid: string;
    onReorder: (order: ColumnId[]) => void;
    onResize: (id: ColumnId, width: number | null) => void;
  }

  let { order, grid, onReorder, onResize }: Props = $props();

  const STEP = 8;

  let dragging = $state<ColumnId | null>(null);
  let over = $state<ColumnId | null>(null);
  let sizing = $state<{ id: ColumnId; from: number; width: number } | null>(null);

  function start(event: DragEvent, id: ColumnId) {
    dragging = id;
    event.dataTransfer?.setData("text/plain", id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function drop(event: DragEvent, id: ColumnId) {
    event.preventDefault();
    const held = dragging ?? event.dataTransfer?.getData("text/plain");
    dragging = null;
    over = null;
    if (isColumnId(held) && held !== id) onReorder(moveColumn(order, held, id));
  }

  function nudge(id: ColumnId, by: number) {
    const at = order.indexOf(id);
    const target = order[at + by];
    if (target) onReorder(moveColumn(order, id, target));
  }

  function cellWidth(grip: HTMLElement): number {
    return grip.parentElement?.getBoundingClientRect().width ?? 0;
  }

  function grab(event: PointerEvent, id: ColumnId) {
    const grip = event.currentTarget as HTMLElement;
    event.preventDefault();
    event.stopPropagation();
    sizing = { id, from: event.clientX, width: cellWidth(grip) };
    grip.setPointerCapture(event.pointerId);
  }

  function slide(event: PointerEvent) {
    if (!sizing) return;
    onResize(sizing.id, sizing.width + (event.clientX - sizing.from));
  }

  function release(event: PointerEvent) {
    if (!sizing) return;
    const grip = event.currentTarget as HTMLElement;
    if (grip.hasPointerCapture(event.pointerId)) grip.releasePointerCapture(event.pointerId);
    sizing = null;
  }

  function stretch(event: KeyboardEvent, id: ColumnId) {
    const by = event.key === "ArrowLeft" ? -STEP : event.key === "ArrowRight" ? STEP : 0;
    if (by === 0) return;
    event.preventDefault();
    event.stopPropagation();
    onResize(id, cellWidth(event.currentTarget as HTMLElement) + by);
  }
</script>

<div class="head" style="grid-template-columns: {grid}" role="row">
  {#each order as id (id)}
    <span
      class="cap"
      class:right={COLUMNS[id].align === "right"}
      class:over={over === id && dragging !== id}
      class:held={dragging === id}
      role="columnheader"
      tabindex="0"
      draggable="true"
      title="Drag to reorder, or use the arrow keys"
      ondragstart={(e) => start(e, id)}
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
      onkeydown={(e) => {
        if (e.key === "ArrowLeft") {
          e.preventDefault();
          nudge(id, -1);
        }
        if (e.key === "ArrowRight") {
          e.preventDefault();
          nudge(id, 1);
        }
      }}
    >
      {COLUMNS[id].label}
      <button
        type="button"
        class="grip"
        class:sizing={sizing?.id === id}
        aria-label="Resize the {COLUMNS[id].label} column"
        title="Drag to resize, or double click to fit"
        draggable="false"
        ondragstart={(e) => {
          e.preventDefault();
          e.stopPropagation();
        }}
        onpointerdown={(e) => grab(e, id)}
        onpointermove={slide}
        onpointerup={release}
        onpointercancel={release}
        ondblclick={() => onResize(id, null)}
        onkeydown={(e) => stretch(e, id)}
      ></button>
    </span>
  {/each}
</div>

<style>
  .head {
    display: grid;
    align-items: center;
    gap: 6px;
    padding-block: 5px;
    padding-inline: 8px;
    border-bottom: 1px solid var(--border);
    border-left: 2px solid transparent;
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--bg-panel, var(--bg));
  }

  .cap {
    position: relative;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-faint);
    cursor: grab;
    user-select: none;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    border-radius: 3px;
  }

  .cap.right {
    text-align: right;
  }

  .cap.held {
    opacity: 0.4;
    cursor: grabbing;
  }

  .cap.over {
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .cap:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }

  .grip {
    position: absolute;
    padding: 0;
    border: none;
    background: none;
    appearance: none;
    top: 0;
    bottom: 0;
    right: 0;
    width: 7px;
    cursor: col-resize;
    touch-action: none;
  }

  .grip::after {
    content: "";
    position: absolute;
    top: 1px;
    bottom: 1px;
    right: 3px;
    width: 1px;
    background: var(--border);
  }

  .grip:hover::after,
  .grip.sizing::after,
  .grip:focus-visible::after {
    background: var(--accent);
  }

  .grip:focus-visible {
    outline: none;
  }
</style>
