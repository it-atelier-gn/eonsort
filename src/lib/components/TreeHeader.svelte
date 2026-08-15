<script lang="ts">
  import { COLUMNS, isColumnId, moveColumn, type ColumnId } from "$lib/columns";

  interface Props {
    order: ColumnId[];
    grid: string;
    onReorder: (order: ColumnId[]) => void;
  }

  let { order, grid, onReorder }: Props = $props();

  let dragging = $state<ColumnId | null>(null);
  let over = $state<ColumnId | null>(null);

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
</style>
