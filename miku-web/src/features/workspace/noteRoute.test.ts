// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import type { NavigateFunction } from "react-router-dom";
import { normalizeNotePath, useNoteRouteRecovery } from "./noteRoute";

describe("note route normalization", () => {
  it("treats extensionless note paths as Markdown files", () => {
    expect(normalizeNotePath("Design/Home")).toBe("Design/Home.md");
    expect(normalizeNotePath("Design/Home.md")).toBe("Design/Home.md");
  });
});

describe("useNoteRouteRecovery", () => {
  const baseOptions = () => ({
    activeId: "target-title.md",
    isNoteRoute: true,
    isError: false,
    hasNote: true,
    tabs: [] as string[],
    dispatch: vi.fn(),
    navigate: vi.fn() as unknown as NavigateFunction,
    setNotice: vi.fn()
  });

  it("does not open a tab under the pre-redirect id while a canonical redirect is pending", () => {
    // Regression test: a wikilink or URL that resolves (server-side) to a
    // different canonical path used to race with the canonicalization
    // effect in WorkspaceApp -- this hook would open a tab under the stale
    // id before the redirect landed, leaving a duplicate tab once the
    // canonical id opened too.
    const options = baseOptions();
    renderHook(() => useNoteRouteRecovery({ ...options, canonicalId: "real-path.md" }));
    expect(options.dispatch).not.toHaveBeenCalled();
  });

  it("opens the tab once activeId already matches the canonical id", () => {
    const options = baseOptions();
    renderHook(() => useNoteRouteRecovery({ ...options, canonicalId: "target-title.md" }));
    expect(options.dispatch).toHaveBeenCalledWith({ type: "open", id: "target-title.md" });
  });

  it("opens the tab when no canonical id is known yet", () => {
    const options = baseOptions();
    renderHook(() => useNoteRouteRecovery({ ...options, canonicalId: undefined }));
    expect(options.dispatch).toHaveBeenCalledWith({ type: "open", id: "target-title.md" });
  });
});
