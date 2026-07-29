import { useEffect, useRef, type Dispatch } from "react";
import type { NavigateFunction } from "react-router-dom";
import type { WorkspaceAction } from "./state";

export function normalizeNotePath(path: string): string {
  return path.endsWith(".md") ? path : `${path}.md`;
}

type CloseTabOptions = {
  id: string;
  tabs: string[];
  /**
   * The note actually being viewed right now, derived from the URL (e.g.
   * `activeId` in WorkspaceApp) -- NOT the reducer's internal
   * `state.activeId`. That field only updates when something dispatches
   * "open"/"replace-tab"; navigating to an already-open note via a direct
   * `navigate()` call (e.g. clicking a wikilink inside MarkdownReader,
   * which never dispatches) changes the URL and the rendered note without
   * updating it. Using the stale reducer field here means closing the tab
   * you're actually looking at silently fails to navigate away, even
   * though the tab is correctly removed from the list.
   */
  activeId: string;
  dispatch: Dispatch<WorkspaceAction>;
  navigate: NavigateFunction;
};

export function closeTab({ id, tabs, activeId, dispatch, navigate }: CloseTabOptions): void {
  const normId = normalizeNotePath(id);
  const remaining = tabs.map(normalizeNotePath).filter((tab) => tab !== normId);
  dispatch({ type: "close", id: normId });
  if (!remaining.length) {
    navigate("/");
  } else if (normalizeNotePath(activeId) === normId) {
    navigate(`/p/${remaining.at(-1)!.split("/").map(encodeURIComponent).join("/")}`);
  }
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
