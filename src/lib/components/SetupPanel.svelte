<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import type { Provider, Settings, Strategy } from "$lib/api";

  interface Props {
    settings: Settings;
    busy: boolean;
    patternError: string | null;
    onChange: (settings: Settings) => void;
  }

  let { settings, busy, patternError, onChange }: Props = $props();

  const PROVIDERS: { id: Provider; label: string; hint: string }[] = [
    { id: "filename", label: "File name", hint: "Dates written into the name" },
    { id: "exif", label: "EXIF", hint: "Camera metadata in photos" },
    { id: "media", label: "Media", hint: "Recording time in videos" },
    { id: "filesystem", label: "File system", hint: "Created / modified time" },
  ];

  const STRATEGIES: { id: Strategy; label: string; hint: string }[] = [
    { id: "oldest", label: "Oldest date wins", hint: "Ask every source, keep the earliest" },
    { id: "priority", label: "First match wins", hint: "Stop at the first source with a date" },
  ];

  async function addSources() {
    const picked = await open({ directory: true, multiple: true, title: "Add source folders" });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    const merged = [...new Set([...settings.sources, ...paths])];
    onChange({ ...settings, sources: merged });
  }

  async function chooseDestination() {
    const picked = await open({ directory: true, multiple: false, title: "Choose destination" });
    if (typeof picked === "string") {
      onChange({ ...settings, destination: picked });
    }
  }

  function removeSource(path: string) {
    onChange({ ...settings, sources: settings.sources.filter((s) => s !== path) });
  }

  function toggleProvider(id: Provider) {
    const enabled = settings.providers.includes(id);
    const next = enabled
      ? settings.providers.filter((p) => p !== id)
      : [...settings.providers, id];
    onChange({ ...settings, providers: next });
  }
</script>

<div class="panel scroll">
  <section>
    <div class="head">
      <label for="source-list">Source folders</label>
      <button class="ghost" onclick={addSources} disabled={busy}>+ Add</button>
    </div>
    <div id="source-list" class="list">
      {#each settings.sources as source (source)}
        <div class="item">
          <span class="mono truncate" title={source}>{source}</span>
          <button class="ghost" onclick={() => removeSource(source)} disabled={busy}>×</button>
        </div>
      {:else}
        <p class="faint empty">No sources yet. Add the folders you want to sort.</p>
      {/each}
    </div>
  </section>

  <section>
    <label for="destination">Destination</label>
    <div class="row">
      <input
        id="destination"
        type="text"
        class="mono"
        readonly
        value={settings.destination ?? ""}
        placeholder="Choose a folder…"
      />
      <button onclick={chooseDestination} disabled={busy}>Browse</button>
    </div>
  </section>

  <section>
    <label for="pattern">Folder layout</label>
    <input
      id="pattern"
      type="text"
      class="mono"
      value={settings.folder_pattern}
      disabled={busy}
      oninput={(e) => onChange({ ...settings, folder_pattern: e.currentTarget.value })}
    />
    {#if patternError}
      <p class="error">{patternError}</p>
    {:else}
      <p class="faint hint">%Y year, %m month, %d day &mdash; e.g. %Y/%m gives 2023/05</p>
    {/if}
  </section>

  <section>
    <label for="providers">Where dates come from</label>
    <div id="providers" class="checks">
      {#each PROVIDERS as provider (provider.id)}
        <label class="check" title={provider.hint}>
          <input
            type="checkbox"
            checked={settings.providers.includes(provider.id)}
            disabled={busy}
            onchange={() => toggleProvider(provider.id)}
          />
          <span>{provider.label}</span>
        </label>
      {/each}
    </div>
  </section>

  <section>
    <label for="strategy">When sources disagree</label>
    <select
      id="strategy"
      disabled={busy}
      value={settings.strategy}
      onchange={(e) => onChange({ ...settings, strategy: e.currentTarget.value as Strategy })}
    >
      {#each STRATEGIES as strategy (strategy.id)}
        <option value={strategy.id}>{strategy.label}</option>
      {/each}
    </select>
    <p class="faint hint">{STRATEGIES.find((s) => s.id === settings.strategy)?.hint}</p>
  </section>

  <section>
    <label for="jobs">Copy settings</label>
    <div class="row">
      <input
        id="jobs"
        type="number"
        min="1"
        max="32"
        value={settings.jobs}
        disabled={busy}
        oninput={(e) => onChange({ ...settings, jobs: Number(e.currentTarget.value) || 1 })}
      />
      <span class="faint nowrap">parallel copies</span>
    </div>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.preserve_times}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, preserve_times: e.currentTarget.checked })}
      />
      <span>Keep original timestamps</span>
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.follow_symlinks}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, follow_symlinks: e.currentTarget.checked })}
      />
      <span>Follow symbolic links</span>
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={settings.compare_hashes}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, compare_hashes: e.currentTarget.checked })}
      />
      <span>Compare contents when checking</span>
    </label>
  </section>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 14px;
    height: 100%;
    background: var(--bg-panel);
    border-right: 1px solid var(--border);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--bg-raised);
    border-radius: var(--radius-sm);
    padding: 4px 4px 4px 8px;
  }

  .item span {
    flex: 1;
    min-width: 0;
  }

  .empty {
    font-size: 12px;
    line-height: 1.4;
  }

  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .row button {
    flex-shrink: 0;
  }

  .checks {
    display: grid;
    gap: 4px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 7px;
    text-transform: none;
    letter-spacing: normal;
    font-size: 13px;
    color: var(--text);
    margin-bottom: 0;
    margin-top: 6px;
    cursor: pointer;
  }

  .checks .check {
    margin-top: 0;
  }

  .hint,
  .error {
    font-size: 11px;
    margin-top: 4px;
    line-height: 1.4;
  }

  .error {
    color: var(--danger);
  }

  .nowrap {
    white-space: nowrap;
    font-size: 12px;
  }
</style>
