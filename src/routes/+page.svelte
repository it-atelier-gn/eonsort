<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    cancelJob,
    checkFolderPattern,
    formatBytes,
    getSettings,
    listEntries,
    listFolders,
    listSkipped,
    openPlan,
    previewFile,
    saveSettings,
    startCopy,
    startScan,
    startVerify,
    type CopyProgress,
    type CopyReport,
    type EntryView,
    type FolderNode,
    type PlanSummary,
    type Preview,
    type ScanProgress,
    type Settings,
    type SkippedView,
    type VerifyProgress,
    type VerifyReport,
  } from "$lib/api";
  import { buildTree, type TreeNode } from "$lib/tree";
  import SetupPanel from "$lib/components/SetupPanel.svelte";
  import TreeItem from "$lib/components/TreeItem.svelte";
  import FileList from "$lib/components/FileList.svelte";
  import PreviewPane from "$lib/components/PreviewPane.svelte";

  type Job = "scan" | "copy" | "verify";

  let settings = $state<Settings | null>(null);
  let summary = $state<PlanSummary | null>(null);
  let folders = $state<FolderNode[]>([]);
  let expanded = $state(new Set<string>());
  let selectedFolder = $state<string | null>(null);
  let entries = $state<EntryView[]>([]);
  let selectedEntry = $state<EntryView | null>(null);
  let preview = $state<Preview | null>(null);
  let previewLoading = $state(false);

  let job = $state<Job | null>(null);
  let scanProgress = $state<ScanProgress | null>(null);
  let copyProgress = $state<CopyProgress | null>(null);
  let verifyProgress = $state<VerifyProgress | null>(null);
  let verifyReport = $state<VerifyReport | null>(null);
  let copyFailures = $state<CopyReport["failures"]>([]);
  let skipped = $state<SkippedView[]>([]);
  let issuesOpen = $state(false);

  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let patternError = $state<string | null>(null);

  const tree = $derived<TreeNode[]>(buildTree(folders));
  const busy = $derived(job !== null);
  const canScan = $derived(
    !busy &&
      settings !== null &&
      settings.sources.length > 0 &&
      !!settings.destination &&
      settings.providers.length > 0 &&
      patternError === null,
  );
  const canRun = $derived(!busy && summary !== null && summary.files > 0);
  const issueCount = $derived(
    skipped.length + copyFailures.length + (verifyReport?.issues.length ?? 0),
  );

  let unlisteners: UnlistenFn[] = [];

  onMount(async () => {
    settings = await getSettings();

    unlisteners = await Promise.all([
      listen<ScanProgress>("scan:progress", (e) => (scanProgress = e.payload)),
      listen<PlanSummary>("scan:done", async (e) => {
        job = null;
        scanProgress = null;
        summary = e.payload;
        notice = `Planned ${e.payload.files} files into ${e.payload.folders} folders.`;
        await refreshTree(true);
      }),
      listen<string>("scan:error", (e) => fail(e.payload)),
      listen<CopyProgress>("copy:progress", (e) => (copyProgress = e.payload)),
      listen<{ report: CopyReport }>("copy:done", async (e) => {
        job = null;
        copyProgress = e.payload.report.progress;
        copyFailures = e.payload.report.failures;
        notice = `Copied ${e.payload.report.progress.copied}, ${e.payload.report.progress.duplicates} kept as copies, ${e.payload.report.progress.already_present} already there.`;
        await refreshTree(false);
      }),
      listen<string>("copy:error", (e) => fail(e.payload)),
      listen<VerifyProgress>("verify:progress", (e) => (verifyProgress = e.payload)),
      listen<{ report: VerifyReport }>("verify:done", (e) => {
        job = null;
        verifyProgress = null;
        verifyReport = e.payload.report;
        notice = `Checked: ${e.payload.report.ok} fine, ${e.payload.report.destination_missing} missing, ${e.payload.report.content_mismatch} differing.`;
        issuesOpen = e.payload.report.issues.length > 0;
      }),
      listen<string>("verify:error", (e) => fail(e.payload)),
    ]);

    if (settings.last_plan) {
      try {
        summary = await openPlan(settings.last_plan);
        await refreshTree(true);
        notice = "Reopened the last plan.";
      } catch {
        /* the plan is gone; start fresh */
      }
    }
  });

  onDestroy(() => unlisteners.forEach((un) => un()));

  function fail(message: string) {
    job = null;
    scanProgress = null;
    verifyProgress = null;
    error = message === "cancelled" ? "Stopped. Run it again to continue where it left off." : message;
  }

  async function updateSettings(next: Settings) {
    settings = next;
    try {
      await checkFolderPattern(next.folder_pattern);
      patternError = null;
    } catch (e) {
      patternError = String(e);
    }
    await saveSettings(next);
  }

  async function refreshTree(reset: boolean) {
    folders = await listFolders();
    skipped = await listSkipped();
    if (reset) {
      expanded = new Set(buildTree(folders).map((node) => node.path));
      selectedFolder = null;
      entries = [];
      selectedEntry = null;
      preview = null;
    } else if (selectedFolder !== null) {
      entries = await listEntries(selectedFolder);
    }
  }

  async function selectFolder(key: string) {
    selectedFolder = key;
    entries = await listEntries(key);
    selectedEntry = null;
    preview = null;
  }

  function toggleFolder(path: string) {
    const next = new Set(expanded);
    if (!next.delete(path)) {
      next.add(path);
    }
    expanded = next;
  }

  async function selectEntry(entry: EntryView) {
    selectedEntry = entry;
    preview = null;
    previewLoading = true;
    try {
      preview = await previewFile(entry.source);
    } finally {
      previewLoading = false;
    }
  }

  async function run(kind: Job, action: () => Promise<unknown>) {
    if (!settings) return;
    error = null;
    notice = null;
    job = kind;
    try {
      await action();
    } catch (e) {
      fail(String(e));
    }
  }

  const scan = () =>
    run("scan", async () => {
      verifyReport = null;
      copyFailures = [];
      copyProgress = null;
      await startScan({
        sources: settings!.sources,
        destination: settings!.destination!,
        folder_pattern: settings!.folder_pattern,
        providers: settings!.providers,
        strategy: settings!.strategy,
        follow_symlinks: settings!.follow_symlinks,
      });
    });

  const copy = () => run("copy", () => startCopy(settings!.jobs, settings!.preserve_times));
  const check = () => run("verify", () => startVerify(settings!.compare_hashes));

  async function openInSystem(path: string) {
    try {
      await openPath(path);
    } catch (e) {
      error = String(e);
    }
  }

  async function reveal(path: string) {
    try {
      await revealItemInDir(path);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="mark">◱</span>
      <div>
        <h1>Eonsort</h1>
        {#if summary}
          <p class="faint truncate" title={summary.destination}>
            {summary.files} files · {formatBytes(summary.bytes)} · into {summary.destination}
          </p>
        {:else}
          <p class="faint">Pick your folders, scan, review, then copy.</p>
        {/if}
      </div>
    </div>

    <div class="actions">
      <button class="primary" onclick={scan} disabled={!canScan}>Scan</button>
      <button onclick={copy} disabled={!canRun}>Copy files</button>
      <button onclick={check} disabled={!canRun}>Check result</button>
      {#if busy}
        <button class="danger" onclick={() => cancelJob()}>Stop</button>
      {/if}
    </div>
  </header>

  {#if settings}
    <main>
      <SetupPanel {settings} {busy} {patternError} onChange={updateSettings} />

      <div class="tree scroll" role="tree" aria-label="Planned folders">
        {#each tree as node (node.path)}
          <TreeItem
            {node}
            depth={0}
            selected={selectedFolder}
            {expanded}
            onSelect={selectFolder}
            onToggle={toggleFolder}
          />
        {:else}
          <p class="placeholder faint">Nothing planned yet. Run a scan to see the preview.</p>
        {/each}
      </div>

      <FileList
        {entries}
        folder={selectedFolder}
        selected={selectedEntry}
        onSelect={selectEntry}
        onOpen={(entry) => openInSystem(entry.source)}
      />

      <PreviewPane
        entry={selectedEntry}
        {preview}
        loading={previewLoading}
        onOpen={openInSystem}
        onReveal={reveal}
      />
    </main>
  {/if}

  {#if issuesOpen}
    <section class="issues scroll">
      <div class="issues-head">
        <strong>Issues</strong>
        <button class="ghost" onclick={() => (issuesOpen = false)}>Close</button>
      </div>
      {#each copyFailures as failure (failure.source)}
        <p><span class="badge danger">copy failed</span> <span class="mono">{failure.source}</span></p>
      {/each}
      {#each verifyReport?.issues ?? [] as issue (issue.source + issue.kind)}
        <p>
          <span class="badge warn">{issue.kind.replace("_", " ")}</span>
          <span class="mono">{issue.source}</span>
        </p>
      {/each}
      {#each skipped as item (item.source)}
        <p><span class="badge info">no date</span> <span class="mono">{item.source}</span></p>
      {/each}
      {#if issueCount === 0}
        <p class="faint">Nothing to report.</p>
      {/if}
    </section>
  {/if}

  <footer>
    <div class="status truncate">
      {#if scanProgress}
        {scanProgress.phase === "counting"
          ? `Counting files… ${scanProgress.files_seen}`
          : `Reading dates… ${scanProgress.files_seen} of ${scanProgress.files_total}`}
      {:else if job === "copy" && copyProgress}
        Copying {copyProgress.files_done} of {copyProgress.files_total} ·
        {formatBytes(copyProgress.bytes_done)} of {formatBytes(copyProgress.bytes_total)}
      {:else if verifyProgress}
        Checking {verifyProgress.checked} of {verifyProgress.total}
      {:else if error}
        <span class="error">{error}</span>
      {:else if notice}
        {notice}
      {:else}
        Ready.
      {/if}
    </div>

    {#if job === "copy" && copyProgress && copyProgress.bytes_total > 0}
      <div class="bar">
        <div
          class="fill"
          style="width: {Math.round((copyProgress.bytes_done / copyProgress.bytes_total) * 100)}%"
        ></div>
      </div>
    {:else if job === "scan" && scanProgress && scanProgress.files_total > 0}
      <div class="bar">
        <div
          class="fill"
          style="width: {Math.round((scanProgress.files_seen / scanProgress.files_total) * 100)}%"
        ></div>
      </div>
    {/if}

    <button class="ghost" onclick={() => (issuesOpen = !issuesOpen)}>
      Issues {issueCount > 0 ? `(${issueCount})` : ""}
    </button>
  </footer>
</div>

<style>
  .app {
    display: grid;
    grid-template-rows: auto 1fr auto auto;
    height: 100vh;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 14px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .mark {
    font-size: 22px;
    color: var(--accent);
    line-height: 1;
  }

  h1 {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .brand p {
    font-size: 11px;
  }

  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  main {
    display: grid;
    grid-template-columns: 300px 230px minmax(0, 1fr) 340px;
    min-height: 0;
  }

  .tree {
    background: var(--bg-panel);
    border-right: 1px solid var(--border);
    padding-block: 6px;
  }

  .placeholder {
    padding: 20px 12px;
    font-size: 12px;
    line-height: 1.5;
  }

  .issues {
    max-height: 180px;
    padding: 10px 14px;
    background: var(--bg-raised);
    border-top: 1px solid var(--border);
  }

  .issues-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .issues p {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
    font-size: 12px;
  }

  .issues .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px 6px 14px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border);
    font-size: 12px;
  }

  .status {
    flex: 1;
    min-width: 0;
    color: var(--text-dim);
  }

  .error {
    color: var(--danger);
  }

  .bar {
    width: 220px;
    height: 5px;
    border-radius: 3px;
    background: var(--bg-base);
    overflow: hidden;
    flex-shrink: 0;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s linear;
  }
</style>
