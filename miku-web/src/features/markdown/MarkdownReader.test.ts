import { describe, expect, it } from "vitest";
import { extractInlineTags } from "../workspace/api";
import { expandInlineTags, expandWikiLinks, mermaidTheme, noteHref, resolveMarkdownHref } from "./MarkdownReader";

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

  it("resolves relative Markdown links from the current note", () => {
    expect(resolveMarkdownHref("abc", "Design/Overview.md")).toBe("/p/Design/abc.md");
    expect(resolveMarkdownHref("../Shared/abc.md#part", "Design/Overview.md")).toBe("/p/Shared/abc.md#part");
    expect(resolveMarkdownHref("/p/abc/xx", "Design/Overview.md")).toBe("/p/abc/xx");
    expect(resolveMarkdownHref("https://example.com", "Design/Overview.md")).toBeNull();
  });

  it("extracts inline tags for note metadata while skipping code", () => {
    expect(extractInlineTags("#demo and #設計\n\n`#literal`\n\n```md\n#code\n```")).toEqual(["demo", "設計"]);
  });

  it("maps Mermaid rendering to the active shell theme", () => {
    expect(mermaidTheme("light")).toBe("default");
    expect(mermaidTheme("dark")).toBe("dark");
  });
});
