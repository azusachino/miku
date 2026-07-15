import type { components } from "./generated/api";
import { fixtureApi, fixtureNotes, fixtureTree, type FixtureNote } from "./fixtures";

type Schemas = components["schemas"];
export type ApiSource = "connecting" | "live" | "fixtures";

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
  legacy: boolean;
  revision?: Schemas["RevisionResponse"];
};

export type TreeNodeModel = {
  placementId: string;
  noteId: string;
  parentId: string | null;
  note: Pick<NoteModel, "id" | "path" | "title" | "legacy" | "parents"> & { order?: number | null };
};

export type WorkspaceModel = {
  noteCount: number;
  placementCount: number;
  legacyCount: number;
  readonly: boolean;
};

export type ContextModel = {
  note: NoteModel;
  parents: TreeNodeModel["note"][];
  children: TreeNodeModel[];
  backlinks: string[];
};

export type SearchItem = { id: string; path: string; title: string; icon: string; snippet: string };

class ApiRequestError extends Error {
  constructor(message: string, readonly network: boolean, readonly status?: number) {
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

function fixtureNote(note: FixtureNote): NoteModel {
  return { ...note, legacy: false };
}

function normalizeNote(note: Schemas["NoteResponse"]): NoteModel {
  const frontmatter = note.frontmatter && typeof note.frontmatter === "object" ? note.frontmatter as Record<string, unknown> : {};
  const tags = Array.isArray(frontmatter.tags) ? frontmatter.tags.filter((tag): tag is string => typeof tag === "string") : [];
  return {
    id: note.note_id,
    path: note.path,
    title: note.title,
    icon: typeof frontmatter.icon === "string" ? frontmatter.icon : "•",
    parents: Array.isArray(frontmatter.parents) ? frontmatter.parents.filter((parent): parent is string => typeof parent === "string") : [],
    updated: note.revision.mtime ? new Date(note.revision.mtime * 1000).toLocaleString() : "unknown",
    body: note.body,
    backlinks: [],
    tags,
    legacy: note.legacy,
    revision: note.revision,
  };
}

function normalizeTreeNode(node: Schemas["TreeNode"]): TreeNodeModel {
  return {
    placementId: node.placement_id,
    noteId: node.note_id,
    parentId: node.parent_id ?? null,
    note: {
      id: node.note.note_id,
      path: node.note.path,
      title: node.note.title,
      legacy: node.note.legacy,
      parents: [],
      order: node.note.order,
    },
  };
}

function fallbackTree(): TreeNodeModel[] {
  return fixtureTree.map((node) => {
    const note = fixtureNotes.find((item) => item.id === node.noteId)!;
    return {
      placementId: node.placementId,
      noteId: node.noteId,
      parentId: node.parentId,
      note: { id: note.id, path: note.path, title: note.title, legacy: false, parents: note.parents },
    };
  });
}

export function createWorkspaceClient(onSource: (source: ApiSource) => void) {
  const withFallback = async <T,>(live: () => Promise<T>, fallback: () => Promise<T>): Promise<T> => {
    try {
      const result = await live();
      onSource("live");
      return result;
    } catch (error) {
      if (!(error instanceof ApiRequestError) || !error.network) throw error;
      onSource("fixtures");
      return fallback();
    }
  };

  const liveTree = async (): Promise<TreeNodeModel[]> => {
    // The tree endpoint is intentionally lazy. Fetching every descendant here
    // would turn a large legacy vault into one request per note; the active
    // note's children arrive through its context query and later expansion
    // work will request a specific parent.
    const response = await request<Schemas["TreeResponse"]>("/api/v1/tree");
    return response.nodes.map(normalizeTreeNode);
  };

  return {
    workspace: () => withFallback(
      async () => {
        const response = await request<Schemas["WorkspaceResponse"]>("/api/v1/workspace");
        return { noteCount: response.note_count, placementCount: response.placement_count, legacyCount: response.legacy_count, readonly: response.readonly };
      },
      async () => { const response = await fixtureApi.workspace(); return { ...response, legacyCount: 0 }; },
    ),
    tree: () => withFallback(liveTree, async () => fallbackTree()),
    note: (id: string) => withFallback(
      async () => normalizeNote(await request<Schemas["NoteResponse"]>(`/api/v1/notes/${encodeURIComponent(id)}`)),
      async () => fixtureNote(await fixtureApi.note(id)),
    ),
    context: (id: string) => withFallback(
      async () => {
        const response = await request<Schemas["ContextResponse"]>(`/api/v1/notes/${encodeURIComponent(id)}/context`);
        return {
          note: normalizeNote(response.note),
          parents: response.parents.map((parent) => ({ id: parent.note_id, path: parent.path, title: parent.title, legacy: parent.legacy, parents: [], order: parent.order })),
          children: response.children.map(normalizeTreeNode),
          backlinks: response.backlinks.map((backlink) => backlink.path),
        } satisfies ContextModel;
      },
      async () => {
        const note = fixtureNote(await fixtureApi.note(id));
        return { note, parents: [], children: fallbackTree().filter((child) => child.parentId === id), backlinks: note.backlinks };
      },
    ),
    search: (query: string): Promise<SearchItem[]> => withFallback(
      async () => {
        const params = new URLSearchParams({ q: query, limit: "20" });
        const response = await request<Schemas["SearchResponse"]>(`/api/v1/search?${params}`);
        return response.results.map((result) => ({ ...result, id: result.path, icon: "•" }));
      },
      async () => (await fixtureApi.search(query)).map((note) => ({ id: note.id, path: note.path, title: note.title, icon: note.icon, snippet: note.body })),
    ),
  };
}

export function subscribeToWorkspaceEvents(onInvalidate: () => void): () => void {
  if (typeof EventSource === "undefined") return () => undefined;
  const source = new EventSource("/events");
  source.onmessage = onInvalidate;
  return () => source.close();
}
