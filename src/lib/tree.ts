import type { EntryView, FolderNode } from "./api";

export interface TreeNode {
  name: string;
  path: string;
  files: number;
  bytes: number;
  children: TreeNode[];
}

const ROOT_LABEL = "(destination root)";

/** Turns the flat `2023/05` folder list into the nested tree the explorer renders. */
export function buildTree(folders: FolderNode[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const index = new Map<string, TreeNode>();

  for (const folder of folders) {
    const segments = folder.path === "" ? [ROOT_LABEL] : folder.path.split("/");
    let prefix = "";
    let siblings = roots;

    for (const segment of segments) {
      prefix = prefix === "" ? segment : `${prefix}/${segment}`;
      let node = index.get(prefix);
      if (!node) {
        node = { name: segment, path: prefix, files: 0, bytes: 0, children: [] };
        index.set(prefix, node);
        siblings.push(node);
      }
      node.files += folder.files;
      node.bytes += folder.bytes;
      siblings = node.children;
    }
  }

  sort(roots);
  return roots;
}

/** The tree path of a node maps back to the folder key the backend uses. */
export function folderKey(path: string): string {
  return path === ROOT_LABEL ? "" : path;
}

export function foldersOf(entries: EntryView[]): FolderNode[] {
  const totals = new Map<string, FolderNode>();

  for (const entry of entries) {
    const path = entry.folder;
    const node = totals.get(path);
    if (node) {
      node.files += 1;
      node.bytes += entry.size;
    } else {
      totals.set(path, { path, files: 1, bytes: entry.size });
    }
  }

  return [...totals.values()].sort((a, b) =>
    a.path.localeCompare(b.path, undefined, { numeric: true }),
  );
}

export function under(entries: EntryView[], key: string | null): EntryView[] {
  if (key === null) return [];
  if (key === "") return entries.filter((entry) => entry.folder === "");
  return entries.filter((entry) => entry.folder === key || entry.folder.startsWith(`${key}/`));
}

export function isLeafFolder(node: TreeNode): boolean {
  return node.children.length === 0;
}

function sort(nodes: TreeNode[]) {
  nodes.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  for (const node of nodes) {
    sort(node.children);
  }
}
