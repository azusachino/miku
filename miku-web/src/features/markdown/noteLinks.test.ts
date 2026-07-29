import { describe, expect, it } from "vitest";
import { createTargetResolver, extractOutgoingLinks } from "./noteLinks";

describe("createTargetResolver", () => {
  const notes = [
    { path: "elden-ring.md", title: "Elden Ring" },
    { path: "Games/Boss-Log.md", title: "Elden Ring Boss Log", aliases: ["ER Boss Log"] }
  ];

  it("folds whitespace, hyphens, and underscores to match a filename", () => {
    const resolve = createTargetResolver(notes);
    expect(resolve("elden ring")).toBe("elden-ring.md");
    expect(resolve("Elden_Ring")).toBe("elden-ring.md");
  });

  it("matches a note by its title even when unrelated to the filename", () => {
    const resolve = createTargetResolver(notes);
    expect(resolve("elden-ring boss_log")).toBe("Games/Boss-Log.md");
  });

  it("matches a note by a frontmatter alias", () => {
    const resolve = createTargetResolver(notes);
    expect(resolve("er-boss-log")).toBe("Games/Boss-Log.md");
  });

  it("returns null when nothing matches", () => {
    const resolve = createTargetResolver(notes);
    expect(resolve("nonexistent note")).toBeNull();
  });
});

describe("extractOutgoingLinks", () => {
  it("resolves wikilinks that use a display alias distinct from the target", () => {
    const notes = [{ path: "abc.md", title: "abc" }];
    const links = extractOutgoingLinks("See [[abc|这是abc]] for details.", notes);
    expect(links).toEqual([{ path: "abc.md", title: "这是abc", isMissing: false }]);
  });

  it("resolves a wikilink target by folded filename without a display alias", () => {
    const notes = [{ path: "elden-ring.md", title: "Elden Ring" }];
    const links = extractOutgoingLinks("[[elden ring]]", notes);
    expect(links).toEqual([{ path: "elden-ring.md", title: "Elden Ring", isMissing: false }]);
  });

  it("resolves outgoing links in subfolders to their real canonical path", () => {
    const notes = [{ path: "vault/maps/reference-note.md", title: "Reference" }];
    const links = extractOutgoingLinks("- [[reference-note]]\n- [[uncreated-note]]", notes, "vault/maps/topic-map.md");
    expect(links).toEqual([
      { path: "vault/maps/reference-note.md", title: "Reference", isMissing: false },
      { path: "vault/maps/uncreated-note.md", title: "uncreated-note", isMissing: true }
    ]);
  });
});

