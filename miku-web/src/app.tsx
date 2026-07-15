import { useEffect, useMemo, useReducer, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { createWorkspaceClient, subscribeToWorkspaceEvents, type ApiSource, type NoteModel, type TreeNodeModel } from "./api";
import { fixtureNotes } from "./fixtures";
import { initialWorkspaceState, workspaceReducer } from "./workspace";

function Icon({ children }: { children: string }) {
  return <span className="icon" aria-hidden="true">{children}</span>;
}

function Tree({ notes, nodes, activeId, onSelect, hoisted }: { notes: NoteModel[]; nodes: TreeNodeModel[]; activeId: string; onSelect: (id: string) => void; hoisted: boolean }) {
  const noteMap = new Map(notes.map((note) => [note.id, note]));
  const roots = nodes.filter((node) => (hoisted ? node.noteId === activeId : node.parentId === null));
  const childrenOf = (id: string) => nodes.filter((node) => node.parentId === id);

  const branch = (node: TreeNodeModel, depth: number) => {
    const note = noteMap.get(node.noteId) ?? { ...node.note, icon: "•", updated: "unknown", body: "", backlinks: [], tags: [] };
    return <div key={node.placementId} className="tree-branch">
      <button
        className={`tree-row ${activeId === note.id ? "is-active" : ""}`}
        style={{ paddingLeft: `${14 + depth * 17}px` }}
        onClick={() => onSelect(note.id)}
        aria-current={activeId === note.id ? "page" : undefined}
      >
        <span className="tree-caret">{childrenOf(note.id).length ? "⌄" : "·"}</span>
        <span className="tree-note-icon">{note.icon}</span>
        <span className="tree-label">{note.title}</span>
      </button>
      {!hoisted && childrenOf(note.id).map((child) => branch(child, depth + 1))}
    </div>
  };

  return <div className="tree-list">{roots.map((note) => branch(note, 0))}</div>;
}

function LaunchBar({ query, setQuery, onSearch }: { query: string; setQuery: (value: string) => void; onSearch: () => void }) {
  return (
    <header className="launch-bar">
      <div className="brand-mark"><span className="brand-glyph">m</span><span>miku</span></div>
      <button className="vault-switcher" aria-label="Switch vault"><span className="status-dot" /> personal vault <Icon>⌄</Icon></button>
      <div className="launch-search">
        <Icon>⌕</Icon>
        <input value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => event.key === "Enter" && onSearch()} placeholder="Search notes, tags, commands" aria-label="Search notes" />
        <kbd>⌘ K</kbd>
      </div>
      <div className="launch-actions"><button className="quiet-button" aria-label="Quick add"><Icon>＋</Icon></button><button className="avatar" aria-label="Account">A</button></div>
    </header>
  );
}

function Sidebar({ notes, nodes, activeId, onSelect, hoisted, onToggleHoist }: { notes: NoteModel[]; nodes: TreeNodeModel[]; activeId: string; onSelect: (id: string) => void; hoisted: boolean; onToggleHoist: () => void }) {
  return (
    <aside className="sidebar">
      <div className="sidebar-toolbar"><span className="eyebrow">Workspace</span><button className={`tool-button ${hoisted ? "is-on" : ""}`} onClick={onToggleHoist} aria-label="Toggle hoisted note">⌃</button><button className="tool-button" aria-label="Tree options">•••</button></div>
      <div className="tree-heading"><span>All notes</span><span className="count-pill">{notes.length}</span></div>
      <Tree notes={notes} nodes={nodes} activeId={activeId} onSelect={onSelect} hoisted={hoisted} />
      <div className="sidebar-bottom"><button className="sidebar-link"><Icon>⌁</Icon> Bookmarks <span>3</span></button><button className="sidebar-link"><Icon>◷</Icon> Recent <span>12</span></button><button className="sidebar-link"><Icon>⚙</Icon> Settings</button></div>
    </aside>
  );
}

function Tabs({ notes, tabs, activeId, onSelect, onClose }: { notes: NoteModel[]; tabs: string[]; activeId: string; onSelect: (id: string) => void; onClose: (id: string) => void }) {
  return <div className="tabs" role="tablist">{tabs.map((id) => { const note = notes.find((item) => item.id === id) ?? { id, title: "Missing note", icon: "•" }; return <div key={id} className={`tab ${activeId === id ? "is-active" : ""}`} role="tab" aria-selected={activeId === id}><button onClick={() => onSelect(id)}><span className="tab-icon">{note.icon}</span>{note.title}</button><button className="tab-close" onClick={() => onClose(id)} aria-label={`Close ${note.title}`}>×</button></div>; })}<button className="new-tab" aria-label="New tab">＋</button></div>;
}

function NotePane({ note, split, onSplit }: { note: NoteModel; split: boolean; onSplit: () => void }) {
  return <section className={`note-pane ${split ? "is-split" : ""}`}>
    <div className="note-toolbar"><div className="breadcrumbs"><span>Vault</span><span>/</span><span>{note.path.split(" / ")[0]}</span><span>/</span><strong>{note.title}</strong></div><div className="note-actions"><button className="toolbar-button" onClick={onSplit}>{split ? "Single pane" : "Split pane"}</button><button className="toolbar-button">•••</button></div></div>
    <div className="note-scroll"><div className="note-kicker"><span className="note-icon-large">{note.icon}</span><span className="saved-state"><span className="saved-dot" /> saved</span></div><h1>{note.title}</h1><p className="note-subtitle">{note.path} <span>·</span> updated {note.updated}</p><div className="tag-row">{note.tags.map((tag) => <span className="tag" key={tag}>#{tag}</span>)}</div><div className="editor-card"><div className="editor-header"><span><span className="editor-dot" /> Markdown</span><button>Preview</button></div><div className="editor-body"><div className="line-numbers">1<br />2<br />3<br />4<br />5</div><pre><span className="md-heading"># {note.title}</span>{"\n\n"}<span className="md-body">{note.body}</span>{"\n\n"}<span className="md-muted">{"<!-- source editor fixture -->"}</span></pre></div></div><div className="note-footer"><span>⌘ Enter to edit</span><span>Markdown source</span></div></div>
  </section>;
}

function ContextPanel({ note, open, onToggle }: { note: NoteModel; open: boolean; onToggle: () => void }) {
  if (!open) return <button className="context-reopen" onClick={onToggle} aria-label="Open context panel">‹</button>;
  return <aside className="context-panel"><div className="context-header"><span className="eyebrow">Context</span><button className="tool-button" onClick={onToggle} aria-label="Close context panel">›</button></div><div className="context-section"><div className="context-title">Relations <span>{note.backlinks.length}</span></div>{note.backlinks.map((backlink) => <button className="relation-row" key={backlink}><span className="relation-line" /><span>{backlink}</span><Icon>↗</Icon></button>)}</div><div className="context-section"><div className="context-title">Properties</div><div className="property-row"><span>type</span><strong>text</strong></div><div className="property-row"><span>revision</span><strong>clean</strong></div><div className="property-row"><span>placements</span><strong>{note.parents.length || 1}</strong></div></div><div className="context-section"><div className="context-title">Activity</div><div className="activity"><span className="activity-dot" /><div><strong>Saved locally</strong><small>{note.updated}</small></div></div></div></aside>;
}

function WorkspaceScreen() {
  const [state, dispatch] = useReducer(workspaceReducer, initialWorkspaceState);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [apiSource, setApiSource] = useState<ApiSource>("connecting");
  const navigate = useNavigate();
  const location = useLocation();
  const routeId = useParams().id;
  const queryClient = useQueryClient();
  const client = useMemo(() => createWorkspaceClient(setApiSource), []);
  const activeId = routeId ?? state.activeId;
  const fallbackNotes = useMemo(() => fixtureNotes.map((note) => ({ ...note, legacy: false })), []);
  const workspace = useQuery({ queryKey: ["workspace"], queryFn: client.workspace });
  const tree = useQuery({ queryKey: ["tree"], queryFn: client.tree });
  const note = useQuery({ queryKey: ["note", activeId], queryFn: () => client.note(activeId), enabled: Boolean(activeId) });
  const context = useQuery({ queryKey: ["context", activeId], queryFn: () => client.context(activeId), enabled: Boolean(activeId) });
  const results = useQuery({ queryKey: ["search", query], queryFn: () => client.search(query), enabled: searchOpen });
  const isWorkspaceRoot = location.pathname === "/";
  const visibleTree = useMemo(() => [...(tree.data ?? []), ...(context.data?.children ?? [])], [context.data?.children, tree.data]);
  const treeNotes = useMemo(() => visibleTree.map((node) => ({ ...node.note, icon: "•", updated: "indexed", body: "", backlinks: [], tags: [] })), [visibleTree]);
  const notes = apiSource === "fixtures" || !visibleTree.length ? fallbackNotes : treeNotes;
  const activeNote = context.data?.note ?? note.data ?? notes.find((candidate) => candidate.id === activeId) ?? fallbackNotes[0];

  useEffect(() => { if (isWorkspaceRoot) navigate(`/n/${state.activeId}`, { replace: true }); }, [isWorkspaceRoot, navigate, state.activeId]);
  useEffect(() => subscribeToWorkspaceEvents(() => {
    void queryClient.invalidateQueries({ queryKey: ["workspace"] });
    void queryClient.invalidateQueries({ queryKey: ["tree"] });
    void queryClient.invalidateQueries({ queryKey: ["note"] });
    void queryClient.invalidateQueries({ queryKey: ["context"] });
    void queryClient.invalidateQueries({ queryKey: ["search"] });
  }), [queryClient]);
  useEffect(() => { const handler = (event: KeyboardEvent) => { if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); setSearchOpen(true); } }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler); }, []);

  const select = (id: string) => { dispatch({ type: "open", id }); navigate(`/n/${id}`); setSearchOpen(false); };
  const openSearch = () => setSearchOpen(true);
  const status = useMemo(() => workspace.data ? `${workspace.data.noteCount} notes · ${workspace.data.placementCount} placements` : "Loading workspace", [workspace.data]);

  const secondaryNote = notes.find((candidate) => candidate.id === (state.tabs.find((tab) => tab !== activeId) ?? "welcome")) ?? activeNote;
  return <div className="app-shell"><LaunchBar query={query} setQuery={setQuery} onSearch={openSearch} />{searchOpen && <div className="search-popover"><div className="search-popover-head"><span>Quick search</span><button onClick={() => setSearchOpen(false)}>Esc</button></div>{results.isLoading ? <div className="search-empty">Searching…</div> : results.data?.length ? results.data.map((result) => <button className="search-result" key={result.id} onClick={() => select(result.id)}><span className="search-result-icon">{result.icon}</span><span><strong>{result.title}</strong><small>{result.path}</small></span><kbd>↵</kbd></button>) : <div className="search-empty">No matching notes</div>}</div>}<div className="workspace-layout"><Sidebar notes={notes} nodes={visibleTree} activeId={activeId} onSelect={select} hoisted={state.hoisted} onToggleHoist={() => dispatch({ type: "toggle-hoist" })} /><main className="main-stage"><Tabs notes={notes} tabs={state.tabs} activeId={activeId} onSelect={select} onClose={(id) => dispatch({ type: "close", id })} /><div className="content-stage"><NotePane note={activeNote} split={state.split} onSplit={() => dispatch({ type: "toggle-split" })} />{state.split && <NotePane note={secondaryNote} split={false} onSplit={() => dispatch({ type: "toggle-split" })} />}<ContextPanel note={activeNote} open={state.contextOpen} onToggle={() => dispatch({ type: "toggle-context" })} /></div><footer className="status-bar"><span><span className="online-dot" /> {apiSource === "live" ? "live vault" : apiSource === "fixtures" ? "fixture vault" : "connecting"}</span><span>{status}</span><span>{workspace.data?.readonly ? "readonly API" : "workspace"}</span><span className="status-spacer" /><span>focus: {state.focus}</span><span>⌘ P commands</span></footer></main></div></div>;
}

export function App() {
  return <Routes><Route path="/n/:id" element={<WorkspaceScreen />} /><Route path="*" element={<WorkspaceScreen />} /></Routes>;
}
