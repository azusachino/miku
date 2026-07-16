import { lazy, Suspense, useEffect, useMemo, useReducer, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Navigate, Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { createWorkspaceClient, sortTreeNodes, subscribeToWorkspaceEvents, type ApiSource, type NoteModel, type SearchScope, type TreeNodeModel } from "./api";
import { UI_STATE_VERSION, readExpandedPaths, readTheme, shellRegions, writeExpandedPaths, writeTheme, type Theme } from "./ui";
import { initialWorkspaceState, workspaceReducer } from "./workspace";
const MarkdownEditor = lazy(() => import("./MarkdownEditor"));
const MarkdownReader = lazy(() => import("./MarkdownReader").then((module) => ({ default: module.MarkdownReader })));

function Icon({ children }: { children: string }) {
  return (
    <span className="icon" aria-hidden="true">
      {children}
    </span>
  );
}

function FileIcon({ kind = "note", large = false }: { kind?: "note" | "folder"; large?: boolean }) {
  return (
    <span className={`file-icon file-icon-${kind} ${large ? "file-icon-large" : ""}`} aria-hidden="true">
      <svg viewBox="0 0 20 20" focusable="false">
        {kind === "folder" ? (
          <path d="M2.5 5.5h5l1.6 1.8h8.4v8.3H2.5z" />
        ) : (
          <>
            <path d="M5 2.5h6.1l3.9 3.9v11.1H5z" />
            <path d="M11 2.7v4h3.8" />
            <path d="M7.3 11.2h5.4M7.3 14h4.2" />
          </>
        )}
      </svg>
    </span>
  );
}

function Tree({
  notes,
  nodes,
  activeId,
  onSelect,
  hoisted,
  client
}: {
  notes: NoteModel[];
  nodes: TreeNodeModel[];
  activeId: string;
  onSelect: (id: string) => void;
  hoisted: boolean;
  client: ReturnType<typeof createWorkspaceClient>;
}) {
  const noteMap = new Map(notes.map((note) => [note.id, note]));
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(readExpandedPaths()));
  const [loaded, setLoaded] = useState<Record<string, TreeNodeModel[]>>({});
  const roots = sortTreeNodes(nodes.filter((node) => (hoisted ? node.path === activeId || activeId.startsWith(`${node.path}/`) : node.parentId === null)));

  useEffect(() => {
    if (!activeId) return;
    const ancestors = activeId
      .split("/")
      .slice(0, -1)
      .map((_, index, parts) => parts.slice(0, index + 1).join("/"));
    setExpanded((current) => {
      const next = new Set(current);
      ancestors.forEach((path) => next.add(path));
      return next;
    });
  }, [activeId]);

  useEffect(() => {
    writeExpandedPaths(expanded);
  }, [expanded]);

  useEffect(() => {
    let cancelled = false;
    const pending = roots.filter((node) => node.kind === "folder" && expanded.has(node.path) && !loaded[node.path]);
    if (!pending.length) return;
    void Promise.all(pending.map(async (node) => [node.path, await client.tree(node.path)] as const)).then((entries) => {
      if (cancelled) return;
      setLoaded((current) => ({ ...current, ...Object.fromEntries(entries) }));
    });
    return () => {
      cancelled = true;
    };
  }, [client, expanded, loaded, roots]);

  const branch = (node: TreeNodeModel, depth: number) => {
    const note = noteMap.get(node.noteId) ?? { ...node.note, icon: "□", updated: "unknown", body: "", backlinks: [], tags: [] };
    const children = sortTreeNodes(loaded[node.path] ?? []);
    const isFolder = node.kind === "folder";
    const isExpanded = expanded.has(node.path);
    const indexNote = children.find((child) => child.kind === "markdown" && child.path === `${node.path}/index.md`);
    const title = isFolder ? (indexNote?.note.title ?? node.note.title) : note.title;
    const toggleFolder = async () => {
      if (isExpanded) {
        setExpanded((current) => {
          const next = new Set(current);
          next.delete(node.path);
          return next;
        });
        return;
      }
      if (!loaded[node.path]) {
        const children = await client.tree(node.path);
        setLoaded((current) => ({ ...current, [node.path]: children }));
      }
      setExpanded((current) => new Set(current).add(node.path));
    };
    return (
      <div key={node.placementId} className="tree-branch">
        <button
          className={`tree-row ${activeId === note.id ? "is-active" : ""}`}
          style={{ paddingLeft: `${14 + depth * 17}px` }}
          onClick={() => {
            if (isFolder) void toggleFolder();
            else onSelect(node.path);
          }}
          aria-current={activeId === note.id ? "page" : undefined}
          aria-expanded={isFolder ? isExpanded : undefined}
        >
          <span className="tree-caret">{isFolder ? (isExpanded ? "⌄" : "›") : "·"}</span>
          <FileIcon kind={isFolder ? "folder" : "note"} />
          <span className="tree-label">{title}</span>
        </button>
        {!hoisted && isExpanded && children.map((child) => branch(child, depth + 1))}
      </div>
    );
  };

  return <div className="tree-list">{roots.map((note) => branch(note, 0))}</div>;
}

function LaunchBar({
  query,
  setQuery,
  onSearch,
  theme,
  onToggleTheme,
  indexPhase,
  noteCount
}: {
  query: string;
  setQuery: (value: string) => void;
  onSearch: () => void;
  theme: "dark" | "light";
  onToggleTheme: () => void;
  indexPhase?: string;
  noteCount?: number;
}) {
  const [vaultOpen, setVaultOpen] = useState(false);
  return (
    <header className="launch-bar" data-region={shellRegions[0]}>
      <div className="brand-mark">
        <span className="brand-glyph">m</span>
        <span>miku</span>
      </div>
      <div className="vault-control">
        <button className="vault-switcher" aria-label="Open vault menu" onClick={() => setVaultOpen((open) => !open)} aria-expanded={vaultOpen}>
          <span className="status-dot" /> personal vault <Icon>{vaultOpen ? "⌃" : "⌄"}</Icon>
        </button>
        {vaultOpen && (
          <div className="vault-menu" role="menu">
            <strong>personal vault</strong>
            <small>miku_docs · {noteCount ?? 0} notes</small>
            <span className={`vault-phase is-${(indexPhase ?? "indexing").toLowerCase()}`}>{indexPhase ?? "Indexing"}</span>
          </div>
        )}
      </div>
      <div className="launch-search">
        <Icon>⌕</Icon>
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && onSearch()}
          placeholder="Search notes, tags, commands"
          aria-label="Search notes"
        />
        <kbd>⌘ K</kbd>
      </div>
      <div className="launch-actions">
        <button className="quiet-button" aria-label="Toggle theme" onClick={onToggleTheme}>
          {theme === "dark" ? "☼" : "☾"}
        </button>
      </div>
    </header>
  );
}

function Sidebar({
  notes,
  nodes,
  activeId,
  onSelect,
  hoisted,
  onToggleHoist,
  client,
  onTags,
  onRecent,
  onSettings
}: {
  notes: NoteModel[];
  nodes: TreeNodeModel[];
  activeId: string;
  onSelect: (id: string) => void;
  hoisted: boolean;
  onToggleHoist: () => void;
  client: ReturnType<typeof createWorkspaceClient>;
  onTags: () => void;
  onRecent: () => void;
  onSettings: () => void;
}) {
  return (
    <aside className="sidebar" data-region={shellRegions[1]}>
      <div className="sidebar-toolbar">
        <span className="eyebrow">Workspace</span>
        <button className={`tool-button ${hoisted ? "is-on" : ""}`} onClick={onToggleHoist} aria-label="Toggle hoisted note">
          ⌃
        </button>
      </div>
      <div className="tree-heading">
        <span>All notes</span>
        <span className="count-pill">{notes.length}</span>
      </div>
      <Tree notes={notes} nodes={nodes} activeId={activeId} onSelect={onSelect} hoisted={hoisted} client={client} />
      <div className="sidebar-bottom">
        <button className="sidebar-link" onClick={onRecent}>
          <Icon>◷</Icon> Recent
        </button>
        <button className="sidebar-link" onClick={onTags}>
          <Icon>#</Icon> Tags
        </button>
        <button className="sidebar-link" onClick={onSettings}>
          <Icon>⚙</Icon> Settings
        </button>
      </div>
    </aside>
  );
}

function Tabs({
  notes,
  tabs,
  activeId,
  activeNote,
  onSelect,
  onClose
}: {
  notes: NoteModel[];
  tabs: string[];
  activeId: string;
  activeNote: NoteModel;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
}) {
  return (
    <div className="tabs" role="tablist">
      {tabs.map((id) => {
        const note = id === activeId ? activeNote : (notes.find((item) => item.id === id) ?? { id, title: "Loading note…", icon: "□" });
        return (
          <div key={id} className={`tab ${activeId === id ? "is-active" : ""}`} role="tab" aria-selected={activeId === id}>
            <button onClick={() => onSelect(id)}>
              <FileIcon />
              {note.title}
            </button>
            <button className="tab-close" onClick={() => onClose(id)} aria-label={`Close ${note.title}`}>
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}

function NotePane({
  note,
  split,
  onSplit,
  readonly,
  client,
  onTagSearch
}: {
  note: NoteModel;
  split: boolean;
  onSplit: () => void;
  readonly: boolean;
  client: ReturnType<typeof createWorkspaceClient>;
  onTagSearch: (tag: string) => void;
}) {
  const [draft, setDraft] = useState(note.body);
  const [saveState, setSaveState] = useState("saved");
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    setDraft(note.body);
    setSaveState("saved");
    setEditing(false);
  }, [note.id, note.body]);
  const save = async () => {
    if (readonly || !note.revision) return;
    setSaveState("saving…");
    try {
      await client.saveNote(note.id, { body: draft, title: note.title, expectedRevision: note.revision });
      setSaveState("saved");
      setEditing(false);
    } catch (error) {
      setSaveState(error instanceof Error && error.message.startsWith("409") ? "conflict" : "save failed");
    }
  };
  return (
    <section className={`note-pane ${split ? "is-split" : ""}`} data-region={shellRegions[2]}>
      <div className="note-toolbar">
        <div className="breadcrumbs">
          <span>{note.path.split("/")[0]}</span>
          <span>/</span>
          <strong>{note.title}</strong>
        </div>
        <div className="note-actions">
          <button className="toolbar-button" onClick={onSplit}>
            {split ? "Single pane" : "Split pane"}
          </button>
          {!readonly && (
            <button className="toolbar-button" onClick={() => setEditing((value) => !value)}>
              {editing ? "Read" : "Edit"}
            </button>
          )}
        </div>
      </div>
      <div className="note-scroll">
        <div className="note-kicker">
          <FileIcon large />
          <span className="saved-state">
            <span className="saved-dot" /> {editing ? saveState : "reading"}
          </span>
        </div>
        <h1>{note.title}</h1>
        <p className="note-subtitle">
          {note.path} <span>·</span> updated {note.updated}
        </p>
        <div className="tag-row">
          {note.tags.map((tag) => (
            <button className="tag" key={tag} onClick={() => onTagSearch(tag)}>
              #{tag}
            </button>
          ))}
        </div>
        {editing ? (
          <Suspense fallback={<div className="markdown-editor-loading">Loading editor…</div>}>
            <MarkdownEditor
              noteId={note.id}
              value={draft}
              readOnly={readonly}
              onChange={(value) => {
                setDraft(value);
                setSaveState("unsaved");
              }}
            />
          </Suspense>
        ) : (
          <Suspense fallback={<div className="markdown-editor-loading">Rendering Markdown…</div>}>
            <MarkdownReader value={note.body} />
          </Suspense>
        )}
        <div className="note-footer">
          <span>{editing ? "Markdown source" : "Reader mode"}</span>
          {editing && (
            <button className="toolbar-button" disabled={readonly || !note.revision || saveState === "saving…"} onClick={save}>
              Save
            </button>
          )}
          <span>{readonly ? "Readonly" : editing ? "Edit mode" : "Reader mode"}</span>
        </div>
      </div>
    </section>
  );
}

function ContextPanel({ note, open, onToggle, onNavigate }: { note: NoteModel; open: boolean; onToggle: () => void; onNavigate: (path: string) => void }) {
  if (!open)
    return (
      <button className="context-reopen" onClick={onToggle} aria-label="Open context panel">
        ‹
      </button>
    );
  return (
    <aside className="context-panel" data-region={shellRegions[3]}>
      <div className="context-header">
        <span className="eyebrow">Context</span>
        <button className="tool-button" onClick={onToggle} aria-label="Close context panel">
          ›
        </button>
      </div>
      <div className="context-section">
        <div className="context-title">
          Relations <span>{note.backlinks.length}</span>
        </div>
        {note.backlinks.map((backlink) => (
          <button className="relation-row" key={backlink} onClick={() => onNavigate(backlink)}>
            <span className="relation-line" />
            <span>{backlink}</span>
            <Icon>↗</Icon>
          </button>
        ))}
      </div>
      <div className="context-section">
        <div className="context-title">Properties</div>
        <div className="property-row">
          <span>type</span>
          <strong>text</strong>
        </div>
        <div className="property-row">
          <span>revision</span>
          <strong>clean</strong>
        </div>
        <div className="property-row">
          <span>placements</span>
          <strong>{note.parents.length || 1}</strong>
        </div>
      </div>
      <div className="context-section">
        <div className="context-title">Source</div>
        <div className="activity">
          <span className="activity-dot" />
          <div>
            <strong>Markdown source</strong>
            <small>{note.updated}</small>
          </div>
        </div>
      </div>
    </aside>
  );
}

function WorkspaceScreen() {
  const [state, dispatch] = useReducer(workspaceReducer, initialWorkspaceState);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchScope, setSearchScope] = useState<SearchScope>("all");
  const [noteCache, setNoteCache] = useState<Record<string, NoteModel>>({});
  const [apiSource, setApiSource] = useState<ApiSource>("connecting");
  const [theme, setTheme] = useState<Theme>(readTheme);
  const navigate = useNavigate();
  const location = useLocation();
  const routeId = useParams()["*"];
  const queryClient = useQueryClient();
  const client = useMemo(() => createWorkspaceClient(setApiSource), []);
  const activeId = routeId ?? state.activeId;
  const workspace = useQuery({ queryKey: ["workspace"], queryFn: client.workspace });
  const tree = useQuery({ queryKey: ["tree"], queryFn: () => client.tree() });
  const note = useQuery({ queryKey: ["note", activeId], queryFn: () => client.note(activeId), enabled: Boolean(activeId) });
  const context = useQuery({ queryKey: ["context", activeId], queryFn: () => client.context(activeId), enabled: Boolean(activeId) });
  const results = useQuery({ queryKey: ["search", query, searchScope], queryFn: () => client.search(query, searchScope), enabled: searchOpen });
  const isWorkspaceRoot = location.pathname === "/";
  const visibleTree = useMemo(() => [...(tree.data ?? []), ...(context.data?.children ?? [])], [context.data?.children, tree.data]);
  const treeNotes = useMemo(() => visibleTree.map((node) => ({ ...node.note, icon: "□", updated: "indexed", body: "", backlinks: [], tags: [] })), [visibleTree]);
  const contextualNote = useMemo(() => (context.data ? { ...context.data.note, backlinks: context.data.backlinks } : undefined), [context.data]);
  useEffect(() => {
    const loadedNote = contextualNote ?? note.data;
    if (loadedNote) setNoteCache((current) => ({ ...current, [loadedNote.id]: loadedNote }));
  }, [contextualNote, note.data]);
  const notes = useMemo(() => {
    const combined = [...treeNotes, ...Object.values(noteCache)];
    return Array.from(new Map(combined.map((candidate) => [candidate.id, candidate])).values());
  }, [noteCache, treeNotes]);
  const activeNote = contextualNote ??
    note.data ??
    notes.find((candidate) => candidate.id === activeId) ?? {
      id: activeId,
      path: "",
      title: note.isLoading ? "Loading note…" : "Note unavailable",
      icon: "□",
      parents: [],
      updated: "",
      body: "",
      backlinks: [],
      tags: [],
      identityGenerated: false
    };

  useEffect(() => {
    if (!isWorkspaceRoot || !tree.data) return;
    const firstNote = tree.data.find((node) => node.kind === "markdown");
    if (firstNote) {
      dispatch({ type: "open", id: firstNote.path });
      navigate(`/p/${firstNote.path}`, { replace: true });
    }
  }, [isWorkspaceRoot, navigate, tree.data]);
  useEffect(
    () =>
      subscribeToWorkspaceEvents(() => {
        void queryClient.invalidateQueries({ queryKey: ["workspace"] });
        void queryClient.invalidateQueries({ queryKey: ["tree"] });
        void queryClient.invalidateQueries({ queryKey: ["note"] });
        void queryClient.invalidateQueries({ queryKey: ["context"] });
        void queryClient.invalidateQueries({ queryKey: ["search"] });
      }),
    [queryClient]
  );
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const select = (id: string) => {
    dispatch({ type: "open", id });
    navigate(`/p/${id}`);
    setSearchOpen(false);
    const recent = JSON.parse(localStorage.getItem("miku-recent") ?? "[]") as string[];
    localStorage.setItem("miku-recent", JSON.stringify([id, ...recent.filter((path) => path !== id)].slice(0, 20)));
  };
  const searchTag = (tag: string) => {
    navigate(`/tags/${encodeURIComponent(tag)}`);
  };
  const openSearch = () => setSearchOpen(true);
  const toggleTheme = () =>
    setTheme((current) => {
      const next = current === "dark" ? "light" : "dark";
      writeTheme(next);
      return next;
    });
  const status = useMemo(() => (workspace.data ? `${workspace.data.noteCount} notes · ${workspace.data.placementCount} placements` : "Loading workspace"), [workspace.data]);

  const secondaryNote = notes.find((candidate) => candidate.id === (state.tabs.find((tab) => tab !== activeId) ?? "welcome")) ?? activeNote;
  return (
    <div className="app-shell" data-theme={theme} data-ui-state-version={UI_STATE_VERSION}>
      <LaunchBar query={query} setQuery={setQuery} onSearch={openSearch} theme={theme} onToggleTheme={toggleTheme} indexPhase={workspace.data?.indexPhase} noteCount={workspace.data?.noteCount} />
      {searchOpen && (
        <div className="search-popover" data-region="quick-open">
          <div className="search-popover-head">
            <span>Quick search</span>
            <button onClick={() => setSearchOpen(false)}>Esc</button>
          </div>
          <input
            className="search-popover-input"
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => event.key === "Escape" && setSearchOpen(false)}
            placeholder="Search notes…"
            aria-label="Quick search input"
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
            results.data.map((result) => (
              <button className="search-result" key={result.id} onClick={() => select(result.id)}>
                <span className="search-result-icon">{result.icon}</span>
                <span>
                  <strong>{result.title}</strong>
                  <small>{result.path}</small>
                </span>
                <kbd>↵</kbd>
              </button>
            ))
          ) : (
            <div className="search-empty">No matching notes</div>
          )}
        </div>
      )}
      <div className="workspace-layout">
        <Sidebar
          notes={notes}
          nodes={visibleTree}
          activeId={activeId}
          onSelect={select}
          hoisted={state.hoisted}
          onToggleHoist={() => dispatch({ type: "toggle-hoist" })}
          client={client}
          onTags={() => navigate("/tags")}
          onRecent={() => navigate("/recent")}
          onSettings={() => navigate("/settings")}
        />
        <main className="main-stage">
          <Tabs notes={notes} tabs={state.tabs} activeId={activeId} activeNote={activeNote} onSelect={select} onClose={(id) => dispatch({ type: "close", id })} />
          <div className="content-stage">
            <NotePane note={activeNote} split={state.split} onSplit={() => dispatch({ type: "toggle-split" })} readonly={workspace.data?.readonly ?? true} client={client} onTagSearch={searchTag} />
            {state.split && (
              <NotePane note={secondaryNote} split={false} onSplit={() => dispatch({ type: "toggle-split" })} readonly={workspace.data?.readonly ?? true} client={client} onTagSearch={searchTag} />
            )}
            <ContextPanel note={activeNote} open={state.contextOpen} onToggle={() => dispatch({ type: "toggle-context" })} onNavigate={select} />
          </div>
          <footer className="status-bar" data-region={shellRegions[4]}>
            <span>
              <span className="online-dot" /> {apiSource === "live" ? "live vault" : apiSource === "offline" ? "offline" : "connecting"}
            </span>
            <span>{status}</span>
            <span>{workspace.data?.readonly ? "readonly API" : "workspace"}</span>
            <span className="status-spacer" />
            <span>focus: {state.focus}</span>
            <span>⌘ P commands</span>
          </footer>
        </main>
      </div>
    </div>
  );
}

export function App() {
  return (
    <Routes>
      <Route path="/p/*" element={<WorkspaceScreen />} />
      <Route path="/n/*" element={<LegacyNoteRedirect />} />
      <Route path="/tags/*" element={<TagsPage />} />
      <Route path="/recent" element={<RecentPage />} />
      <Route path="/settings" element={<SettingsPage />} />
      <Route path="*" element={<WorkspaceScreen />} />
    </Routes>
  );
}

function LegacyNoteRedirect() {
  const path = useParams()["*"] ?? "";
  return <Navigate replace to={`/p/${path}`} />;
}

function RecentPage() {
  const navigate = useNavigate();
  const recent = (JSON.parse(localStorage.getItem("miku-recent") ?? "[]") as string[]).slice(0, 20);
  return (
    <main className="tags-page utility-page">
      <span className="eyebrow">Workspace</span>
      <h1>Recent notes</h1>
      <p>Notes opened most recently in this browser.</p>
      <div className="tag-note-list">
        {recent.length ? (
          recent.map((path) => (
            <button className="tag-note-row" key={path} onClick={() => navigate(`/p/${path}`)}>
              <strong>{path.split("/").pop()}</strong>
              <small>{path}</small>
            </button>
          ))
        ) : (
          <p className="search-empty">No recent notes yet.</p>
        )}
      </div>
    </main>
  );
}

function SettingsPage() {
  const navigate = useNavigate();
  const [theme, setTheme] = useState<Theme>(readTheme);
  const toggleTheme = () =>
    setTheme((current) => {
      const next = current === "dark" ? "light" : "dark";
      writeTheme(next);
      return next;
    });
  return (
    <main className="tags-page utility-page settings-page" data-theme={theme}>
      <button className="toolbar-button" onClick={() => navigate("/")}>
        ← Workspace
      </button>
      <span className="eyebrow">Configuration</span>
      <h1>Settings</h1>
      <div className="settings-card">
        <div>
          <strong>Theme</strong>
          <small>Current appearance: {theme}</small>
        </div>
        <button className="toolbar-button" onClick={toggleTheme}>
          Use {theme === "dark" ? "light" : "dark"} theme
        </button>
      </div>
      <div className="settings-card">
        <div>
          <strong>Source</strong>
          <small>miku_docs Markdown filesystem</small>
        </div>
        <span>authoritative</span>
      </div>
    </main>
  );
}

function TagsPage() {
  const client = useMemo(() => createWorkspaceClient(() => undefined), []);
  const navigate = useNavigate();
  const wildcard = useParams()["*"] ?? "";
  const tag = wildcard ? decodeURIComponent(wildcard) : "";
  const tags = useQuery({ queryKey: ["tags"], queryFn: client.tags });
  const notes = useQuery({ queryKey: ["tag-notes", tag], queryFn: () => client.tagNotes(tag), enabled: Boolean(tag) });
  return (
    <main className="tags-page">
      <div className="tags-page-header">
        <span className="eyebrow">Index</span>
        <h1>{tag ? `#${tag}` : "Tags"}</h1>
        <p>Browse indexed Markdown notes by tag.</p>
      </div>
      {tag ? (
        <div className="tag-note-list">
          {notes.isLoading ? (
            <p>Loading notes…</p>
          ) : (
            notes.data?.map((note) => (
              <button className="tag-note-row" key={note.path} onClick={() => navigate(`/p/${note.path}`)}>
                <strong>{note.title}</strong>
                <small>{note.path}</small>
              </button>
            ))
          )}
        </div>
      ) : (
        <div className="tag-index">
          {tags.isLoading ? (
            <p>Loading tags…</p>
          ) : (
            tags.data?.map((item) => (
              <button className="tag-index-row" key={item.tag} onClick={() => navigate(`/tags/${encodeURIComponent(item.tag)}`)}>
                <span>#{item.tag}</span>
                <small>{item.count}</small>
              </button>
            ))
          )}
        </div>
      )}
    </main>
  );
}
