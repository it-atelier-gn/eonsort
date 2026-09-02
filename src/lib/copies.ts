import type { DuplicateReport } from "$lib/api";

export const LIST_LIMIT = 200;

export interface Listing<T> {
  shown: T[];
  hidden: number;
}

export function listed<T>(items: T[], limit: number = LIST_LIMIT): Listing<T> {
  return {
    shown: items.slice(0, limit),
    hidden: Math.max(0, items.length - limit),
  };
}

export function removableCopies(report: DuplicateReport | null): number {
  if (!report) return 0;
  return Math.max(0, report.files - report.groups.length);
}

export interface MemberSet {
  members: string[];
  extra_bytes: number;
}

export interface BurstTally {
  bursts: number;
  files: number;
  extra: number;
}

export function tallyBursts(sets: MemberSet[]): BurstTally {
  return {
    bursts: sets.length,
    files: sets.reduce((sum, set) => sum + set.members.length, 0),
    extra: sets.reduce((sum, set) => sum + set.extra_bytes, 0),
  };
}
