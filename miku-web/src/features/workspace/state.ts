export type WorkspaceState = {
  tabs: string[];
  activeId: string;
  split: boolean;
  contextOpen: boolean;
  hoisted: boolean;
  focus: "tree" | "note" | "context";
};

export type WorkspaceAction =
  | { type: "open"; id: string }
  | { type: "close"; id: string }
  | { type: "replace-tab"; oldId: string; newId: string }
  | { type: "toggle-split" }
  | { type: "toggle-context" }
  | { type: "toggle-hoist" }
  | { type: "focus"; target: WorkspaceState["focus"] };

export const initialWorkspaceState: WorkspaceState = {
  tabs: [],
  activeId: "",
  split: false,
  contextOpen: true,
  hoisted: false,
  focus: "note"
};

function normalize(id: string): string {
  if (!id) return "";
  let clean = id.trim();
  try {
    clean = decodeURIComponent(clean);
  } catch {
    // keep raw if decode fails
  }
  clean = clean.replace(/^\/+/, "");
  if (clean.startsWith("p/")) {
    clean = clean.slice(2);
  }
  return clean.endsWith(".md") ? clean : `${clean}.md`;
}


export function workspaceReducer(state: WorkspaceState, action: WorkspaceAction): WorkspaceState {
  switch (action.type) {
    case "open": {
      const normId = normalize(action.id);
      if (!normId) return state;
      const normTabs = state.tabs.map(normalize);
      return {
        ...state,
        tabs: normTabs.includes(normId) ? normTabs : [...normTabs, normId],
        activeId: normId,
        focus: "note"
      };
    }
    case "close": {
      const normId = normalize(action.id);
      const normActive = normalize(state.activeId);
      const tabs = state.tabs.map(normalize).filter((tab) => tab !== normId);
      if (!tabs.length) return { ...state, tabs: [], activeId: "" };
      return {
        ...state,
        tabs,
        activeId: normActive === normId ? tabs.at(-1)! : normActive
      };
    }
    case "replace-tab": {
      const oldNorm = normalize(action.oldId);
      const newNorm = normalize(action.newId);
      if (!newNorm) return state;
      const updatedTabs: string[] = [];
      for (const tab of state.tabs.map(normalize)) {
        const target = tab === oldNorm ? newNorm : tab;
        if (!updatedTabs.includes(target)) {
          updatedTabs.push(target);
        }
      }
      return {
        ...state,
        tabs: updatedTabs,
        activeId: normalize(state.activeId) === oldNorm ? newNorm : normalize(state.activeId)
      };
    }
    case "toggle-split":
      return { ...state, split: !state.split };
    case "toggle-context":
      return { ...state, contextOpen: !state.contextOpen };
    case "toggle-hoist":
      return { ...state, hoisted: !state.hoisted };
    case "focus":
      return { ...state, focus: action.target };
  }
}
