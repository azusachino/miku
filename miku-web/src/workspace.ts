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
  | { type: "toggle-split" }
  | { type: "toggle-context" }
  | { type: "toggle-hoist" }
  | { type: "focus"; target: WorkspaceState["focus"] };

export const initialWorkspaceState: WorkspaceState = {
  tabs: ["welcome", "roadmap"],
  activeId: "roadmap",
  split: false,
  contextOpen: true,
  hoisted: false,
  focus: "note",
};

export function workspaceReducer(state: WorkspaceState, action: WorkspaceAction): WorkspaceState {
  switch (action.type) {
    case "open":
      return {
        ...state,
        tabs: state.tabs.includes(action.id) ? state.tabs : [...state.tabs, action.id],
        activeId: action.id,
        focus: "note",
      };
    case "close": {
      const tabs = state.tabs.filter((tab) => tab !== action.id);
      if (!tabs.length) return { ...state, tabs: ["welcome"], activeId: "welcome" };
      return { ...state, tabs, activeId: state.activeId === action.id ? tabs.at(-1)! : state.activeId };
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
