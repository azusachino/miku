import { useEffect, useState } from "react";
import { createWorkspaceClient, sortTreeNodes, type NoteModel, type TreeNodeModel } from "./api";
import { readExpandedPaths, writeExpandedPaths } from "./ui";
import { ActionIcon, NoteIcon } from "./workspaceIcons";

export function WorkspaceTree({ notes, nodes, activeId, onSelect, hoisted, client }: {
  notes: NoteModel[];
  nodes: TreeNodeModel[];
  activeId: string;
  onSelect: (id: string) => void;
  hoisted: boolean;
  client: ReturnType<typeof createWorkspaceClient>;
}) {
  const noteMap = new Map(notes.map((note) => [note.id, note]));
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    const persisted = readExpandedPaths();
    return new Set(persisted.filter((path) => !persisted.some((parent) => parent !== path && path.startsWith(`${parent}/`))));
  });
  const [loaded, setLoaded] = useState<Record<string, TreeNodeModel[]>>({});
  const roots = sortTreeNodes(nodes.filter((node) => node.parentId === null));

  useEffect(() => {
    if (!activeId || hoisted) return;
    const ancestors = activeId.split("/").slice(0, -1).map((_, index, parts) => parts.slice(0, index + 1).join("/"));
    setExpanded((current) => {
      const next = new Set(current);
      ancestors.forEach((path) => next.add(path));
      return next;
    });
  }, [activeId, hoisted]);

  useEffect(() => writeExpandedPaths(expanded), [expanded]);

  useEffect(() => {
    let cancelled = false;
    const pending: TreeNodeModel[] = [];
    const collect = (items: TreeNodeModel[]) => {
      for (const node of items) {
        if (node.kind !== "folder" || !expanded.has(node.path)) continue;
        if (!loaded[node.path]) pending.push(node);
        else collect(loaded[node.path]);
      }
    };
    collect(roots);
    if (!pending.length) return;
    void Promise.all(pending.map(async (node) => [node.path, await client.tree(node.path)] as const)).then((entries) => {
      if (!cancelled) setLoaded((current) => ({ ...current, ...Object.fromEntries(entries) }));
    });
    return () => { cancelled = true; };
  }, [client, expanded, loaded, roots]);

  const branch = (node: TreeNodeModel, depth: number) => {
    const note = noteMap.get(node.noteId) ?? { ...node.note, icon: "file-text", updated: "unknown", body: "", backlinks: [], tags: [] };
    const children = sortTreeNodes(loaded[node.path] ?? []);
    const isFolder = node.kind === "folder";
    const isExpanded = expanded.has(node.path);
    const indexNote = children.find((child) => child.kind === "markdown" && child.path === `${node.path}/index.md`);
    const title = isFolder ? (indexNote?.note.title ?? node.note.title) : note.title;
    const toggleFolder = async () => {
      if (isExpanded) {
        setExpanded((current) => new Set([...current].filter((path) => path !== node.path)));
        return;
      }
      if (!loaded[node.path]) {
        const children = await client.tree(node.path);
        setLoaded((current) => ({ ...current, [node.path]: children }));
      }
      setExpanded((current) => new Set(current).add(node.path));
    };
    return <div key={node.placementId} className="tree-branch">
      <button className={`tree-row ${activeId === note.id ? "is-active" : ""}`} style={{ paddingLeft: `${14 + depth * 17}px` }} onClick={() => isFolder ? void toggleFolder() : onSelect(node.path)} aria-current={activeId === note.id ? "page" : undefined} aria-expanded={isFolder ? isExpanded : undefined}>
        <span className="tree-caret">{isFolder ? <ActionIcon name={isExpanded ? "chevron-down" : "chevron-right"} /> : null}</span>
        <span className="tree-icon">{isFolder ? <NoteIcon value="folder" /> : <NoteIcon value={note.icon} />}</span>
        <span className="tree-label">{title}</span>
      </button>
      {!hoisted && isExpanded && children.map((child) => branch(child, depth + 1))}
    </div>;
  };
  return <div className="tree-list">{roots.map((node) => branch(node, 0))}</div>;
}
