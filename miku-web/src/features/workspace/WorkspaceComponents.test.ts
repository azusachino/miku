import { describe, expect, it } from "vitest";
import { curatedFrontmatter } from "./WorkspaceComponents";

describe("curatedFrontmatter", () => {
  it("orders meaningful properties and removes values rendered elsewhere", () => {
    expect(
      curatedFrontmatter({
        impacts: ["miku-web"],
        tags: ["miku"],
        title: "Miku Note",
        superseded_by: [],
        status: "active",
        type: "guide",
        id: "guide-1",
        note: null
      })
    ).toEqual([
      ["type", "guide"],
      ["status", "active"],
      ["id", "guide-1"],
      ["impacts", ["miku-web"]]
    ]);
  });
});
