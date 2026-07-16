import { describe, expect, it } from "vitest";
import { normalizeNotePath } from "./noteRoute";

describe("note route normalization", () => {
  it("treats extensionless note paths as Markdown files", () => {
    expect(normalizeNotePath("Design/Home")).toBe("Design/Home.md");
    expect(normalizeNotePath("Design/Home.md")).toBe("Design/Home.md");
  });
});
