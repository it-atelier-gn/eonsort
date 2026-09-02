<script lang="ts">
  import { formatBytes } from "$lib/api";
  import { folderKey, type TreeNode } from "$lib/tree";
  import type { TreeColumnId } from "$lib/columns";
  import Self from "./TreeItem.svelte";

  interface Props {
    node: TreeNode;
    depth: number;
    selected: string | null;
    expanded: Set<string>;
    order: TreeColumnId[];
    grid: string;
    onSelect: (key: string) => void;
    onToggle: (path: string) => void;
  }

  let { node, depth, selected, expanded, order, grid, onSelect, onToggle }: Props = $props();

  const hasChildren = $derived(node.children.length > 0);
  const isOpen = $derived(expanded.has(node.path));
  const isSelected = $derived(selected === folderKey(node.path));

  function activate() {
    onSelect(folderKey(node.path));
  }

  function fold(event: MouseEvent) {
    event.stopPropagation();
    onToggle(node.path);
  }
</script>

<div
  class="row"
  class:selected={isSelected}
  style="grid-template-columns: {grid}"
  title="{node.files} files, {formatBytes(node.bytes)}"
  role="treeitem"
  aria-selected={isSelected}
  aria-expanded={hasChildren ? isOpen : undefined}
  tabindex="0"
  onclick={activate}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      activate();
    }
    if (e.key === "ArrowRight" && hasChildren && !isOpen) onToggle(node.path);
    if (e.key === "ArrowLeft" && hasChildren && isOpen) onToggle(node.path);
  }}
>
  {#each order as id (id)}
    {#if id === "name"}
      <span class="cell name" style="padding-left: {depth * 14}px">
        <span
          class="chevron"
          class:open={isOpen}
          class:hidden={!hasChildren}
          role="presentation"
          onclick={fold}
        >
          ›
        </span>
        <span class="truncate">{node.name}</span>
      </span>
    {:else if id === "files"}
      <span class="cell number faint">{node.files}</span>
    {:else}
      <span class="cell number faint">{formatBytes(node.bytes)}</span>
    {/if}
  {/each}
</div>

{#if hasChildren && isOpen}
  {#each node.children as child (child.path)}
    <Self
      node={child}
      depth={depth + 1}
      {selected}
      {expanded}
      {order}
      {grid}
      {onSelect}
      {onToggle}
    />
  {/each}
{/if}

<style>
  .row {
    display: grid;
    align-items: center;
    gap: 6px;
    padding-block: 4px;
    padding-inline: 8px;
    cursor: pointer;
    user-select: none;
    border-left: 2px solid transparent;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .row.selected {
    background: var(--bg-active);
    border-left-color: var(--accent);
  }

  .cell {
    min-width: 0;
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .number {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .chevron {
    display: inline-block;
    width: 10px;
    flex: none;
    color: var(--text-faint);
    transition: transform 0.12s ease;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .chevron.hidden {
    visibility: hidden;
  }
</style>
