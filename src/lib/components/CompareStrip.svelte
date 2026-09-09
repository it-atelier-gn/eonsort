<script lang="ts">
  import { formatBytes, thumbnailFor, type EntryView } from "$lib/api";
  import { agree, cards, type CompareCard } from "$lib/compare";

  interface Props {
    sources: string[];
    keeper: string | null;
    entries: Record<string, EntryView>;
    busy: boolean;
    onOpen: (path: string) => void;
    onReveal: (path: string) => void;
    onLeaveOut: (path: string) => void;
    onClose: () => void;
  }

  let { sources, keeper, entries, busy, onOpen, onReveal, onLeaveOut, onClose }: Props = $props();

  const COMPARE_EDGE = 220;

  let shots = $state<Record<string, string>>({});
  const shown = $derived(cards(sources, keeper, entries));
  const sameDate = $derived(agree(shown, (card) => card.taken));
  const sameSize = $derived(agree(shown, (card) => card.size));

  $effect(() => {
    for (const source of sources) {
      if (shots[source] === undefined) void fetch(source);
    }
  });

  async function fetch(source: string) {
    shots[source] = "";
    try {
      const found = await thumbnailFor(source, COMPARE_EDGE);
      shots[source] = found.kind === "image" ? `data:image/jpeg;base64,${found.data}` : "";
    } catch {
      shots[source] = "";
    }
  }

  function facts(card: CompareCard): string {
    const parts: string[] = [];
    if (card.size !== null) parts.push(formatBytes(card.size));
    if (card.taken !== null) parts.push(card.taken);
    if (card.provider !== null) parts.push(`by ${card.provider}`);
    return parts.join(" · ");
  }
</script>

<div class="compare">
  <div class="compare-head">
    <span class="faint tiny">
      {shown.length} files, side by side.
      {#if !sameSize}
        <span class="warnText">They are not all the same size.</span>
      {:else if !sameDate}
        <span class="warnText">They carry different dates.</span>
      {/if}
    </span>
    <button class="ghost" onclick={onClose}>Close</button>
  </div>

  <div class="cards">
    {#each shown as card (card.source)}
      <figure class:kept={card.keeper}>
        <div class="shot">
          {#if shots[card.source]}
            <img src={shots[card.source]} alt={card.name} />
          {:else}
            <span class="faint tiny">no preview</span>
          {/if}
        </div>
        <figcaption>
          <span class="mono truncate" title={card.source}>{card.name}</span>
          <span class="faint tiny truncate" title={card.folder}>{card.folder}</span>
          <span class="faint tiny">{facts(card)}</span>
          {#if card.keeper}
            <span class="badge info">kept</span>
          {:else}
            <span class="badge warn">goes to the recycle bin</span>
          {/if}
          <span class="row">
            <button class="ghost" onclick={() => onOpen(card.source)}>Open</button>
            <button class="ghost" onclick={() => onReveal(card.source)}>Show in folder</button>
            <button class="ghost" disabled={busy} onclick={() => onLeaveOut(card.source)}>
              Leave out
            </button>
          </span>
        </figcaption>
      </figure>
    {/each}
  </div>
</div>

<style>
  .compare {
    margin: 6px 0 10px;
    padding: 8px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .compare-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .cards {
    display: flex;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  figure {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 6px;
    width: 220px;
    flex: 0 0 auto;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 4px;
  }

  figure.kept {
    border-color: var(--accent, #4c8);
  }

  .shot {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 160px;
    background: var(--bg-sunken, rgba(0, 0, 0, 0.2));
    border-radius: 3px;
  }

  .shot img {
    max-width: 100%;
    max-height: 160px;
    object-fit: contain;
  }

  figcaption {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
