<script lang="ts">
  import { formatBytes, type EntryView, type Preview } from "$lib/api";

  interface Props {
    entry: EntryView | null;
    preview: Preview | null;
    loading: boolean;
    onOpen: (path: string) => void;
    onReveal: (path: string) => void;
  }

  let { entry, preview, loading, onOpen, onReveal }: Props = $props();
</script>

<aside class="pane">
  {#if !entry}
    <div class="empty faint">
      <p>Select a file to preview it.</p>
      <p class="hint">Double-click a row to open it in your usual application.</p>
    </div>
  {:else}
    <div class="visual">
      {#if loading}
        <p class="faint">Loading preview…</p>
      {:else if preview?.kind === "image"}
        <img src="data:{preview.mime};base64,{preview.data}" alt={entry.name} />
      {:else if preview?.kind === "text"}
        <pre class="mono">{preview.head}{preview.truncated ? "\n…" : ""}</pre>
      {:else if preview?.kind === "missing"}
        <p class="faint">The source file is no longer there.</p>
      {:else}
        <p class="faint">No preview for this file type.</p>
      {/if}
    </div>

    <div class="details scroll">
      <h2 class="truncate" title={entry.name}>{entry.name}</h2>

      <dl>
        <dt>Date used</dt>
        <dd class="mono">{entry.taken}</dd>

        <dt>Read from</dt>
        <dd>{entry.provider}{entry.provider_info ? ` · ${entry.provider_info}` : ""}</dd>

        <dt>Size</dt>
        <dd>{formatBytes(entry.size)}</dd>

        <dt>Source</dt>
        <dd class="mono break" title={entry.source}>{entry.source}</dd>

        <dt>Goes to</dt>
        <dd class="mono break" title={entry.destination}>{entry.destination}</dd>

        {#if entry.outcome}
          <dt>Result</dt>
          <dd>{entry.outcome}</dd>
        {/if}
      </dl>

      <div class="actions">
        <button onclick={() => onOpen(entry.source)}>Open source</button>
        <button onclick={() => onReveal(entry.source)}>Show in folder</button>
        {#if entry.outcome && entry.outcome !== "failed"}
          <button onclick={() => onOpen(entry.destination)}>Open copy</button>
        {/if}
      </div>
    </div>
  {/if}
</aside>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-panel);
    border-left: 1px solid var(--border);
  }

  .empty {
    padding: 24px 16px;
    font-size: 12px;
  }

  .empty .hint {
    margin-top: 8px;
    font-size: 11px;
  }

  .visual {
    flex-shrink: 0;
    height: 240px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    padding: 10px;
  }

  .visual img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-sm);
  }

  .visual pre {
    width: 100%;
    height: 100%;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 11px;
    color: var(--text-dim);
  }

  .details {
    padding: 14px;
    flex: 1;
  }

  h2 {
    font-size: 14px;
    margin-bottom: 12px;
  }

  dl {
    display: grid;
    grid-template-columns: 1fr;
    gap: 2px;
  }

  dt {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin-top: 10px;
  }

  dd {
    font-size: 12px;
  }

  .break {
    word-break: break-all;
    line-height: 1.4;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 18px;
  }
</style>
