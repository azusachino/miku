import { describe, expect, it } from "vitest";
import type { TreeNodeModel } from "./api";
import { formatUpdatedAt, sortTreeNodes } from "./api";

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

describe("updated timestamp formatting", () => {
  it("uses an unambiguous 24-hour local timestamp", () => {
    const timestamp = new Date(2026, 6, 14, 17, 58, 9).getTime() / 1000;
    expect(formatUpdatedAt(timestamp)).toBe("2026-07-14 17:58:09");
  });

  it("falls back when the revision timestamp is unavailable", () => {
    expect(formatUpdatedAt(null)).toBe("unknown");
    expect(formatUpdatedAt(Number.NaN)).toBe("unknown");
  });
});
