// @vitest-environment jsdom

import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TreeNodeModel } from "../../features/workspace/api";
import { EXPLORER_STATE_KEY } from "../../shared/ui";
import { WorkspaceTree } from "./WorkspaceTree";

function folder(path: string): TreeNodeModel {
  return {
    kind: "folder",
    path,
    hasChildren: true,
    placementId: `path:${path}`,
    noteId: path,
    parentId: null,
    note: {
      id: path,
      path,
      title: path.split("/").at(-1) ?? path,
      identityGenerated: false,
      parents: [],
      aliases: []
    }
  };
}

function markdown(path: string): TreeNodeModel {
  return {
    ...folder(path),
    kind: "markdown",
    hasChildren: false
  };
}

describe("WorkspaceTree", () => {
  it("reopens a globally collapsed tree when a folder is clicked once", async () => {
    const tree = vi.fn().mockResolvedValue([folder("dedao-docs/docs"), markdown("dedao-docs/README.md")]);
    const rootNodes = [folder("dedao-docs")];
    localStorage.setItem(EXPLORER_STATE_KEY, JSON.stringify(["dedao-docs"]));

    function CollapsedTree() {
      const [hoisted, setHoisted] = useState(true);
      return <WorkspaceTree notes={[]} nodes={rootNodes} activeId="" onSelect={() => undefined} hoisted={hoisted} onExpandTree={() => setHoisted(false)} client={{ tree } as never} />;
    }

    render(<CollapsedTree />);
    fireEvent.click(screen.getByRole("button", { name: "dedao-docs" }));

    await waitFor(() => expect(screen.getByRole("button", { name: "docs" })).toBeTruthy());
    expect(screen.getByRole("button", { name: "README.md" })).toBeTruthy();
    expect(tree).toHaveBeenCalledWith("dedao-docs");
    localStorage.clear();
  });
});
