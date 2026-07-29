import { describe, expect, it } from "vitest";
import { combineResolvers, createResolverFromOutgoingLinks, createTargetResolver, extractOutgoingLinks } from "./noteLinks";

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

describe("createResolverFromOutgoingLinks", () => {
  it("resolves a link the backend already resolved, even when the client-side notes list would have missed it", () => {
    // Regression: a wikilink written by title/alias (e.g. a Chinese
    // frontmatter title on an English-filename note) can fail to resolve
    // against the frontend's lazily-loaded notes list and fall back to
    // treating the raw title text as a literal path -- the backend
    // resolves every link server-side against the full page index
    // (ADR-0022), so preferring it avoids that wrong-href detour entirely.
    const resolve = createResolverFromOutgoingLinks([{ target: "一切都是童年的错吗", path: "apricot/note/books/2025/is-everything-the-fault-of-childhood.md" }]);
    expect(resolve("一切都是童年的错吗")).toBe("apricot/note/books/2025/is-everything-the-fault-of-childhood.md");
    expect(resolve("nonexistent")).toBeNull();
  });

  it("folds the same way the backend does, so a differently-spaced/hyphenated target still matches", () => {
    const resolve = createResolverFromOutgoingLinks([{ target: "Elden Ring", path: "elden-ring.md" }]);
    expect(resolve("elden-ring")).toBe("elden-ring.md");
    expect(resolve("Elden_Ring")).toBe("elden-ring.md");
  });
});

describe("combineResolvers", () => {
  it("prefers the primary resolver, falling back to the secondary only when the primary finds nothing", () => {
    const primary = createResolverFromOutgoingLinks([{ target: "known", path: "known.md" }]);
    const secondary = createTargetResolver([{ path: "fallback.md", title: "Fallback" }]);
    const resolve = combineResolvers(primary, secondary);
    expect(resolve("known")).toBe("known.md");
    expect(resolve("fallback")).toBe("fallback.md");
    expect(resolve("neither")).toBeNull();
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

  it("ignores links inside fenced and inline code", () => {
    const body = ["```markdown", "[wikilinks](/p/wikilinks.md)", "[[fenced-wikilink]]", "```", "~~~md", "[tilde](/p/tilde.md)", "~~~", "`[inline](/p/inline.md)`", "[visible](/p/visible.md)"].join(
      "\n"
    );

    expect(extractOutgoingLinks(body)).toEqual([{ path: "visible.md", title: "visible", isMissing: true }]);
  });
});
