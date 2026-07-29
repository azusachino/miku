import { lazy, Suspense, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import { createWorkspaceClient, subscribeToWorkspaceEvents, type ApiSource, type NoteModel, type SearchScope } from "./api";
import { ContextPanel, FolderBrowser, LaunchBar, NotePane, SettingsDialog, Sidebar, Tabs, WorkspaceUtility } from "./WorkspaceComponents";
import { NoteIcon } from "../../components/workspace/icons";
import { WorkspaceNotice } from "../../components/workspace/WorkspaceNotice";
import { normalizeNotePath, useNoteRouteRecovery } from "./noteRoute";
import { UI_STATE_VERSION, moveSearchSelection, readTheme, shellRegions, writeTheme, type Theme } from "../../shared/ui";
import { initialWorkspaceState, workspaceReducer } from "./state";

const INDEX_NOTE_PATH = "Index.md";
export function WorkspaceScreen() {
  const [state, dispatch] = useReducer(workspaceReducer, initialWorkspaceState);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchScope, setSearchScope] = useState<SearchScope>("all");
  const [searchSelection, setSearchSelection] = useState(-1);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(() => Number(localStorage.getItem("miku-sidebar-width") ?? 244));
  const [contextWidth, setContextWidth] = useState(() => Number(localStorage.getItem("miku-context-width") ?? 235));
  const [notice, setNotice] = useState<string | null>(null);
  const [noteCache, setNoteCache] = useState<Record<string, NoteModel>>({});
  const [apiSource, setApiSource] = useState<ApiSource>("connecting");
  const [theme, setTheme] = useState<Theme>(readTheme);
  const searchPanelRef = useRef<HTMLDivElement>(null);
  const resizingSidebar = useRef(false);
  const resizingContext = useRef(false);
  const navigate = useNavigate();
  const location = useLocation();
  const routeId = useParams()["*"];
  const queryClient = useQueryClient();
  const client = useMemo(() => createWorkspaceClient(setApiSource), []);
  const isNoteRoute = location.pathname.startsWith("/p/");
  const isFolderRoute = location.pathname.startsWith("/folder/");
  const folderPath = isFolderRoute
    ? location.pathname
        .slice("/folder/".length)
        .split("/")
        .map((part) => decodeURIComponent(part))
        .join("/")
    : "";
  const utilityRoute = location.pathname.startsWith("/tags") ? "tags" : location.pathname === "/recent" ? "recent" : undefined;
  const activeId = isNoteRoute ? normalizeNotePath(routeId ?? state.activeId) : "";
  const workspace = useQuery({ queryKey: ["workspace"], queryFn: client.workspace });
  const pagesQuery = useQuery({ queryKey: ["pages"], queryFn: client.pages });
  const tree = useQuery({ queryKey: ["tree"], queryFn: () => client.tree() });
  const folder = useQuery({ queryKey: ["folder", folderPath], queryFn: () => client.tree(folderPath), enabled: Boolean(folderPath) });
  const context = useQuery({
    queryKey: ["context", activeId],
    queryFn: () => client.context(activeId),
    enabled: Boolean(activeId),
    placeholderData: (previousData) => {
      return queryClient.getQueryData(["context", activeId]) ?? previousData;
    }
  });
  const [debouncedQuery, setDebouncedQuery] = useState(query);
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), 150);
    return () => clearTimeout(timer);
  }, [query]);
  const searchKey = debouncedQuery.trim();
  const results = useQuery({
    queryKey: ["search", searchKey, searchScope],
    queryFn: () => client.search(searchKey, searchScope),
    enabled: searchOpen && searchKey.length > 0,
    staleTime: 30_000
  });
  const isWorkspaceRoot = location.pathname === "/";
  const visibleTree = useMemo(() => [...(tree.data ?? []), ...(context.data?.children ?? [])], [context.data?.children, tree.data]);
  const treeNotes = useMemo(() => visibleTree.map((node) => ({ ...node.note, icon: "file-text", updated: "indexed", body: "", backlinks: [], tags: [] })), [visibleTree]);
  const pagesNotes = useMemo(
    () =>
      (pagesQuery.data ?? []).map((summary) => ({
        id: summary.path,
        path: summary.path,
        title: summary.title,
        icon: "file-text",
        parents: [],
        aliases: summary.aliases ?? [],
        updated: "indexed",
        body: "",
        backlinks: [],
        tags: [],
        identityGenerated: summary.identity_generated
      })),
    [pagesQuery.data]
  );
  const contextualNote = useMemo(() => context.data?.note, [context.data]);
  useEffect(() => {
    if (contextualNote) setNoteCache((current) => ({ ...current, [contextualNote.id]: contextualNote }));
  }, [contextualNote]);
  useEffect(() => {
    if (!contextualNote || !activeId || !isNoteRoute || context.isPlaceholderData) return;
    const normActive = normalizeNotePath(activeId);
    const normCanonical = normalizeNotePath(contextualNote.path);
    if (normActive !== normCanonical) {
      dispatch({ type: "replace-tab", oldId: normActive, newId: normCanonical });
      navigate(`/p/${normCanonical.split("/").map(encodeURIComponent).join("/")}`, { replace: true });
    }
  }, [activeId, contextualNote, context.isPlaceholderData, isNoteRoute, navigate]);
  const notes = useMemo(() => {
    const combined = [...pagesNotes, ...treeNotes, ...Object.values(noteCache)];
    const map = new Map<string, NoteModel>();
    for (const candidate of combined) {
      if (candidate.id) map.set(candidate.id, candidate);
      if (candidate.path) map.set(candidate.path, candidate);
      if (candidate.path) map.set(normalizeNotePath(candidate.path), candidate);
    }
    return Array.from(map.values());
  }, [noteCache, pagesNotes, treeNotes]);

  const normActiveId = normalizeNotePath(activeId);
  const activeNote = contextualNote ??
    notes.find((candidate) => candidate.id === activeId || candidate.path === activeId || normalizeNotePath(candidate.path) === normActiveId) ?? {
      id: activeId,
      path: activeId,
      title: context.isPending || context.isFetching ? "Loading note…" : activeId.split("/").pop()?.replace(/\.md$/, "") || "Note unavailable",
      icon: "file-text",
      parents: [],
      aliases: [],
      updated: "",
      body: "",
      backlinks: [],
      tags: [],
      identityGenerated: false
    };
  useNoteRouteRecovery({
    activeId,
    isNoteRoute,
    isError: context.isError,
    hasNote: Boolean(context.data?.note),
    tabs: state.tabs,
    dispatch,
    navigate,
    setNotice
  });

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 5000);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    if (!isWorkspaceRoot || !tree.data) return;
    const firstNote = tree.data.find((node) => node.kind === "markdown" && node.path === INDEX_NOTE_PATH) ?? tree.data.find((node) => node.kind === "markdown");
    if (firstNote) {
      dispatch({ type: "open", id: firstNote.path });
      navigate(`/p/${firstNote.path}`, { replace: true });
    }
  }, [isWorkspaceRoot, navigate, tree.data]);
  useEffect(() => {
    if (!isNoteRoute || !routeId || routeId.endsWith(".md")) return;
    navigate(`/p/${normalizeNotePath(routeId)}`, { replace: true });
  }, [isNoteRoute, navigate, routeId]);
  useEffect(
    () =>
      subscribeToWorkspaceEvents(() => {
        client.invalidateTree();
        void queryClient.invalidateQueries({ queryKey: ["workspace"] });
        void queryClient.invalidateQueries({ queryKey: ["tree"] });
        void queryClient.invalidateQueries({ queryKey: ["context"] });
        void queryClient.invalidateQueries({ queryKey: ["search"] });
      }),
    [client, queryClient]
  );
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearchSelection(-1);
        setSearchOpen(true);
      } else if (event.key === "Escape" && searchOpen) {
        setSearchOpen(false);
      } else if (event.key === "Escape" && settingsOpen) {
        setSettingsOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [searchOpen, settingsOpen]);

  useEffect(() => {
    const onPointerMove = (event: PointerEvent) => {
      if (resizingSidebar.current) {
        setSidebarWidth(Math.min(480, Math.max(200, event.clientX)));
      } else if (resizingContext.current) {
        setContextWidth(Math.min(420, Math.max(190, window.innerWidth - event.clientX)));
      }
    };
    const onPointerUp = () => {
      if (!resizingSidebar.current && !resizingContext.current) return;
      resizingSidebar.current = false;
      resizingContext.current = false;
      document.body.style.cursor = "";
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, []);
  useEffect(() => {
    localStorage.setItem("miku-sidebar-width", String(sidebarWidth));
  }, [sidebarWidth]);
  useEffect(() => {
    localStorage.setItem("miku-context-width", String(contextWidth));
  }, [contextWidth]);

  useEffect(() => {
    if (!searchOpen) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!searchPanelRef.current?.contains(event.target as Node)) setSearchOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [searchOpen]);

  useEffect(() => {
    setSearchSelection((current) => (results.data?.length ? Math.min(current, results.data.length - 1) : -1));
  }, [results.data]);

  const select = (id: string) => {
    const targetId = normalizeNotePath(id);
    void queryClient.prefetchQuery({ queryKey: ["context", targetId], queryFn: () => client.context(targetId) });
    dispatch({ type: "open", id: targetId });
    navigate(`/p/${targetId.split("/").map(encodeURIComponent).join("/")}`);
    setSearchOpen(false);
    const recent = JSON.parse(localStorage.getItem("miku-recent") ?? "[]") as string[];
    localStorage.setItem("miku-recent", JSON.stringify([targetId, ...recent.filter((path) => path !== targetId)].slice(0, 20)));
  };
  const closeTab = (id: string) => {
    const remaining = state.tabs.filter((tab) => tab !== id);
    dispatch({ type: "close", id });
    if (!remaining.length) {
      navigate("/");
    } else if (state.activeId === id) {
      navigate(`/p/${remaining.at(-1)!.split("/").map(encodeURIComponent).join("/")}`);
    }
  };
  const openBreadcrumbPath = (path: string) => {
    if (!path) {
      navigate("/");
      return;
    }
    navigate(`/folder/${path.split("/").map(encodeURIComponent).join("/")}`);
  };
  const toggleWorkspaceTree = () => {
    const expanding = state.hoisted;
    dispatch({ type: "toggle-hoist" });
    if (expanding) {
      requestAnimationFrame(() => {
        document.querySelector<HTMLElement>('.tab[aria-selected="true"] > button:first-child')?.focus();
      });
    }
  };
  const searchTag = (tag: string) => {
    navigate(`/tags/${encodeURIComponent(tag)}`);
  };
  const navigateContextPath = (path: string) => {
    select(path);
  };
  const openSearch = () => {
    setSearchSelection(-1);
    setSearchOpen(true);
  };
  const updateSearchQuery = (value: string) => {
    setQuery(value);
    setSearchSelection(-1);
  };
  const toggleTheme = () =>
    setTheme((current) => {
      const next = current === "dark" ? "light" : "dark";
      writeTheme(next);
      return next;
    });
  const handleSaveNote = (updated: NoteModel) => {
    queryClient.setQueryData(["context", activeId], (old: unknown) =>
      old && typeof old === "object" ? { ...old, note: updated } : old
    );
    if (updated.path) {
      queryClient.setQueryData(["context", updated.path], (old: unknown) =>
        old && typeof old === "object" ? { ...old, note: updated } : old
      );
    }
    setNoteCache((current) => ({
      ...current,
      [updated.id]: updated,
      ...(updated.path ? { [updated.path]: updated } : {})
    }));
  };
  const status = useMemo(() => (workspace.data ? `${workspace.data.noteCount} notes` : "Loading workspace"), [workspace.data]);

  const secondaryNote = notes.find((candidate) => candidate.id === (state.tabs.find((tab) => tab !== activeId) ?? "welcome")) ?? activeNote;
  return (
    <div className="app-shell flex h-screen min-h-0 flex-col bg-miku-bg text-miku-text" data-theme={theme} data-ui-state-version={UI_STATE_VERSION}>
      <LaunchBar onSearch={openSearch} theme={theme} onToggleTheme={toggleTheme} />
      {searchOpen && (
        <div className="search-popover" ref={searchPanelRef} data-region="quick-open">
          <div className="search-popover-head">
            <span>Quick search</span>
            <button onClick={() => setSearchOpen(false)}>Esc</button>
          </div>
          <input
            className="search-popover-input"
            autoFocus
            value={query}
            onChange={(event) => updateSearchQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                setSearchOpen(false);
              } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const key = event.key as "ArrowDown" | "ArrowUp";
                setSearchSelection((current) => moveSearchSelection(current, results.data?.length ?? 0, key));
              } else if (event.key === "Enter" && searchSelection >= 0 && results.data?.[searchSelection]) {
                event.preventDefault();
                select(results.data[searchSelection].id);
              }
            }}
            placeholder="Search notes…"
            aria-label="Quick search input"
            role="combobox"
            aria-controls="quick-open-results"
            aria-activedescendant={searchSelection >= 0 ? `search-result-${searchSelection}` : undefined}
            aria-expanded="true"
          />
          <div className="search-scopes" role="group" aria-label="Search scope">
            {(
              [
                ["all", "All"],
                ["title", "Title"],
                ["content", "Content"]
              ] as const
            ).map(([value, label]) => (
              <button key={value} className={`search-scope ${searchScope === value ? "is-active" : ""}`} aria-pressed={searchScope === value} onClick={() => setSearchScope(value)}>
                {label}
              </button>
            ))}
          </div>
          {results.isLoading ? (
            <div className="search-empty">Searching…</div>
          ) : results.data?.length ? (
            <div id="quick-open-results" role="listbox" aria-label="Search results">
              {results.data.map((result, index) => (
                <button
                  className={`search-result ${searchSelection === index ? "is-selected" : ""}`}
                  id={`search-result-${index}`}
                  key={result.id}
                  role="option"
                  aria-selected={searchSelection === index}
                  onMouseEnter={() => setSearchSelection(index)}
                  onClick={() => select(result.id)}
                >
                  <span className="search-result-icon">
                    <NoteIcon value={result.icon} />
                  </span>
                  <span>
                    <strong>{result.title}</strong>
                    <small>{result.path}</small>
                    {result.snippet && <small className="search-result-snippet">{result.snippet}</small>}
                  </span>
                  <kbd>{searchSelection === index ? "↵" : index + 1}</kbd>
                </button>
              ))}
            </div>
          ) : (
            <div className="search-empty">No matching notes</div>
          )}
        </div>
      )}
      <WorkspaceNotice message={notice} onDismiss={() => setNotice(null)} />
      <div
        className="workspace-layout flex h-[calc(100vh-var(--shell-topbar-height))] min-h-0 overflow-hidden"
        style={{ "--shell-sidebar-width": `${sidebarWidth}px`, "--shell-context-width": `${contextWidth}px` } as React.CSSProperties}
      >
        <Sidebar
          notes={notes}
          nodes={visibleTree}
          activeId={activeId}
          onSelect={select}
          hoisted={state.hoisted}
          onToggleHoist={toggleWorkspaceTree}
          client={client}
          onTags={() => navigate("/tags")}
          onRecent={() => navigate("/recent")}
          onSettings={() => setSettingsOpen(true)}
          noteCount={workspace.data?.noteCount ?? 0}
          onResizeStart={(event) => {
            event.preventDefault();
            resizingSidebar.current = true;
            document.body.style.cursor = "col-resize";
          }}
        />
        <main className="main-stage flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {folderPath ? (
            <FolderBrowser
              path={folderPath}
              nodes={folder.data ?? []}
              isLoading={folder.isLoading}
              isError={folder.isError}
              onSelect={select}
              onOpenFolder={(path) => navigate(`/folder/${path.split("/").map(encodeURIComponent).join("/")}`)}
              onNavigatePath={openBreadcrumbPath}
            />
          ) : utilityRoute ? (
            <WorkspaceUtility route={utilityRoute} theme={theme} onToggleTheme={toggleTheme} client={client} />
          ) : (
            <>
              <Tabs notes={notes} tabs={state.tabs} activeId={activeId} activeNote={activeNote} onSelect={select} onClose={closeTab} />
              <div className="content-stage">
                <NotePane
                  note={activeNote}
                  split={state.split}
                  onSplit={() => dispatch({ type: "toggle-split" })}
                  readonly={workspace.data?.readonly ?? true}
                  indexPhase={workspace.data?.indexPhase}
                  client={client}
                  onTagSearch={searchTag}
                  onNavigatePath={openBreadcrumbPath}
                  onSaveNote={handleSaveNote}
                  theme={theme}
                  notes={notes}
                />
                {state.split && (
                  <NotePane
                    note={secondaryNote}
                    split={false}
                    onSplit={() => dispatch({ type: "toggle-split" })}
                    readonly={workspace.data?.readonly ?? true}
                    indexPhase={workspace.data?.indexPhase}
                    client={client}
                    onTagSearch={searchTag}
                    onNavigatePath={openBreadcrumbPath}
                    onSaveNote={handleSaveNote}
                    theme={theme}
                    notes={notes}
                  />
                )}
                <ContextPanel
                  note={activeNote}
                  backlinks={context.data?.backlinks ?? []}
                  indexPhase={workspace.data?.indexPhase}
                  open={state.contextOpen}
                  onToggle={() => dispatch({ type: "toggle-context" })}
                  onNavigate={navigateContextPath}
                  onResizeStart={(event) => {
                    event.preventDefault();
                    resizingContext.current = true;
                    document.body.style.cursor = "col-resize";
                  }}
                  notes={notes}
                />
              </div>
            </>
          )}
          <footer className="status-bar" data-region={shellRegions[4]}>
            <span>
              <span className="online-dot" /> {apiSource === "live" ? "live vault" : apiSource === "offline" ? "offline" : "connecting"}
            </span>
            <span>{status}</span>
            <span>{workspace.data?.readonly ? "readonly API" : "workspace"}</span>
            <span className="status-spacer" />
            <span>focus: {state.focus}</span>
          </footer>
        </main>
      </div>
      {settingsOpen && <SettingsDialog theme={theme} onToggleTheme={toggleTheme} onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
