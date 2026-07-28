import { useEffect, useRef, type Dispatch } from "react";
import type { NavigateFunction } from "react-router-dom";
import type { WorkspaceAction } from "./state";

export function normalizeNotePath(path: string): string {
  return path.endsWith(".md") ? path : `${path}.md`;
}

type NoteRouteRecoveryOptions = {
  activeId: string;
  isNoteRoute: boolean;
  isError: boolean;
  hasNote: boolean;
  tabs: string[];
  dispatch: Dispatch<WorkspaceAction>;
  navigate: NavigateFunction;
  setNotice: (notice: string) => void;
};

export function useNoteRouteRecovery({ activeId, isNoteRoute, isError, hasNote, tabs, dispatch, navigate, setNotice }: NoteRouteRecoveryOptions): void {
  const handledInvalidRoute = useRef<string | null>(null);

  useEffect(() => {
    if (!isNoteRoute || !activeId) {
      handledInvalidRoute.current = null;
      return;
    }
    if (isError) {
      if (handledInvalidRoute.current === activeId) return;
      handledInvalidRoute.current = activeId;
      setNotice(`Note not found: ${activeId}`);

      const remaining = tabs.filter((t) => t !== activeId);
      dispatch({ type: "close", id: activeId });

      if (remaining.length > 0) {
        const next = remaining[remaining.length - 1];
        navigate(`/p/${next.split("/").map(encodeURIComponent).join("/")}`);
      } else {
        navigate("/");
      }
      return;
    }
    handledInvalidRoute.current = null;
    if (hasNote) dispatch({ type: "open", id: activeId });
  }, [activeId, dispatch, hasNote, isError, isNoteRoute, navigate, setNotice, tabs]);
}
