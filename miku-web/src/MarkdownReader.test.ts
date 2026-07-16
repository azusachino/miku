import { describe, expect, it } from "vitest";
import { expandInlineTags, expandWikiLinks, noteHref } from "./MarkdownReader";

describe("Markdown reader navigation", () => {
  it("keeps nested note paths readable in URLs", () => {
    expect(noteHref("projects/miku/home")).toBe("/p/projects/miku/home.md");
  });

  it("expands links and page embeds into reader navigation", () => {
    expect(expandWikiLinks("[[projects/miku/home|Home]]")).toContain("[Home](/p/projects/miku/home.md)");
    expect(expandWikiLinks("![[projects/miku/home]]")).toContain("Embedded note: [projects/miku/home](/p/projects/miku/home.md)");
  });

  it("links inline tags without rewriting code", () => {
    const result = expandInlineTags("Read #miku and `#literal`.");
    expect(result).toContain("[#miku](/tags/miku)");
    expect(result).toContain("`#literal`");
  });
});
