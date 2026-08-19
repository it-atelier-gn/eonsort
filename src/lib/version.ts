import { getVersion } from "@tauri-apps/api/app";

export const RELEASES = "https://github.com/it-atelier-gn/eonsort/releases";

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

export function releaseUrl(version: string | null): string {
  const clean = (version ?? "").trim();
  return clean === "" ? RELEASES : `${RELEASES}/tag/v${clean}`;
}
