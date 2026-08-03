import type { FolderNode } from "./api";

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

export function isLeafFolder(node: TreeNode): boolean {
  return node.children.length === 0;
}

function sort(nodes: TreeNode[]) {
  nodes.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  for (const node of nodes) {
    sort(node.children);
  }
}
