<script lang="ts">
  import { formatBytes } from "$lib/api";
  import { folderKey, type TreeNode } from "$lib/tree";
  import Self from "./TreeItem.svelte";

  interface Props {
    node: TreeNode;
    depth: number;
    selected: string | null;
    expanded: Set<string>;
    onSelect: (key: string) => void;
    onToggle: (path: string) => void;
  }

  let { node, depth, selected, expanded, onSelect, onToggle }: Props = $props();

  const hasChildren = $derived(node.children.length > 0);
  const isOpen = $derived(expanded.has(node.path));
  const isSelected = $derived(selected === folderKey(node.path));

  function activate() {
    onSelect(folderKey(node.path));
    if (hasChildren) {
      onToggle(node.path);
    }
  }
</script>

<div
  class="row"
  class:selected={isSelected}
  style="padding-left: {8 + depth * 14}px"
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
  }}
>
  <span class="chevron" class:open={isOpen} class:hidden={!hasChildren}>›</span>
  <span class="name truncate">{node.name}</span>
  <span class="count faint">{node.files}</span>
</div>

{#if hasChildren && isOpen}
  {#each node.children as child (child.path)}
    <Self node={child} depth={depth + 1} {selected} {expanded} {onSelect} {onToggle} />
  {/each}
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-block: 4px;
    padding-right: 8px;
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

  .chevron {
    display: inline-block;
    width: 10px;
    color: var(--text-faint);
    transition: transform 0.12s ease;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .chevron.hidden {
    visibility: hidden;
  }

  .name {
    flex: 1;
  }

  .count {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
</style>
