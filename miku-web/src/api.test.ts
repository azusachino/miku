import { describe, expect, it } from "vitest";
import type { TreeNodeModel } from "./api";
import { sortTreeNodes } from "./api";

const node = (kind: TreeNodeModel["kind"], path: string, title = path): TreeNodeModel => ({
  kind,
  path,
  hasChildren: kind === "folder",
  placementId: `path:${path}`,
  noteId: path,
  parentId: null,
  note: { id: path, path, title, identityGenerated: false, parents: [] }
});

describe("tree ordering", () => {
  it("places folders first and sorts each group case-insensitively", () => {
    const sorted = sortTreeNodes([node("markdown", "z.md", "zeta"), node("folder", "b"), node("folder", "A"), node("markdown", "a.md", "Alpha")]);
    expect(sorted.map((item) => item.path)).toEqual(["A", "b", "a.md", "z.md"]);
  });
});
