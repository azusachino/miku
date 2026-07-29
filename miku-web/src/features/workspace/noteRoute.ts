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
  /**
   * The backend-resolved canonical path for the active note, when known
   * (i.e. context data has loaded and isn't a stale placeholder). When this
   * differs from `activeId`, a redirect to the canonical URL is imminent
   * (see the canonicalization effect in WorkspaceApp), so opening a tab
   * under the pre-redirect id here would leave a stale duplicate once the
   * redirect lands and re-opens under the canonical id instead.
   */
  canonicalId?: string;
  tabs: string[];
  dispatch: Dispatch<WorkspaceAction>;
  navigate: NavigateFunction;
  setNotice: (notice: string) => void;
};

export function useNoteRouteRecovery({ activeId, isNoteRoute, isError, hasNote, canonicalId, tabs, dispatch, navigate, setNotice }: NoteRouteRecoveryOptions): void {
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
    const awaitingCanonicalRedirect = canonicalId !== undefined && canonicalId !== normActiveId;
    if (hasNote && !awaitingCanonicalRedirect && !tabs.map(normalizeNotePath).includes(normActiveId)) {
      dispatch({ type: "open", id: normActiveId });
    }
  }, [activeId, canonicalId, dispatch, hasNote, isError, isNoteRoute, navigate, setNotice, tabs]);
}
