import { useEffect, useRef, type Dispatch } from "react";
import type { NavigateFunction } from "react-router-dom";
import type { WorkspaceAction } from "./state";

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
      const fallback = tabs.find((tab) => tab !== activeId);
      setNotice(`Note not found: ${activeId}`);
      dispatch({ type: "close", id: activeId });
      navigate(fallback ? `/p/${fallback}` : "/p/Index.md");
      return;
    }
    handledInvalidRoute.current = null;
    if (hasNote) dispatch({ type: "open", id: activeId });
  }, [activeId, dispatch, hasNote, isError, isNoteRoute, navigate, setNotice, tabs]);
}
