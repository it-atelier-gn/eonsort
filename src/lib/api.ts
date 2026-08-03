import { invoke } from "@tauri-apps/api/core";

export type Provider = "filename" | "exif" | "media" | "filesystem";
export type Strategy = "oldest" | "priority";

export interface Settings {
  sources: string[];
  destination: string | null;
  folder_pattern: string;
  providers: Provider[];
  strategy: Strategy;
  follow_symlinks: boolean;
  jobs: number;
  preserve_times: boolean;
  compare_hashes: boolean;
  last_plan: string | null;
}

export interface ScanRequest {
  sources: string[];
  destination: string;
  folder_pattern: string;
  providers: Provider[];
  strategy: Strategy;
  follow_symlinks: boolean;
}

export interface PlanSummary {
  plan_path: string;
  sources: string[];
  destination: string;
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

export interface EntryView {
  source: string;
  destination: string;
  name: string;
  folder: string;
  taken: string;
  provider: Provider;
  provider_info: string | null;
  size: number;
  destination_exists: boolean;
  outcome: string | null;
}

export interface SkippedView {
  source: string;
  reason: string;
}

export type Preview =
  | { kind: "image"; mime: string; data: string; bytes: number }
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

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const checkFolderPattern = (pattern: string) =>
  invoke<void>("check_folder_pattern", { pattern });
export const cancelJob = () => invoke<void>("cancel_job");
export const startScan = (request: ScanRequest) => invoke<string>("start_scan", { request });
export const startCopy = (jobs: number, preserveTimes: boolean) =>
  invoke<void>("start_copy", { jobs, preserveTimes });
export const startVerify = (compareHashes: boolean) =>
  invoke<void>("start_verify", { compareHashes });
export const openPlan = (path: string) => invoke<PlanSummary>("open_plan", { path });
export const listFolders = () => invoke<FolderNode[]>("list_folders");
export const listEntries = (folder: string) => invoke<EntryView[]>("list_entries", { folder });
export const listSkipped = () => invoke<SkippedView[]>("list_skipped");
export const previewFile = (path: string) => invoke<Preview>("preview_file", { path });

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
