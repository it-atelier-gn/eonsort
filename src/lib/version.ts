import { getVersion } from "@tauri-apps/api/app";

export function versionLabel(version: string | null): string {
  const clean = (version ?? "").trim();
  return clean === "" ? "" : `v${clean}`;
}

export async function appVersion(): Promise<string | null> {
  try {
    return await getVersion();
  } catch {
    return null;
  }
}
