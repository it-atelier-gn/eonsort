<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    cancelInstall,
    checkModel,
    formatBytes,
    installModel,
    uninstallModel,
    type AiConfig,
    type ModelApi,
    type ModelStatus,
    type PullProgress,
  } from "$lib/api";
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
    {
      id: "vision",
      label: "In the picture",
      hint: "A local model reads a date printed in the image itself",
    },
    { id: "filesystem", label: "File system", hint: "Created / modified time" },
  ];

  const MANAGED: { key: "vision_model" | "embed_model"; placeholder: string }[] = [
    { key: "vision_model", placeholder: "vision model" },
    { key: "embed_model", placeholder: "embedding model" },
  ];

  const REMOTE_ONLY = "Only an Ollama runner can install and remove models from here";

  const MODEL_APIS: { id: ModelApi; label: string }[] = [
    { id: "ollama", label: "Ollama" },
    { id: "open_ai", label: "OpenAI-compatible" },
  ];

  let status = $state<ModelStatus | null>(null);
  let checking = $state(false);
  let pulling = $state<string | null>(null);
  let progress = $state<PullProgress | null>(null);
  let removing = $state<string | null>(null);
  let confirming = $state<string | null>(null);
  let modelNote = $state<string | null>(null);
  let modelError = $state<string | null>(null);

  const manageable = $derived(settings.ai.enabled && settings.ai.api === "ollama");
  const share = $derived(
    progress && progress.total > 0 ? Math.min(1, progress.completed / progress.total) : null,
  );

  let unlisteners: UnlistenFn[] = [];

  onMount(async () => {
    unlisteners = await Promise.all([
      listen<PullProgress>("model:progress", (e) => (progress = e.payload)),
      listen<{ model: string }>("model:done", async (e) => {
        pulling = null;
        progress = null;
        modelNote = `${e.payload.model} is installed.`;
        await check();
      }),
      listen<string>("model:error", (e) => {
        pulling = null;
        progress = null;
        modelError =
          e.payload === "cancelled" ? "Download stopped. Nothing was installed." : e.payload;
      }),
    ]);
  });

  onDestroy(() => unlisteners.forEach((un) => un()));

  function updateAi(patch: Partial<AiConfig>) {
    onChange({ ...settings, ai: { ...settings.ai, ...patch } });
    status = null;
    confirming = null;
  }

  async function check() {
    checking = true;
    try {
      status = await checkModel(settings.ai);
    } catch (e) {
      status = {
        reachable: false,
        models: [],
        error: String(e),
        vision_present: false,
        embed_present: false,
      };
    } finally {
      checking = false;
    }
  }

  async function install(model: string) {
    modelError = null;
    modelNote = null;
    pulling = model;
    progress = null;
    try {
      await installModel(settings.ai, model);
    } catch (e) {
      pulling = null;
      modelError = String(e);
    }
  }

  async function remove(model: string) {
    modelError = null;
    modelNote = null;
    confirming = null;
    removing = model;
    try {
      await uninstallModel(settings.ai, model);
      modelNote = `${model} was removed.`;
      await check();
    } catch (e) {
      modelError = String(e);
    } finally {
      removing = null;
    }
  }

  function installed(model: string): boolean {
    const wanted = model.trim();
    if (!wanted || !status?.reachable) return false;
    return status.models.some((m) => m === wanted || m.split(":")[0] === wanted);
  }

  const STRATEGIES: { id: Strategy; label: string; hint: string }[] = [
    {
      id: "smart",
      label: "Weigh the evidence",
      hint: "Rejects camera reset dates and impossible dates, then prefers what two sources agree on",
    },
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

  function clearDestination() {
    onChange({ ...settings, destination: null });
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
        placeholder="Not chosen yet"
      />
      <button onclick={chooseDestination} disabled={busy}>Browse</button>
      {#if settings.destination}
        <button class="ghost" onclick={clearDestination} disabled={busy} title="Forget it">×</button>
      {/if}
    </div>
    {#if !settings.destination}
      <p class="faint hint">
        Only needed to copy. Scan without one to see where everything would land.
      </p>
    {/if}
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
            disabled={busy || (provider.id === "vision" && !settings.ai.enabled)}
            onchange={() => toggleProvider(provider.id)}
          />
          <span>{provider.label}</span>
        </label>
      {/each}
    </div>
    {#if settings.providers.includes("vision") && !settings.ai.vision_in_scan}
      <p class="faint hint">
        Only used when you ask for it on a file. Turn on &ldquo;look at every picture&rdquo; below to
        use it during the scan.
      </p>
    {/if}
  </section>

  <section>
    <label for="ai-enabled">Local model</label>
    <label class="check" title="Nothing leaves your machine — eonsort talks to a model runner you host">
      <input
        id="ai-enabled"
        type="checkbox"
        checked={settings.ai.enabled}
        disabled={busy}
        onchange={(e) => updateAi({ enabled: e.currentTarget.checked })}
      />
      <span>Use a local model</span>
    </label>

    {#if settings.ai.enabled}
      <input
        class="spaced"
        type="text"
        placeholder="http://localhost:11434"
        value={settings.ai.endpoint}
        disabled={busy}
        onchange={(e) => updateAi({ endpoint: e.currentTarget.value })}
      />
      <select
        class="spaced"
        value={settings.ai.api}
        disabled={busy}
        onchange={(e) => updateAi({ api: e.currentTarget.value as ModelApi })}
      >
        {#each MODEL_APIS as option (option.id)}
          <option value={option.id}>{option.label}</option>
        {/each}
      </select>
      {#each MANAGED as slot (slot.key)}
        {@const name = settings.ai[slot.key]}
        {@const here = installed(name)}
        <div class="model spaced">
          <input
            type="text"
            placeholder={slot.placeholder}
            value={name}
            disabled={busy || pulling !== null}
            onchange={(e) => updateAi({ [slot.key]: e.currentTarget.value })}
          />
          <div class="model-foot">
            {#if status?.reachable}
              <span class="badge" class:ok={here} class:missing={!here}>
                {here ? "installed" : "not installed"}
              </span>
            {:else}
              <span class="faint tiny">check the connection to see if it is installed</span>
            {/if}

            <span class="grow"></span>

            {#if pulling === name}
              <button class="ghost" onclick={() => cancelInstall()}>Stop</button>
            {:else if here}
              {#if confirming === name}
                <button class="danger" disabled={removing !== null} onclick={() => remove(name)}>
                  Really remove?
                </button>
                <button class="ghost" onclick={() => (confirming = null)}>Keep</button>
              {:else}
                <button
                  class="ghost"
                  disabled={busy || !manageable || removing !== null || pulling !== null}
                  title={manageable ? "Delete it from the runner" : REMOTE_ONLY}
                  onclick={() => (confirming = name)}
                >
                  Remove
                </button>
              {/if}
            {:else}
              <button
                disabled={busy || !manageable || !name.trim() || pulling !== null}
                title={manageable ? "Download it into Ollama" : REMOTE_ONLY}
                onclick={() => install(name)}
              >
                Download
              </button>
            {/if}
          </div>

          {#if pulling === name}
            <div class="pull">
              <div class="track">
                <div class="fill" style:width="{(share ?? 0) * 100}%" class:idle={share === null}></div>
              </div>
              <span class="faint tiny">
                {progress?.status ?? "starting"}
                {#if progress && progress.total > 0}
                  · {formatBytes(progress.completed)} of {formatBytes(progress.total)}
                {/if}
              </span>
            </div>
          {/if}
        </div>
      {/each}

      <label class="check" title="Much slower — roughly a second per picture">
        <input
          type="checkbox"
          checked={settings.ai.vision_in_scan}
          disabled={busy}
          onchange={(e) => updateAi({ vision_in_scan: e.currentTarget.checked })}
        />
        <span>Look at every picture during the scan</span>
      </label>

      <div class="row spaced">
        <button disabled={busy || checking} onclick={check}>
          {checking ? "Checking…" : "Check connection"}
        </button>
      </div>

      {#if status}
        {#if !status.reachable}
          <p class="hint error">{status.error}</p>
        {:else}
          <p class="hint">
            <span class="badge ok">reachable</span>
            {status.models.length}
            {status.models.length === 1 ? "model" : "models"}
          </p>
        {/if}
      {/if}

      {#if modelNote}
        <p class="hint">{modelNote}</p>
      {/if}
      {#if modelError}
        <p class="hint error">{modelError}</p>
      {/if}
    {/if}
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
        checked={settings.auto_rotate}
        disabled={busy}
        onchange={(e) => onChange({ ...settings, auto_rotate: e.currentTarget.checked })}
      />
      <span>Turn pictures upright when copying</span>
    </label>
    <p class="faint hint">
      Decided during the scan from each picture's own orientation tag, and correctable per file in
      the preview. JPEGs are turned without any loss of quality.
    </p>
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

  .spaced {
    margin-top: 6px;
    width: 100%;
  }

  .model {
    display: grid;
    gap: 5px;
  }

  .model input {
    width: 100%;
  }

  .model-foot {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .grow {
    flex: 1;
  }

  .tiny {
    font-size: 10px;
  }

  .badge.missing {
    background: var(--bg-hover);
    color: var(--text-dim);
  }

  .pull {
    display: grid;
    gap: 3px;
  }

  .pull .track {
    height: 5px;
    background: var(--bg-hover);
    border-radius: 3px;
    overflow: hidden;
  }

  .pull .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 120ms linear;
  }

  .pull .fill.idle {
    width: 35% !important;
    background: linear-gradient(90deg, transparent, var(--accent-dim), transparent);
    animation: sweep 1.1s ease-in-out infinite;
  }

  @keyframes sweep {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
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
