<script lang="ts">
  import type { EntryView, Provider } from "$lib/api";
  import { describeShift, fromInputValue, toInputValue } from "$lib/dates";

  interface Props {
    entries: EntryView[];
    busy: boolean;
    onShift: (sources: string[], seconds: number) => void;
    onReprovider: (sources: string[], provider: Provider) => void;
    onRotate: (sources: string[], quarterTurns: number) => void;
    onClear: () => void;
  }

  let { entries, busy, onShift, onReprovider, onRotate, onClear }: Props = $props();

  let target = $state("");
  let anchored = $state("");
  let provider = $state<Provider>("filesystem");

  const anchor = $derived(entries[0] ?? null);
  const sources = $derived(entries.map((entry) => entry.source));

  const providers = $derived(
    [...new Set(entries.flatMap((entry) => entry.candidates.map((c) => c.provider)))].sort(),
  );

  $effect(() => {
    if (anchor && anchor.source !== anchored) {
      anchored = anchor.source;
      target = toInputValue(anchor.taken_epoch);
    }
  });

  const seconds = $derived.by(() => {
    if (!anchor) return null;
    const parsed = fromInputValue(target);
    return parsed === null ? null : parsed - anchor.taken_epoch;
  });

  const turnable = $derived(entries.filter((entry) => entry.orientation > 0));
</script>

<div class="panel">
  <div class="head">
    <strong>{entries.length} selected</strong>
    <button class="ghost" onclick={onClear}>Clear</button>
  </div>

  {#if anchor}
    <div class="row">
      <label for="anchor-date">
        True date of <span class="mono">{anchor.name}</span>
      </label>
      <div class="controls">
        <input id="anchor-date" type="datetime-local" step="1" bind:value={target} disabled={busy} />
        <button
          class="primary"
          disabled={busy || seconds === null || seconds === 0}
          onclick={() => seconds !== null && onShift(sources, seconds)}
        >
          Shift all by {seconds === null ? "…" : describeShift(seconds)}
        </button>
      </div>
      <p class="faint hint">
        Moves every selected file by the same offset, keeping the gaps between them.
      </p>
    </div>
  {/if}

  {#if providers.length > 0}
    <div class="row">
      <label for="cluster-provider">Re-date them all from</label>
      <div class="controls">
        <select id="cluster-provider" bind:value={provider} disabled={busy}>
          {#each providers as option (option)}
            <option value={option}>{option}</option>
          {/each}
        </select>
        <button disabled={busy} onclick={() => onReprovider(sources, provider)}>
          Apply to {entries.length}
        </button>
      </div>
      <p class="faint hint">Files without a date from that source are left untouched.</p>
    </div>
  {/if}

  {#if turnable.length > 0}
    <div class="row">
      <span class="label">Turn them all</span>
      <div class="controls">
        <button
          disabled={busy}
          onclick={() => onRotate(turnable.map((entry) => entry.source), -1)}
        >
          ↺ Left
        </button>
        <button disabled={busy} onclick={() => onRotate(turnable.map((entry) => entry.source), 1)}>
          ↻ Right
        </button>
      </div>
      <p class="faint hint">
        {turnable.length} of {entries.length} can be turned. Applied when they are copied.
      </p>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    align-items: flex-start;
    gap: 20px;
    flex-wrap: wrap;
    padding: 10px 14px;
    background: var(--bg-raised);
    border-top: 1px solid var(--border);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    padding-top: 14px;
  }

  .row {
    display: grid;
    gap: 4px;
  }

  .controls {
    display: flex;
    gap: 5px;
  }

  .hint {
    font-size: 10px;
    max-width: 320px;
  }

  .label {
    display: block;
    color: var(--text-dim);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: 4px;
  }
</style>
