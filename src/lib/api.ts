import { invoke } from "@tauri-apps/api/core";

export type Provider =
  | "filename"
  | "exif"
  | "media"
  | "xmp"
  | "takeout"
  | "windows"
  | "filesystem";
export type Strategy = "smart" | "oldest" | "priority";
export type Confidence = "high" | "medium" | "low";

export interface Settings {
  sources: string[];
  destination: string | null;
  folder_pattern: string;
  providers: Provider[];
  strategy: Strategy;
  follow_symlinks: boolean;
  auto_rotate: boolean;
  pair_companions: boolean;
  tag_pictures: boolean;
  preserve_times: boolean;
  stamp_date: boolean;
  compare_hashes: boolean;
  last_plan: string | null;
}

export interface ScanRequest {
  sources: string[];
  destination: string | null;
  folder_pattern: string;
  providers: Provider[];
  strategy: Strategy;
  follow_symlinks: boolean;
  auto_rotate: boolean;
  pair_companions: boolean;
}

export interface PlanSummary {
  plan_path: string;
  sources: string[];
  destination: string | null;
  folder_pattern: string;
  files: number;
  bytes: number;
  skipped: number;
  folders: number;
  copied: number;
  duplicates: number;
  already_present: number;
  failed: number;
}

export interface FolderNode {
  path: string;
  files: number;
  bytes: number;
}

export interface CandidateView {
  provider: Provider;
  provider_info: string | null;
  taken: string;
  taken_epoch: number;
}

export interface FlagView {
  kind: string;
  description: string;
  hard: boolean;
}

export interface DuplicateView {
  sources: string[];
  folder: string;
  bytes: number;
  wasted: number;
}

export interface DuplicateReport {
  groups: DuplicateView[];
  files: number;
  wasted: number;
}

export interface BurstView {
  keeper: string;
  members: string[];
  folder: string;
  taken: string;
  extra_bytes: number;
}

export interface EntryView {
  source: string;
  destination: string;
  name: string;
  folder: string;
  taken: string;
  taken_epoch: number;
  provider: Provider;
  provider_info: string | null;
  size: number;
  destination_exists: boolean;
  outcome: string | null;
  candidates: CandidateView[];
  flags: FlagView[];
  confidence: Confidence;
  override_origin: string | null;
  orientation: number;
  rotate: Transform;
  rotate_by_hand: boolean;
  rotate_lossless: boolean;
  reencode: boolean;
  subject: string | null;
  tags: string[];
  caption: string | null;
}

export interface SkippedView {
  source: string;
  reason: string;
}

export interface SuspectGroup {
  key: string;
  kind: string;
  reason: string;
  folder: string;
  files: number;
  earliest: string;
  latest: string;
  sources: string[];
  destination_folders: string[];
}

export type DateChoice =
  | { kind: "candidate"; provider: Provider }
  | { kind: "manual"; taken: string };

export type Transform =
  | "none"
  | "rotate90"
  | "rotate180"
  | "rotate270"
  | "flip_h"
  | "flip_v"
  | "transpose"
  | "transverse";


export interface RotationProbe {
  lossless: boolean;
  reason: string | null;
}

export type Preview =
  | { kind: "image"; mime: string; data: string; bytes: number }
  | { kind: "video"; mime: string; bytes: number }
  | { kind: "audio"; mime: string; bytes: number }
  | { kind: "pdf"; bytes: number }
  | { kind: "text"; head: string; bytes: number; truncated: boolean }
  | { kind: "binary"; bytes: number }
  | { kind: "missing" };

export interface ScanProgress {
  phase: "counting" | "analysing";
  files_seen: number;
  files_total: number;
  bytes_total: number;
  current: string | null;
}

export interface CopyProgress {
  files_done: number;
  files_total: number;
  bytes_done: number;
  bytes_total: number;
  copied: number;
  duplicates: number;
  already_present: number;
  failed: number;
  turned: number;
  not_turned: number;
  current: string | null;
}

export interface VerifyProgress {
  checked: number;
  total: number;
  current: string | null;
}

export interface JournalRecord {
  source: string;
  status: string;
  destination?: string;
  error?: string;
}

export interface CopyReport {
  progress: CopyProgress;
  failures: JournalRecord[];
}

export interface VerifyIssue {
  kind: "source_missing" | "destination_missing" | "content_mismatch";
  source: string;
  destination: string;
  source_size: number;
  destination_size: number | null;
}

export interface VerifyReport {
  ok: number;
  source_missing: number;
  destination_missing: number;
  content_mismatch: number;
  source_bytes: number;
  destination_bytes: number;
  duplicate_files: number;
  duplicate_bytes: number;
  issues: VerifyIssue[];
}

export type Thumbnail =
  | { kind: "image"; data: string; width: number; height: number }
  | { kind: "playable"; mime: string }
  | { kind: "none" };

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const checkFolderPattern = (pattern: string) =>
  invoke<void>("check_folder_pattern", { pattern });
export const cancelJob = () => invoke<void>("cancel_job");
export const startScan = (request: ScanRequest) => invoke<string>("start_scan", { request });
export const startCopy = (preserveTimes: boolean, stampDate: boolean) =>
  invoke<void>("start_copy", { preserveTimes, stampDate });
export const findDuplicates = () => invoke<DuplicateReport>("find_duplicates");

export const findBursts = () => invoke<BurstView[]>("find_bursts");

export const startVerify = (compareHashes: boolean) =>
  invoke<void>("start_verify", { compareHashes });
export const openPlan = (path: string) => invoke<PlanSummary>("open_plan", { path });

export const setDestination = (destination: string | null) =>
  invoke<PlanSummary>("set_destination", { destination });
export const thumbnailFor = (path: string, edge: number, rotate?: Transform) =>
  invoke<Thumbnail>("thumbnail_for", { path, edge, rotate: rotate ?? null });
export const listFolders = () => invoke<FolderNode[]>("list_folders");
export const listAllEntries = () => invoke<EntryView[]>("list_all_entries");
export const listSkipped = () => invoke<SkippedView[]>("list_skipped");
export const listSuspects = () => invoke<SuspectGroup[]>("list_suspects");
export const setDateOverride = (source: string, choice: DateChoice) =>
  invoke<EntryView>("set_date_override", { source, choice });
export const clearDateOverride = (source: string) =>
  invoke<EntryView>("clear_date_override", { source });
export const shiftDates = (sources: string[], seconds: number) =>
  invoke<number>("shift_dates", { sources, seconds });
export const reproviderCluster = (sources: string[], provider: Provider) =>
  invoke<number>("reprovider_cluster", { sources, provider });
export const turnRotation = (source: string, quarterTurns: number) =>
  invoke<EntryView>("turn_rotation", { source, quarterTurns });
export const setRotation = (source: string, reencode: boolean) =>
  invoke<EntryView>("set_rotation", { source, reencode });
export const clearRotation = (source: string) =>
  invoke<EntryView>("clear_rotation", { source });
export const rotateMarked = (sources: string[], quarterTurns: number) =>
  invoke<number>("rotate_marked", { sources, quarterTurns });
export const probeRotation = (source: string) =>
  invoke<RotationProbe>("probe_rotation", { source });
export const previewFile = (path: string) => invoke<Preview>("preview_file", { path });

export interface TagModelStatus {
  present: boolean;
  bytes: number;
  total: number;
  built_in: boolean;
}

export interface TagProgress {
  done: number;
  total: number;
  current: string | null;
}

export interface TagHit {
  source: string;
  score: number;
}

export const tagModelStatus = () => invoke<TagModelStatus>("tag_model_status");
export const installTagModel = () => invoke<void>("install_tag_model");
export const cancelTagInstall = () => invoke<void>("cancel_tag_install");
export const startTagging = () => invoke<number>("start_tagging");
export const cancelTagging = () => invoke<void>("cancel_tagging");
export const listTags = () => invoke<Record<string, string[]>>("list_tags");
export const searchPictures = (words: string) => invoke<TagHit[]>("search_pictures", { words });
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
