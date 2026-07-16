import { describe, expect, it } from "vitest";
import { EXPLORER_STATE_KEY, UI_STATE_KEY, moveSearchSelection, readExpandedPaths, readTheme, shellRegions, writeExpandedPaths, writeTheme } from "./ui";

describe("shared UI contract", () => {
  it("keeps shell regions explicit", () => {
    expect(shellRegions).toEqual(["launch", "explorer", "content", "context", "status"]);
  });

  it("uses one versioned storage key for theme state", () => {
    const storage = new Map<string, string>();
    const adapter = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      }
    } as unknown as Storage;

    expect(readTheme(adapter)).toBe("dark");
    writeTheme("light", adapter);
    expect(adapter.getItem(UI_STATE_KEY)).toBe("light");
    expect(readTheme(adapter)).toBe("light");
  });

  it("reads the previous theme key during the state migration", () => {
    const storage = new Map([["miku-theme", "light"]]);
    const adapter = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      }
    } as unknown as Storage;

    expect(readTheme(adapter)).toBe("light");
    writeTheme("dark", adapter);
    expect(adapter.getItem("miku-theme")).toBeNull();
  });

  it("round-trips bounded explorer disclosure state", () => {
    const storage = new Map<string, string>();
    const adapter = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      }
    } as unknown as Storage;

    writeExpandedPaths(["projects", "projects/miku", "projects", "projects/miku"], adapter);
    expect(adapter.getItem(EXPLORER_STATE_KEY)).toBe('["projects","projects/miku"]');
    expect(readExpandedPaths(adapter)).toEqual(["projects", "projects/miku"]);
  });

  it("wraps quick-open selection without inventing a result", () => {
    expect(moveSearchSelection(-1, 3, "ArrowDown")).toBe(0);
    expect(moveSearchSelection(0, 3, "ArrowUp")).toBe(2);
    expect(moveSearchSelection(0, 0, "ArrowDown")).toBe(-1);
  });
});
