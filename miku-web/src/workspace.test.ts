import { describe, expect, it } from "vitest";
import { initialWorkspaceState, workspaceReducer } from "./workspace";

describe("workspace reducer", () => {
  it("opens notes once and makes them active", () => {
    const opened = workspaceReducer(initialWorkspaceState, { type: "open", id: "workspace" });
    const reopened = workspaceReducer(opened, { type: "open", id: "workspace" });
    expect(opened.activeId).toBe("workspace");
    expect(reopened.tabs.filter((id) => id === "workspace")).toHaveLength(1);
  });

  it("keeps a recoverable tab when the last tab closes", () => {
    const onlyTab = { ...initialWorkspaceState, tabs: ["roadmap"], activeId: "roadmap" };
    const next = workspaceReducer(onlyTab, { type: "close", id: "roadmap" });
    expect(next.tabs).toEqual(["welcome"]);
    expect(next.activeId).toBe("welcome");
  });

  it("toggles split, context, and hoist independently", () => {
    const split = workspaceReducer(initialWorkspaceState, { type: "toggle-split" });
    const context = workspaceReducer(split, { type: "toggle-context" });
    const hoisted = workspaceReducer(context, { type: "toggle-hoist" });
    expect(hoisted).toMatchObject({ split: true, contextOpen: false, hoisted: true });
  });
});
