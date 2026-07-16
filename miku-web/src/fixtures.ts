export type FixtureNote = {
  id: string;
  path: string;
  title: string;
  icon: string;
  parents: string[];
  updated: string;
  body: string;
  backlinks: string[];
  tags: string[];
};

export type FixtureTreeNode = {
  placementId: string;
  noteId: string;
  parentId: string | null;
};

export const fixtureNotes: FixtureNote[] = [
  {
    id: "welcome",
    path: "Miku / Welcome",
    title: "Welcome to Miku",
    icon: "✦",
    parents: [],
    updated: "just now",
    body: "A quiet, file-backed workspace for thinking in Markdown.",
    backlinks: ["Miku / Roadmap"],
    tags: ["miku", "start-here"]
  },
  {
    id: "roadmap",
    path: "Miku / Roadmap",
    title: "0.0.3 roadmap",
    icon: "◈",
    parents: ["welcome"],
    updated: "12 min ago",
    body: "The workspace grows from the file outward: identity, placements, APIs, then the shell.",
    backlinks: ["Miku / Welcome"],
    tags: ["miku", "planning"]
  },
  {
    id: "workspace",
    path: "Projects / Miku workspace",
    title: "Miku workspace",
    icon: "⌘",
    parents: ["welcome"],
    updated: "28 min ago",
    body: "A note can appear in several places without copying its Markdown body.",
    backlinks: ["Miku / Roadmap", "Research / Trilium"],
    tags: ["miku", "architecture"]
  },
  {
    id: "trilium",
    path: "Research / Trilium",
    title: "Trilium UX notes",
    icon: "◇",
    parents: ["welcome"],
    updated: "1 hr ago",
    body: "Tree first, note second, context always close enough to be useful.",
    backlinks: ["Miku / Roadmap"],
    tags: ["research", "ux"]
  },
  {
    id: "shell",
    path: "Projects / Miku workspace / Shell",
    title: "Workspace shell",
    icon: "▦",
    parents: ["workspace", "trilium"],
    updated: "2 hr ago",
    body: "Tabs and splits are presentation state. The URL keeps the focused note recoverable.",
    backlinks: ["Miku / Miku workspace"],
    tags: ["ux", "frontend"]
  }
];

export const fixtureTree: FixtureTreeNode[] = fixtureNotes.flatMap((note) =>
  (note.parents.length ? note.parents : [null]).map((parentId) => ({
    placementId: `placement-${parentId ?? "root"}-${note.id}`,
    noteId: note.id,
    parentId
  }))
);

const wait = <T>(value: () => T): Promise<T> => new Promise((resolve) => window.setTimeout(() => resolve(value()), 80));

export const fixtureApi = {
  workspace: () => wait(() => ({ noteCount: fixtureNotes.length, placementCount: fixtureTree.length, readonly: true })),
  tree: (parentId: string | null) => wait(() => fixtureTree.filter((node) => node.parentId === parentId)),
  note: (id: string) => wait(() => fixtureNotes.find((note) => note.id === id) ?? fixtureNotes[0]),
  search: (query: string) =>
    wait(() => {
      const normalized = query.trim().toLowerCase();
      return fixtureNotes.filter((note) => !normalized || `${note.title} ${note.path} ${note.body}`.toLowerCase().includes(normalized));
    })
};
