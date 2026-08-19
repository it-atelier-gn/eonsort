<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    cancelJob,
    checkFolderPattern,
    clearDateOverride,
    clearRotation,
    formatBytes,
    getSettings,
    cancelTagging,
    listAllEntries,
    listTags,
    searchPictures,
    startTagging,
    tagModelStatus,
    listFolders,
    listSkipped,
    listSuspects,
    openPlan,
    previewFile,
    reproviderCluster,
    rotateMarked,
    saveSettings,
    setDateOverride,
    setRotation,
    shiftDates,
    turnRotation,
    startCopy,
    setDestination,
    startScan,
    startVerify,
    type CopyProgress,
    type CopyReport,
    type DateChoice,
    type EntryView,
    type FolderNode,
    type PlanSummary,
    type Preview,
    type Provider,
    type TagHit,
    type TagProgress,
    type ScanProgress,
    type Settings,
    type SkippedView,
    type SuspectGroup,
    type VerifyProgress,
    type VerifyReport,
  } from "$lib/api";
  import { buildTree, foldersOf, under, type TreeNode } from "$lib/tree";
  import TreeHeader from "$lib/components/TreeHeader.svelte";
  import {
    cleanOrder,
    cleanWidths,
    template,
    widthOf,
    withWidth,
    ORDER_KEY,
    WIDTH_KEY,
    type ColumnId,
    type ColumnWidths,
  } from "$lib/columns";
  import { appVersion, versionLabel } from "$lib/version";
  import SetupPanel from "$lib/components/SetupPanel.svelte";
  import TreeItem from "$lib/components/TreeItem.svelte";
  import FileList from "$lib/components/FileList.svelte";
  import PreviewPane from "$lib/components/PreviewPane.svelte";
  import DateFixPanel from "$lib/components/DateFixPanel.svelte";
  import TimeScape from "$lib/components/TimeScape.svelte";
  import ChartsPanel from "$lib/components/ChartsPanel.svelte";
  import GalleryView from "$lib/components/GalleryView.svelte";
  import ScopeBar from "$lib/components/ScopeBar.svelte";
  import { filterRange, sameRange, type TimeRange } from "$lib/viz/range";

  type Job = "scan" | "copy" | "verify";

  let settings = $state<Settings | null>(null);
  let summary = $state<PlanSummary | null>(null);
  let folders = $state<FolderNode[]>([]);
  let expanded = $state(new Set<string>());
  let selectedFolder = $state<string | null>(null);
  let selectedEntry = $state<EntryView | null>(null);
  let marked = $state<string[]>([]);
  let preview = $state<Preview | null>(null);
  let previewLoading = $state(false);
  let suspects = $state<SuspectGroup[]>([]);
  let fixing = $state(false);
  let view = $state<"folders" | "timeline" | "charts" | "gallery">("folders");
  let timelineEntries = $state<EntryView[]>([]);
  let loadingAll = $state(false);
  let tagProgress = $state<TagProgress | null>(null);
  let tagging = $state(false);
  let tagNote = $state<string | null>(null);
  let tagsBySource = $state<Record<string, string[]>>({});
  let query = $state("");
  let hits = $state<TagHit[] | null>(null);
  let searching = $state(false);
  let scopes = $state<TimeRange[]>([]);

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

  let columnOrder = $state<ColumnId[]>(rememberedOrder());
  let columnWidths = $state<ColumnWidths>(rememberedWidths());
  let version = $state<string | null>(null);

  function rememberedOrder(): ColumnId[] {
    if (typeof localStorage === "undefined") return cleanOrder(null);
    try {
      const held = localStorage.getItem(ORDER_KEY);
      return cleanOrder(held === null ? null : JSON.parse(held));
    } catch {
      return cleanOrder(null);
    }
  }

  function reorderColumns(next: ColumnId[]) {
    columnOrder = next;
    remember(ORDER_KEY, next);
  }

  function rememberedWidths(): ColumnWidths {
    if (typeof localStorage === "undefined") return {};
    try {
      const held = localStorage.getItem(WIDTH_KEY);
      return cleanWidths(held === null ? null : JSON.parse(held));
    } catch {
      return {};
    }
  }

  function resizeColumn(id: ColumnId, width: number | null) {
    columnWidths = withWidth(columnWidths, id, width);
    remember(WIDTH_KEY, columnWidths);
  }

  function remember(key: string, value: unknown) {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      // a layout we cannot remember is not worth interrupting anyone over
    }
  }

  const busy = $derived(job !== null);
  const canScan = $derived(
    !busy &&
      settings !== null &&
      settings.sources.length > 0 &&
      settings.providers.length > 0 &&
      patternError === null,
  );
  const canRun = $derived(
    !busy && summary !== null && summary.files > 0 && summary.destination !== null,
  );
  const runHint = $derived(
    summary !== null && summary.destination === null
      ? "Choose a destination folder first"
      : undefined,
  );
  const suspectCount = $derived(suspects.reduce((total, group) => total + group.files, 0));
  const issueCount = $derived(
    skipped.length + copyFailures.length + (verifyReport?.issues.length ?? 0) + suspects.length,
  );
  const scope = $derived<TimeRange | null>(scopes[scopes.length - 1] ?? null);
  const taggedEntries = $derived(
    timelineEntries.map((entry) => {
      const seen = tagsBySource[entry.source];
      return seen && seen.length > 0 ? { ...entry, tags: seen } : entry;
    }),
  );
  const foundEntries = $derived(
    hits === null
      ? taggedEntries
      : hits
          .map((hit) => taggedEntries.find((entry) => entry.source === hit.source))
          .filter((entry): entry is EntryView => entry !== undefined),
  );
  const scopedEntries = $derived(filterRange(foundEntries, scope));
  const entries = $derived(under(scopedEntries, selectedFolder));
  const markedEntries = $derived(entries.filter((entry) => marked.includes(entry.source)));

  const tree = $derived<TreeNode[]>(buildTree(foldersOf(scopedEntries)));
  const flatTree = $derived(flatten(tree));
  const columnGrid = $derived(
    template(columnOrder, {
      name: columnWidths.name ?? 0,
      files:
        columnWidths.files ??
        widthOf(
          "files",
          flatTree.map((node) => String(node.files)),
        ),
      size:
        columnWidths.size ??
        widthOf(
          "size",
          flatTree.map((node) => formatBytes(node.bytes)),
        ),
    }),
  );

  function flatten(nodes: TreeNode[]): TreeNode[] {
    return nodes.flatMap((node) => [node, ...flatten(node.children)]);
  }

  function drill(next: TimeRange) {
    if (sameRange(next, scope)) return;
    scopes = [...scopes, next];
  }

  function popScope(depth: number) {
    scopes = scopes.slice(0, Math.max(0, depth));
  }

  let unlisteners: UnlistenFn[] = [];

  onMount(async () => {
    settings = await getSettings();
    version = await appVersion();

    unlisteners = await Promise.all([
      listen<ScanProgress>("scan:progress", (e) => (scanProgress = e.payload)),
      listen<PlanSummary>("scan:done", async (e) => {
        job = null;
        scanProgress = null;
        summary = e.payload;
        notice = `Planned ${e.payload.files} files into ${e.payload.folders} folders.`;
        await refreshTree(true);
        if (settings?.tag_pictures) void beginTagging();
      }),
      listen<string>("scan:error", (e) => fail(e.payload)),
      listen<TagProgress>("tags:progress", (e) => (tagProgress = e.payload)),
      listen<number>("tags:done", async () => {
        tagProgress = null;
        tagging = false;
        await refreshTags();
      }),
      listen<string>("tags:error", (e) => {
        tagProgress = null;
        tagging = false;
        tagNote = e.payload;
      }),
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
    const retarget = summary !== null && next.destination !== summary.destination;
    settings = next;
    try {
      await checkFolderPattern(next.folder_pattern);
      patternError = null;
    } catch (e) {
      patternError = String(e);
    }
    await saveSettings(next);

    if (retarget) {
      try {
        summary = await setDestination(next.destination);
        await refreshTree(false);
        notice = next.destination
          ? `The plan now copies into ${next.destination}.`
          : "The plan has no destination folder.";
      } catch (e) {
        error = String(e);
      }
    }
  }

  async function beginTagging() {
    tagNote = null;
    try {
      const status = await tagModelStatus();
      if (!status.built_in) {
        tagNote = "This build was made without the tagging model.";
        return;
      }
      if (!status.present) {
        tagNote = "The tagging model is not downloaded yet — get it in the setup panel.";
        return;
      }
      tagging = true;
      await startTagging();
    } catch (e) {
      tagging = false;
      tagNote = String(e);
    }
  }

  async function refreshTags() {
    try {
      tagsBySource = await listTags();
    } catch {
      tagsBySource = {};
    }
  }

  async function look() {
    const words = query.trim();
    if (words === "") {
      hits = null;
      return;
    }
    searching = true;
    try {
      hits = await searchPictures(words);
    } catch (e) {
      tagNote = String(e);
      hits = null;
    } finally {
      searching = false;
    }
  }

  function clearSearch() {
    query = "";
    hits = null;
  }

  async function refreshTree(reset: boolean) {
    folders = await listFolders();
    loadingAll = true;
    try {
      timelineEntries = await listAllEntries();
    } finally {
      loadingAll = false;
    }
    await refreshTags();
    skipped = await listSkipped();
    suspects = await listSuspects();
    if (reset) {
      scopes = [];
      expanded = new Set(buildTree(folders).map((node) => node.path));
      selectedFolder = null;
      selectedEntry = null;
      marked = [];
      preview = null;
    }
  }

  function selectFolder(key: string) {
    selectedFolder = key;
    selectedEntry = null;
    marked = [];
    preview = null;
  }

  function showAll(next: "timeline" | "charts" | "gallery") {
    view = next;
  }

  async function afterFix(message: string) {
    folders = await listFolders();
    suspects = await listSuspects();

    timelineEntries = await listAllEntries();
    marked = marked.filter((source) => entries.some((entry) => entry.source === source));

    const pool = view === "folders" ? entries : timelineEntries;
    selectedEntry = pool.find((entry) => entry.source === selectedEntry?.source) ?? null;

    if (summary) {
      summary = { ...summary, folders: folders.length };
    }
    notice = message;
    error = null;
  }

  async function fix<T>(action: () => Promise<T>, describe: (result: T) => string) {
    if (fixing) return;
    fixing = true;
    try {
      const result = await action();
      await afterFix(describe(result));
    } catch (e) {
      error = String(e);
    } finally {
      fixing = false;
    }
  }

  const chooseDate = (choice: DateChoice) => {
    const source = selectedEntry?.source;
    if (!source) return;
    return fix(
      () => setDateOverride(source, choice),
      (entry) => `Moved into ${entry.folder || "the destination root"}.`,
    );
  };

  const revertDate = () => {
    const source = selectedEntry?.source;
    if (!source) return;
    return fix(
      () => clearDateOverride(source),
      (entry) => `Back to the detected date, ${entry.taken}.`,
    );
  };

  function patchEntry(updated: EntryView) {
    const swap = (list: EntryView[]) =>
      list.map((entry) => (entry.source === updated.source ? updated : entry));
    timelineEntries = swap(timelineEntries);
    selectedEntry = updated;
  }

  async function turnSelected<T extends EntryView>(action: () => Promise<T>, message: string) {
    if (fixing) return;
    fixing = true;
    try {
      patchEntry(await action());
      notice = message;
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      fixing = false;
    }
  }

  const turnEntry = (quarterTurns: number) => {
    const source = selectedEntry?.source;
    if (!source) return;
    return turnSelected(() => turnRotation(source, quarterTurns), "Turned.");
  };

  const resetTurn = () => {
    const source = selectedEntry?.source;
    if (!source) return;
    return turnSelected(() => clearRotation(source), "Back to the detected orientation.");
  };

  const allowReencode = () => {
    const source = selectedEntry?.source;
    if (!source) return;
    return turnSelected(
      () => setRotation(source, true),
      "This one will be re-encoded when it is copied.",
    );
  };

  const rotateMarkedFiles = (sources: string[], quarterTurns: number) =>
    fix(
      () => rotateMarked(sources, quarterTurns),
      (count) => `Turned ${count} ${count === 1 ? "file" : "files"}.`,
    );

  function onKeydown(event: KeyboardEvent) {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (view === "gallery" || busy || fixing) return;
    const target = event.target as HTMLElement | null;
    if (
      target &&
      (target.isContentEditable ||
        ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName))
    ) {
      return;
    }
    if (!selectedEntry || selectedEntry.orientation === 0) return;

    switch (event.key) {
      case "[":
        event.preventDefault();
        turnEntry(-1);
        break;
      case "]":
        event.preventDefault();
        turnEntry(1);
        break;
      case "\\":
        event.preventDefault();
        turnEntry(2);
        break;
      case "0":
        event.preventDefault();
        resetTurn();
        break;
    }
  }

  const shiftMarked = (sources: string[], seconds: number) =>
    fix(
      () => shiftDates(sources, seconds),
      (count) => `Shifted ${count} ${count === 1 ? "file" : "files"}.`,
    );

  const reproviderMarked = (sources: string[], provider: Provider) =>
    fix(
      () => reproviderCluster(sources, provider),
      (count) => `Re-dated ${count} ${count === 1 ? "file" : "files"} from ${provider}.`,
    );

  async function selectSuspects(group: SuspectGroup) {
    const folder = group.destination_folders[0] ?? "";
    if (folder !== selectedFolder) {
      await selectFolder(folder);
      expanded = new Set([...expanded, ...ancestors(folder)]);
    }

    marked = group.sources.filter((source) => entries.some((entry) => entry.source === source));
    selectedEntry = entries.find((entry) => entry.source === marked[0]) ?? null;
    if (selectedEntry) await selectEntry(selectedEntry);

    issuesOpen = false;
    if (group.destination_folders.length > 1) {
      notice = `Showing the ${marked.length} of ${group.files} that land in ${folder || "the destination root"}.`;
    }
  }

  function ancestors(folder: string): string[] {
    const parts = folder.split("/").filter(Boolean);
    return parts.map((_, index) => parts.slice(0, index + 1).join("/"));
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
        destination: settings!.destination,
        folder_pattern: settings!.folder_pattern,
        providers: settings!.providers,
        strategy: settings!.strategy,
        follow_symlinks: settings!.follow_symlinks,
        auto_rotate: settings!.auto_rotate,
        pair_companions: settings!.pair_companions,
      });
    });

  const copy = () =>
    run("copy", () => startCopy(settings!.jobs, settings!.preserve_times, settings!.stamp_date));
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

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <header>
    <div class="brand">
      <span class="mark">◱</span>
      <div>
        <h1>Eonsort</h1>
        {#if summary}
          <p class="faint truncate" title={summary.destination ?? "No destination chosen yet"}>
            {summary.files} files · {formatBytes(summary.bytes)} ·
            {summary.destination
              ? `into ${summary.destination}`
              : "no destination yet — pick one to copy"}
          </p>
        {:else}
          <p class="faint">Pick your folders, scan, review, then copy.</p>
        {/if}
      </div>
    </div>

    <div class="actions">
      <div class="views">
        <button class:active={view === "folders"} onclick={() => (view = "folders")}>Folders</button>
        <button class:active={view === "timeline"} onclick={() => showAll("timeline")}>
          Timeline
        </button>
        <button class:active={view === "charts"} onclick={() => showAll("charts")}>Charts</button>
        <button class:active={view === "gallery"} onclick={() => showAll("gallery")}>Gallery</button>
      </div>
      <div class="look">
        <input
          type="search"
          placeholder="forest and dog"
          bind:value={query}
          onkeydown={(e) => {
            if (e.key === "Enter") void look();
            if (e.key === "Escape") clearSearch();
          }}
        />
        <button onclick={() => void look()} disabled={searching || query.trim() === ""}>
          {searching ? "Looking…" : "Find"}
        </button>
        {#if hits !== null}
          <button class="ghost" onclick={clearSearch}>Clear</button>
        {/if}
      </div>
      <button class="primary" onclick={scan} disabled={!canScan}>Scan</button>
      <button onclick={copy} disabled={!canRun} title={runHint}>Copy files</button>
      <button onclick={check} disabled={!canRun} title={runHint}>Check result</button>
      {#if busy}
        <button class="danger" onclick={() => cancelJob()}>Stop</button>
      {/if}
    </div>
  </header>

  {#if tagProgress || tagNote || hits !== null}
    <div class="tagbar faint tiny">
      {#if tagProgress}
        <span class="spinner"></span>
        <span>
          Looking at pictures — {tagProgress.done.toLocaleString()} of {tagProgress.total.toLocaleString()}
        </span>
        <button class="ghost" onclick={() => void cancelTagging()}>Stop</button>
      {:else if hits !== null}
        <span>{hits.length.toLocaleString()} pictures match &ldquo;{query.trim()}&rdquo;</span>
        <button class="ghost" onclick={clearSearch}>Show all again</button>
      {/if}
      {#if tagNote}
        <span>{tagNote}</span>
      {/if}
    </div>
  {/if}

  {#if scopes.length > 0}
    <ScopeBar
      {scopes}
      shown={scopedEntries.length}
      total={timelineEntries.length}
      onPop={popScope}
    />
  {/if}

  {#if settings}
    <main class:wide={view !== "folders"}>
      <SetupPanel {settings} {busy} {patternError} onChange={updateSettings} />

      {#if loadingAll}
        <div class="loading faint">
          <span class="spinner"></span>
          Reading {summary ? summary.files.toLocaleString() : ""} files out of the plan…
        </div>
      {:else if view === "charts"}
        <ChartsPanel entries={scopedEntries} range={scope} onDrill={drill} />
      {:else if view === "gallery"}
        <GalleryView entries={scopedEntries} onSelect={selectEntry} />
      {:else if view === "timeline"}
        <TimeScape entries={scopedEntries} selected={selectedEntry} onSelect={selectEntry} />
      {:else}
        <div class="tree scroll" role="tree" aria-label="Planned folders">
          <TreeHeader
            order={columnOrder}
            grid={columnGrid}
            onReorder={reorderColumns}
            onResize={resizeColumn}
          />
          {#each tree as node (node.path)}
            <TreeItem
              {node}
              depth={0}
              selected={selectedFolder}
              {expanded}
              order={columnOrder}
              grid={columnGrid}
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
          {marked}
          onSelect={selectEntry}
          onMark={(sources) => (marked = sources)}
          onOpen={(entry) => openInSystem(entry.source)}
        />
      {/if}

      <PreviewPane
        entry={selectedEntry}
        {preview}
        loading={previewLoading}
        busy={fixing || busy}
        onOpen={openInSystem}
        onReveal={reveal}
        onChoose={chooseDate}
        onRevert={revertDate}
        onTurn={turnEntry}
        onResetTurn={resetTurn}
        onReencode={allowReencode}
      />
    </main>
  {/if}

  {#if markedEntries.length > 1 && view === "folders"}
    <DateFixPanel
      entries={markedEntries}
      busy={fixing || busy}
      onShift={shiftMarked}
      onReprovider={reproviderMarked}
      onRotate={rotateMarkedFiles}
      onClear={() => (marked = [])}
    />
  {/if}

  {#if issuesOpen}
    <section class="issues scroll">
      <div class="issues-head">
        <strong>Issues</strong>
        <button class="ghost" onclick={() => (issuesOpen = false)}>Close</button>
      </div>
      {#each suspects as group (group.key)}
        <button class="suspect" onclick={() => selectSuspects(group)}>
          <span class="badge danger">{group.files} dates look wrong</span>
          <span class="reason">Each one {group.reason}.</span>
          <span class="mono faint truncate">{group.folder}</span>
          <span class="mono faint">{group.earliest} → {group.latest}</span>
        </button>
      {/each}
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

    {#if suspectCount > 0}
      <button class="ghost warn-text" onclick={() => (issuesOpen = true)}>
        {suspectCount} suspicious {suspectCount === 1 ? "date" : "dates"}
      </button>
    {/if}

    <button class="ghost" onclick={() => (issuesOpen = !issuesOpen)}>
      Issues {issueCount > 0 ? `(${issueCount})` : ""}
    </button>

    {#if versionLabel(version)}
      <span class="split" aria-hidden="true"></span>
      <span class="version">{versionLabel(version)}</span>
    {/if}
  </footer>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
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
    flex: 1;
    min-height: 0;
  }

  main.wide {
    grid-template-columns: 300px minmax(0, 1fr) 340px;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    font-size: 12px;
    background: var(--bg-panel);
  }

  .spinner {
    width: 13px;
    height: 13px;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .views {
    display: flex;
    gap: 2px;
    margin-right: 6px;
  }

  .views button.active {
    border-color: var(--accent);
    background: var(--bg-active);
    color: var(--text);
  }

  .look {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .look input {
    width: 190px;
  }

  .tagbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    border-bottom: 1px solid var(--border);
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
    flex-shrink: 0;
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

  .suspect {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    padding: 5px 8px;
    margin-bottom: 4px;
    font-size: 12px;
  }

  .suspect:hover {
    border-color: var(--danger);
  }

  .suspect .reason {
    color: var(--text);
  }

  .suspect .faint {
    font-size: 11px;
  }

  .warn-text {
    color: var(--warn);
  }

  footer {
    display: flex;
    flex-shrink: 0;
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

  .split {
    width: 1px;
    height: 12px;
    background: var(--border);
    flex-shrink: 0;
  }

  .version {
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
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
