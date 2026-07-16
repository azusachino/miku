import { describe, expect, it } from "vitest";
import { UI_STATE_KEY, keyboardShortcuts, readTheme, shellRegions, writeTheme, workspaceRoutes } from "./ui";

describe("shared UI contract", () => {
  it("keeps shell regions and route ownership explicit", () => {
    expect(shellRegions).toEqual(["launch", "explorer", "content", "context", "status"]);
    expect(workspaceRoutes.note).toBe("/p/*");
    expect(workspaceRoutes.settings).toBe("/settings");
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

  it("keeps keyboard shortcuts discoverable and platform-neutral", () => {
    expect(keyboardShortcuts.quickOpen).toBe("Mod+K");
    expect(keyboardShortcuts.commandPalette).toBe("Mod+P");
  });
});
