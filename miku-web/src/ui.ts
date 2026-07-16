export const UI_STATE_VERSION = 2 as const;
export const UI_STATE_KEY = "miku:ui:v2" as const;

export const shellRegions = ["launch", "explorer", "content", "context", "status"] as const;
export type ShellRegion = (typeof shellRegions)[number];

export const workspaceRoutes = {
  root: "/",
  note: "/p/*",
  tags: "/tags/*",
  recent: "/recent",
  settings: "/settings"
} as const;

export const focusTargets = ["tree", "note", "context"] as const;
export type FocusTarget = (typeof focusTargets)[number];

export const keyboardShortcuts = {
  quickOpen: "Mod+K",
  commandPalette: "Mod+P",
  focusTree: "Alt+Shift+1",
  focusNote: "Alt+Shift+2",
  toggleContext: "Alt+Shift+3"
} as const;

export type Theme = "dark" | "light";

export function readTheme(storage: Storage = localStorage): Theme {
  const current = storage.getItem(UI_STATE_KEY);
  if (current === "light" || current === "dark") return current;
  return storage.getItem("miku-theme") === "light" ? "light" : "dark";
}

export function writeTheme(theme: Theme, storage: Storage = localStorage): void {
  storage.setItem(UI_STATE_KEY, theme);
  storage.removeItem("miku-theme");
}
