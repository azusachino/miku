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
    const normActiveId = normalizeNotePath(activeId);
    if (isError) {
      if (handledInvalidRoute.current === normActiveId) return;
      handledInvalidRoute.current = normActiveId;
      setNotice(`Note not found: ${normActiveId}`);

      const remaining = tabs.map(normalizeNotePath).filter((t) => t !== normActiveId);
      dispatch({ type: "close", id: normActiveId });

      if (remaining.length > 0) {
        const next = remaining[remaining.length - 1];
        navigate(`/p/${next.split("/").map(encodeURIComponent).join("/")}`);
      } else {
        navigate("/");
      }
      return;
    }
    handledInvalidRoute.current = null;
    if (hasNote && !tabs.map(normalizeNotePath).includes(normActiveId)) {
      dispatch({ type: "open", id: normActiveId });
    }
  }, [activeId, dispatch, hasNote, isError, isNoteRoute, navigate, setNotice, tabs]);
}
