import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import { createWorkspaceClient, sortTreeNodes, type BacklinkModel, type NoteModel, type TreeNodeModel } from "./api";
import { ActionIcon, NoteIcon } from "../../components/workspace/icons";
import { WorkspaceTree } from "../../components/workspace/WorkspaceTree";
import { headingSlug, shellRegions, type Theme } from "../../shared/ui";

const MarkdownEditor = lazy(() => import("../markdown/MarkdownEditor"));
const MarkdownReader = lazy(() => import("../markdown/MarkdownReader").then((module) => ({ default: module.MarkdownReader })));

function noteHeadings(markdown: string): { id: string; text: string; level: number }[] {
  const headings: { id: string; text: string; level: number }[] = [];
  const seen = new Map<string, number>();
  for (const line of markdown.split("\n")) {
    const match = line.match(/^(#{2,4})\s+(.+?)\s*#*$/);
    if (!match) continue;
    const text = match[2].replace(/[*_`]/g, "").trim();
    const base = headingSlug(text);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    headings.push({ id: count ? `${base}-${count}` : base, text, level: match[1].length });
  }
  return headings;
}

export function LaunchBar({ onSearch, theme, onToggleTheme }: { onSearch: () => void; theme: "dark" | "light"; onToggleTheme: () => void }) {
  const navigate = useNavigate();
  return (
    <header className="launch-bar" data-region={shellRegions[0]}>
      <button className="brand-mark" onClick={() => navigate("/")} aria-label="Go to workspace home">
        <img className="brand-icon" src={`/miku-icon-${theme}.svg`} alt="" />
        <span>miku note</span>
      </button>
      <button className="launch-search" onClick={onSearch} aria-label="Open quick search">
        <ActionIcon name="search" />
        <span className="launch-search-placeholder">Search notes, tags, commands</span>
        <kbd>⌘ K</kbd>
      </button>
      <div className="launch-actions">
        <button className="quiet-button" aria-label="Toggle theme" onClick={onToggleTheme}>
          <ActionIcon name={theme === "dark" ? "sun" : "moon"} />
        </button>
      </div>
    </header>
  );
}

export function Sidebar({
  notes,
  nodes,
  activeId,
  onSelect,
  hoisted,
  onToggleHoist,
  client,
  onTags,
  onRecent,
  onSettings,
  noteCount,
  onResizeStart
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
  noteCount: number;
  onResizeStart: (event: React.PointerEvent<HTMLButtonElement>) => void;
}) {
  return (
    <aside className="sidebar" data-region={shellRegions[1]}>
      <button className="sidebar-resizer" onPointerDown={onResizeStart} aria-label="Resize workspace navigation" />
      <div className="sidebar-toolbar">
        <span className="eyebrow">Workspace</span>
        <button
          className={`tool-button ${hoisted ? "is-on" : ""}`}
          onClick={onToggleHoist}
          aria-label={hoisted ? "Expand workspace tree" : "Collapse workspace tree"}
          aria-pressed={hoisted}
          title={hoisted ? "Expand workspace tree" : "Collapse workspace tree"}
        >
          <ActionIcon name="tree" />
        </button>
      </div>
      <div className="tree-heading">
        <span>All notes</span>
        <span className="count-pill">{noteCount}</span>
      </div>
      <WorkspaceTree notes={notes} nodes={nodes} activeId={activeId} onSelect={onSelect} hoisted={hoisted} client={client} />
      <div className="sidebar-bottom">
        <button className="sidebar-link" onClick={onRecent}>
          <ActionIcon name="clock" /> Recent
        </button>
        <button className="sidebar-link" onClick={onTags}>
          <ActionIcon name="hash" /> Tags
        </button>
        <button className="sidebar-link" onClick={onSettings}>
          <ActionIcon name="settings" /> Settings
        </button>
      </div>
    </aside>
  );
}

export function Tabs({
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
        const note = id === activeId ? activeNote : (notes.find((item) => item.id === id) ?? { id, title: "Loading note…", path: id, icon: "file-text" });
        return (
          <div key={id} className={`tab ${activeId === id ? "is-active" : ""}`} role="tab" aria-selected={activeId === id}>
            <button className="tab-label" onClick={() => onSelect(id)} title={note.path}>
              <NoteIcon value={note.icon} />
              <span>{note.title}</span>
            </button>
            <button className="tab-close" onClick={() => onClose(id)} aria-label={`Close ${note.title}`}>
              <ActionIcon name="close" />
            </button>
          </div>
        );
      })}
    </div>
  );
}

export function NotePane({
  note,
  split,
  onSplit,
  readonly,
  indexPhase,
  client,
  onTagSearch,
  onNavigatePath,
  theme
}: {
  note: NoteModel;
  split: boolean;
  onSplit: () => void;
  readonly: boolean;
  indexPhase?: string;
  client: ReturnType<typeof createWorkspaceClient>;
  onTagSearch: (tag: string) => void;
  onNavigatePath: (path: string) => void;
  theme: Theme;
}) {
  const [draft, setDraft] = useState(note.body);
  const [saveState, setSaveState] = useState("saved");
  const [sourceMode, setSourceMode] = useState(false);
  useEffect(() => {
    setDraft(note.body);
    setSaveState("saved");
    setSourceMode(false);
  }, [note.id, note.body]);
  const save = async () => {
    if (readonly || !note.revision) return;
    setSaveState("saving…");
    try {
      await client.saveNote(note.id, { body: draft, title: note.title, expectedRevision: note.revision });
      setSaveState("saved");
      setSourceMode(false);
    } catch (error) {
      setSaveState(error instanceof Error && error.message.startsWith("409") ? "conflict" : "save failed");
    }
  };
  return (
    <section className={`note-pane ${split ? "is-split" : ""}`} data-region={shellRegions[2]}>
      <div className="note-toolbar">
        <nav className="breadcrumbs" aria-label="Note location">
          {note.path.split("/").map((part, index, parts) => (
            <span key={`${part}-${index}`}>
              {index > 0 && <span aria-hidden="true">/</span>}
              <button
                className="breadcrumb-link"
                disabled={index === parts.length - 1}
                onClick={() => onNavigatePath(parts.slice(0, index + 1).join("/"))}
                aria-current={index === parts.length - 1 ? "page" : undefined}
              >
                {index === parts.length - 1 ? note.title : part}
              </button>
            </span>
          ))}
        </nav>
        <div className="note-actions">
          <button className="toolbar-button" onClick={onSplit}>
            {split ? "Single pane" : "Split pane"}
          </button>
          {!readonly && (
            <div className="view-switch" role="tablist" aria-label="Note view">
              <button className={!sourceMode ? "is-active" : ""} onClick={() => setSourceMode(false)} role="tab" aria-selected={!sourceMode}>
                Reader
              </button>
              <button className={sourceMode ? "is-active" : ""} onClick={() => setSourceMode(true)} role="tab" aria-selected={sourceMode}>
                Source
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="note-scroll">
        <div className="note-header">
          <span className="note-icon-large">
            <NoteIcon value={note.icon} large />
          </span>
          <div className="note-heading-copy">
            <h1>{note.title}</h1>
            <ul className="note-meta-list">
              <li>
                <span className="meta-label">type</span> Markdown note
              </li>
              <li>
                <span className="meta-label">status</span>{" "}
                <span className="saved-state">
                  <span className="saved-dot" /> {sourceMode ? saveState : "reading"}
                </span>
              </li>
              <li>
                <span className="meta-label">updated</span> {note.updated}
              </li>
              {note.tags.length > 0 && (
                <li className="note-meta-tags">
                  <span className="tag-row">
                    {note.tags.map((tag) => (
                      <button className="tag" key={tag} onClick={() => onTagSearch(tag)}>
                        #{tag}
                      </button>
                    ))}
                  </span>
                </li>
              )}
            </ul>
          </div>
        </div>
        {sourceMode ? (
          <Suspense fallback={<div className="markdown-editor-loading">Loading editor…</div>}>
            <MarkdownEditor
              noteId={note.id}
              value={draft}
              readOnly={readonly}
              theme={theme}
              onChange={(value) => {
                setDraft(value);
                setSaveState("unsaved");
              }}
            />
          </Suspense>
        ) : (
          <Suspense fallback={<div className="markdown-editor-loading">Rendering Markdown…</div>}>
            <MarkdownReader value={note.body} path={note.path} theme={theme} />
          </Suspense>
        )}
        {sourceMode && (
          <div className="note-footer">
            <span>Markdown source · changes stay local until saved</span>
            <button className="toolbar-button" disabled={readonly || !note.revision || saveState === "saving…"} onClick={save}>
              Save
            </button>
          </div>
        )}
      </div>
    </section>
  );
}

export function ContextPanel({
  note,
  backlinks,
  indexPhase,
  open,
  onToggle,
  onNavigate,
  onResizeStart
}: {
  note: NoteModel;
  backlinks: BacklinkModel[];
  indexPhase?: string;
  open: boolean;
  onToggle: () => void;
  onNavigate: (path: string) => void;
  onResizeStart: (event: React.PointerEvent<HTMLButtonElement>) => void;
}) {
  if (!open)
    return (
      <button className="context-reopen" onClick={onToggle} aria-label="Open context panel" title="Open context panel">
        <ActionIcon name="chevron-left" />
      </button>
    );
  return (
    <aside className="context-panel" data-region={shellRegions[3]}>
      <button className="context-resizer" onPointerDown={onResizeStart} aria-label="Resize note context panel" />
      <div className="context-header">
        <span className="eyebrow">Context</span>
        <button className="tool-button" onClick={onToggle} aria-label="Close context panel" title="Close context panel">
          <ActionIcon name="chevron-right" />
        </button>
      </div>
      <div className="context-section">
        <div className="context-title">
          Backlinks <span>{backlinks.length}</span>
        </div>
        {backlinks.length ? (
          backlinks.map((backlink) => (
            <button className="relation-row backlink-row" key={backlink.path} onClick={() => onNavigate(backlink.path)}>
              <span className="relation-line" />
              <span className="relation-copy">
                <strong>{backlink.title}</strong>
                <small>{backlink.path}</small>
              </span>
              <ActionIcon name="arrow-up-right" />
            </button>
          ))
        ) : (
          <p className="context-empty">No backlinks indexed yet.</p>
        )}
      </div>
      <div className="context-section">
        <div className="context-title">
          On this page <span>{noteHeadings(note.body).length}</span>
        </div>
        {noteHeadings(note.body).length ? (
          <nav className="toc-list" aria-label="Table of contents">
            {noteHeadings(note.body).map((heading) => (
              <a
                className={`toc-item toc-level-${heading.level}`}
                href={`#${heading.id}`}
                key={heading.id}
                onClick={(event) => {
                  event.preventDefault();
                  window.history.replaceState(window.history.state, "", `${window.location.pathname}${window.location.search}#${heading.id}`);
                  document.getElementById(heading.id)?.scrollIntoView({ behavior: "smooth", block: "start" });
                }}
              >
                {heading.text}
              </a>
            ))}
          </nav>
        ) : (
          <p className="context-empty">No headings in this note.</p>
        )}
      </div>
      <div className="context-section">
        <div className="context-title">Properties</div>
        <div className="property-row">
          <span>type</span>
          <strong>text</strong>
        </div>
        <div className="property-row">
          <span>revision</span>
          <strong>{note.revision ? note.revision.content_hash.slice(0, 8) : "unavailable"}</strong>
        </div>
        <div className="property-row">
          <span>placements</span>
          <strong>{note.parents.length || 1}</strong>
        </div>
      </div>
      <div className="context-section">
        <div className="context-title">Source</div>
        <div className="property-row">
          <span>file</span>
          <strong title={note.path}>{note.path.split("/").pop()}</strong>
        </div>
        <div className="property-row">
          <span>index</span>
          <strong>{indexPhase ?? "unknown"}</strong>
        </div>
      </div>
    </aside>
  );
}

export function WorkspaceUtility({
  route,
  theme,
  onToggleTheme,
  client
}: {
  route: "recent" | "tags" | "settings";
  theme: Theme;
  onToggleTheme: () => void;
  client: ReturnType<typeof createWorkspaceClient>;
}) {
  const navigate = useNavigate();
  const wildcard = useParams()["*"] ?? "";
  const tag = route === "tags" && wildcard ? decodeURIComponent(wildcard) : "";
  const tags = useQuery({ queryKey: ["tags"], queryFn: client.tags, enabled: route === "tags" });
  const tagNotes = useQuery({ queryKey: ["tag-notes", tag], queryFn: () => client.tagNotes(tag), enabled: route === "tags" && Boolean(tag) });
  const [tagLimit, setTagLimit] = useState(10);
  const tagSentinelRef = useRef<HTMLDivElement>(null);
  const recent = route === "recent" ? (JSON.parse(localStorage.getItem("miku-recent") ?? "[]") as string[]).slice(0, 20) : [];
  const visibleTags = tags.data?.slice(0, tagLimit) ?? [];
  useEffect(() => setTagLimit(10), [tags.data]);
  useEffect(() => {
    if (route !== "tags" || tag || !tagSentinelRef.current) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) setTagLimit((current) => Math.min(current + 10, tags.data?.length ?? current));
      },
      { rootMargin: "120px" }
    );
    observer.observe(tagSentinelRef.current);
    return () => observer.disconnect();
  }, [route, tag, tags.data]);
  return (
    <div className="workspace-utility" data-theme={theme}>
      <div className="utility-page-header">
        <div>
          <h1>{route === "recent" ? "Recent notes" : tag ? `#${tag}` : "Tags"}</h1>
          <p>{route === "recent" ? "Notes opened most recently in this browser." : "Browse indexed Markdown notes by tag."}</p>
        </div>
      </div>
      {route === "recent" && (
        <div className="utility-list">
          {recent.length ? (
            recent.map((path) => (
              <button className="utility-row" key={path} onClick={() => navigate(`/p/${path}`)}>
                <strong>{path.split("/").pop()}</strong>
                <small>{path}</small>
              </button>
            ))
          ) : (
            <p className="search-empty">No recent notes yet.</p>
          )}
        </div>
      )}
      {route === "tags" && (
        <div className="utility-list">
          {tag ? (
            tagNotes.isLoading ? (
              <p>Loading notes…</p>
            ) : (
              tagNotes.data?.map((note) => (
                <button className="utility-row" key={note.path} onClick={() => navigate(`/p/${note.path}`)}>
                  <strong>{note.title}</strong>
                  <small>{note.path}</small>
                </button>
              ))
            )
          ) : tags.isLoading ? (
            <p>Loading tags…</p>
          ) : (
            visibleTags.map((item) => (
              <button className="utility-row" key={item.tag} onClick={() => navigate(`/tags/${encodeURIComponent(item.tag)}`)}>
                <strong>#{item.tag}</strong>
                <small>{item.count} notes</small>
              </button>
            ))
          )}
          {!tag && <div ref={tagSentinelRef} className="utility-list-sentinel" aria-hidden="true" />}
        </div>
      )}
    </div>
  );
}

export function SettingsDialog({ theme, onToggleTheme, onClose }: { theme: Theme; onToggleTheme: () => void; onClose: () => void }) {
  return (
    <div className="settings-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <div className="settings-dialog-header">
          <div>
            <span className="eyebrow">Workspace</span>
            <h2 id="settings-title">Settings</h2>
          </div>
          <button className="quiet-button" onClick={onClose} aria-label="Close settings">
            <ActionIcon name="close" />
          </button>
        </div>
        <div className="settings-dialog-row">
          <div>
            <strong>Theme</strong>
            <small>Current appearance: {theme}</small>
          </div>
          <button className="toolbar-button" onClick={onToggleTheme}>
            Use {theme === "dark" ? "light" : "dark"} theme
          </button>
        </div>
        <div className="settings-dialog-row">
          <div>
            <strong>Source</strong>
            <small>Local Markdown files</small>
          </div>
          <span className="utility-status">authoritative</span>
        </div>
      </section>
    </div>
  );
}

export function FolderBrowser({
  path,
  nodes,
  isLoading,
  isError,
  onSelect,
  onOpenFolder,
  onNavigatePath
}: {
  path: string;
  nodes: TreeNodeModel[];
  isLoading: boolean;
  isError: boolean;
  onSelect: (id: string) => void;
  onOpenFolder: (path: string) => void;
  onNavigatePath: (path: string) => void;
}) {
  const title = path.split("/").at(-1) || path;
  return (
    <section className="folder-browser" aria-labelledby="folder-browser-title">
      <div className="folder-browser-header">
        <div>
          <span className="eyebrow">Folder</span>
          <h1 id="folder-browser-title">{title}</h1>
          <nav className="folder-breadcrumbs" aria-label="Folder location">
            {path.split("/").map((part, index, parts) => (
              <span key={`${part}-${index}`}>
                <span aria-hidden="true">/</span>
                <button className="breadcrumb-link" disabled={index === parts.length - 1} onClick={() => onNavigatePath(parts.slice(0, index + 1).join("/"))}>
                  {part}
                </button>
              </span>
            ))}
          </nav>
        </div>
        <span className="folder-browser-count">{nodes.length} items</span>
      </div>
      {isLoading ? (
        <p className="search-empty">Loading folder…</p>
      ) : isError ? (
        <p className="search-empty">Unable to load this folder.</p>
      ) : nodes.length ? (
        <div className="folder-card-grid">
          {sortTreeNodes(nodes).map((node) => (
            <button className="folder-card" key={node.placementId} onClick={() => (node.kind === "folder" ? onOpenFolder(node.path) : onSelect(node.path))}>
              <span className="folder-card-icon">
                <NoteIcon value={node.kind === "folder" ? "folder" : "file-text"} />
              </span>
              <span className="folder-card-copy">
                <strong>{node.note.title}</strong>
                <small>{node.kind === "folder" ? "Folder" : node.path}</small>
              </span>
              <ActionIcon name="chevron-right" />
            </button>
          ))}
        </div>
      ) : (
        <p className="search-empty">This folder is empty.</p>
      )}
    </section>
  );
}
