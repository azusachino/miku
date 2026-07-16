import type { components } from "./generated/api";

type Schemas = components["schemas"];
export type ApiSource = "connecting" | "live" | "offline";

export type NoteModel = {
  id: string;
  path: string;
  title: string;
  icon: string;
  parents: string[];
  updated: string;
  body: string;
  backlinks: string[];
  tags: string[];
  identityGenerated: boolean;
  revision?: Schemas["RevisionResponse"];
};

export type TreeNodeModel = {
  kind: "folder" | "markdown";
  path: string;
  hasChildren: boolean;
  placementId: string;
  noteId: string;
  parentId: string | null;
  note: Pick<NoteModel, "id" | "path" | "title" | "identityGenerated" | "parents"> & { order?: number | null };
};

export function sortTreeNodes(nodes: TreeNodeModel[]): TreeNodeModel[] {
  return [...nodes].sort((left, right) => {
    if (left.kind !== right.kind) return left.kind === "folder" ? -1 : 1;
    return left.note.title.localeCompare(right.note.title, undefined, { sensitivity: "base", numeric: true }) || left.path.localeCompare(right.path);
  });
}

type ApiTreeNode = Schemas["TreeNode"];

type ApiTreeResponse = { parent_id: string | null; nodes: ApiTreeNode[] };

export type WorkspaceModel = {
  noteCount: number;
  placementCount: number;
  legacyCount: number;
  indexPhase: Schemas["WorkspaceResponse"]["index_phase"];
  readonly: boolean;
};

export type ContextModel = {
  note: NoteModel;
  parents: TreeNodeModel["note"][];
  children: TreeNodeModel[];
  backlinks: BacklinkModel[];
};

export type SearchItem = { id: string; path: string; title: string; icon: string; snippet: string };
export type SearchScope = "all" | "title" | "content";
export type TagModel = { tag: string; count: number };
export type BacklinkModel = { path: string; title: string };
export type TagNoteModel = { path: string; title: string; mtime: number };
export type SaveNoteInput = { body: string; title: string; expectedRevision: NonNullable<NoteModel["revision"]> };

const padDatePart = (value: number): string => String(value).padStart(2, "0");

export function formatUpdatedAt(unixSeconds: number | null | undefined): string {
  if (typeof unixSeconds !== "number" || !Number.isFinite(unixSeconds)) return "unknown";
  const date = new Date(unixSeconds * 1000);
  return `${date.getFullYear()}-${padDatePart(date.getMonth() + 1)}-${padDatePart(date.getDate())} ${padDatePart(date.getHours())}:${padDatePart(date.getMinutes())}:${padDatePart(date.getSeconds())}`;
}

export function extractInlineTags(markdown: string): string[] {
  const tags = new Set<string>();
  for (const [index, segment] of markdown.split(/(```[\s\S]*?```|`[^`]*`)/g).entries()) {
    if (index % 2 === 1) continue;
    for (const match of segment.matchAll(/(?:^|[\s(])#([\p{L}\p{N}][\p{L}\p{N}_\-/]*)/gu)) tags.add(match[1]);
  }
  return [...tags].sort();
}

class ApiRequestError extends Error {
  constructor(
    message: string,
    readonly network: boolean,
    readonly status?: number
  ) {
    super(message);
  }
}

async function request<T>(path: string): Promise<T> {
  try {
    const response = await fetch(path, { headers: { Accept: "application/json" } });
    if (!response.ok) throw new ApiRequestError(`${response.status} ${response.statusText}`, false, response.status);
    return (await response.json()) as T;
  } catch (error) {
    if (error instanceof ApiRequestError) throw error;
    throw new ApiRequestError(error instanceof Error ? error.message : "request failed", true);
  }
}

function normalizeNote(note: Schemas["NoteResponse"]): NoteModel {
  const frontmatter = note.frontmatter && typeof note.frontmatter === "object" ? (note.frontmatter as Record<string, unknown>) : {};
  const frontmatterTags = Array.isArray(frontmatter.tags) ? frontmatter.tags.filter((tag): tag is string => typeof tag === "string").map((tag) => tag.replace(/^#/, "")) : [];
  const tags = [...new Set([...frontmatterTags, ...extractInlineTags(note.body)])].sort();
  return {
    id: note.path,
    path: note.path,
    title: note.title,
    icon: typeof frontmatter.icon === "string" ? frontmatter.icon : "file-text",
    parents: Array.isArray(frontmatter.parents) ? frontmatter.parents.filter((parent): parent is string => typeof parent === "string") : [],
    updated: formatUpdatedAt(note.revision.mtime),
    body: note.body,
    backlinks: [],
    tags,
    identityGenerated: note.identity_generated,
    revision: note.revision
  };
}

function normalizeTreeNode(node: ApiTreeNode): TreeNodeModel {
  return {
    kind: node.kind === "folder" ? "folder" : "markdown",
    path: node.note.path,
    hasChildren: node.has_children,
    placementId: node.placement_id,
    noteId: node.note_id,
    parentId: node.parent_id ?? null,
    note: {
      id: node.note.path,
      path: node.note.path,
      title: node.note.title,
      identityGenerated: node.note.identity_generated,
      parents: [],
      order: node.note.order
    }
  };
}

export function createWorkspaceClient(onSource: (source: ApiSource) => void) {
  const live = async <T>(requestLive: () => Promise<T>): Promise<T> => {
    try {
      const result = await requestLive();
      onSource("live");
      return result;
    } catch (error) {
      if (error instanceof ApiRequestError && error.network) onSource("offline");
      throw error;
    }
  };

  const liveTree = async (folder?: string): Promise<TreeNodeModel[]> => {
    // The tree endpoint is intentionally lazy. Fetching every descendant here
    // would turn a large vault into one request per note; the active
    // note's children arrive through its context query and later expansion
    // work will request a specific parent.
    const query = folder ? `?folder=${encodeURIComponent(folder)}` : "";
    const response = await request<ApiTreeResponse>(`/api/v1/tree${query}`);
    return sortTreeNodes(response.nodes.map(normalizeTreeNode));
  };

  return {
    workspace: () =>
      live(async () => {
        const response = await request<Schemas["WorkspaceResponse"]>("/api/v1/workspace");
        return {
          noteCount: response.note_count,
          placementCount: response.placement_count,
          generatedIdentityCount: response.generated_identity_count,
          indexPhase: response.index_phase,
          readonly: response.readonly
        };
      }),
    tree: (folder?: string) => live(() => liveTree(folder)),
    note: (id: string) => live(() => request<Schemas["NoteResponse"]>(`/api/v1/notes/${encodeURIComponent(id)}`).then(normalizeNote)),
    saveNote: async (id: string, input: SaveNoteInput): Promise<NoteModel> => {
      const response = await fetch(`/api/v1/notes/${encodeURIComponent(id)}`, {
        method: "PUT",
        headers: { Accept: "application/json", "Content-Type": "application/json" },
        body: JSON.stringify({ body: input.body, title: input.title, expected_revision: input.expectedRevision })
      });
      if (!response.ok) throw new ApiRequestError(`${response.status} ${response.statusText}`, false, response.status);
      onSource("live");
      return normalizeNote((await response.json()) as Schemas["NoteResponse"]);
    },
    context: (id: string) =>
      live(async () => {
        const response = await request<Schemas["ContextResponse"]>(`/api/v1/note-context/${encodeURIComponent(id)}`);
        return {
          note: normalizeNote(response.note),
          parents: response.parents.map((parent) => ({ id: parent.path, path: parent.path, title: parent.title, identityGenerated: parent.identity_generated, parents: [], order: parent.order })),
          children: sortTreeNodes(response.children.map((node) => normalizeTreeNode(node as ApiTreeNode))),
          backlinks: response.backlinks.map((backlink) => ({ path: backlink.path, title: backlink.title }))
        } satisfies ContextModel;
      }),
    search: (query: string, scope: SearchScope = "all"): Promise<SearchItem[]> =>
      live(async () => {
        const params = new URLSearchParams({ q: query, limit: "20", scope });
        const response = await request<Schemas["SearchResponse"]>(`/api/v1/search?${params}`);
        return response.results.map((result) => ({ ...result, id: result.path, icon: "file-text" }));
      }),
    tags: (): Promise<TagModel[]> => live(() => request<Schemas["TagResponse"][]>("/api/v1/tags")),
    tagNotes: (tag: string): Promise<TagNoteModel[]> => live(() => request<Schemas["TagNoteResponse"][]>(`/api/v1/tags/${encodeURIComponent(tag)}/notes`))
  };
}

export function subscribeToWorkspaceEvents(onInvalidate: () => void): () => void {
  if (typeof EventSource === "undefined") return () => undefined;
  const source = new EventSource("/events");
  source.onmessage = onInvalidate;
  return () => source.close();
}
