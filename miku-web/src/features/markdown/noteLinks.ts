export type NoteCandidate = { id?: string; path: string; title?: string; aliases?: string[] };
export type TargetResolver = (target: string) => string | null;

export function isAssetFile(path: string): boolean {
  const lower = path.trim().toLowerCase();
  return (
    lower.endsWith(".png") ||
    lower.endsWith(".jpg") ||
    lower.endsWith(".jpeg") ||
    lower.endsWith(".gif") ||
    lower.endsWith(".svg") ||
    lower.endsWith(".webp") ||
    lower.endsWith(".pdf")
  );
}

// Folds whitespace, hyphens, and underscores out of a name so that
// "elden ring", "elden-ring", and "Elden_Ring" compare equal.
function foldName(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/\.md$/, "")
    .replace(/[\s_-]+/g, "");
}

export function createTargetResolver(notes: NoteCandidate[], currentPath?: string): TargetResolver {
  const pathMap = new Map<string, string>();
  const nameMap = new Map<string, string[]>();

  const addName = (name: string | undefined, path: string) => {
    if (!name) return;
    const folded = foldName(name);
    if (!folded) return;
    const candidates = nameMap.get(folded) ?? [];
    if (!candidates.includes(path)) candidates.push(path);
    nameMap.set(folded, candidates);
  };

  for (const note of notes) {
    if (!note.path) continue;
    const normPath = note.path.toLowerCase().replace(/\.md$/, "");
    pathMap.set(normPath, note.path);

    // Every name a wikilink might use to reach this note: its filename,
    // its title, and its frontmatter aliases.
    addName(note.path.split("/").pop() ?? note.path, note.path);
    addName(note.title, note.path);
    for (const alias of note.aliases ?? []) addName(alias, note.path);
  }

  const pickBest = (candidates: string[]): string => {
    if (candidates.length === 1) return candidates[0];
    // Same-directory locality priority if ambiguous
    if (currentPath) {
      const folderDir = currentPath.split("/").slice(0, -1).join("/").toLowerCase();
      const localMatch = candidates.find((m) => m.toLowerCase().startsWith(folderDir + "/"));
      if (localMatch) return localMatch;
    }
    // Top-level / Shortest path fallback
    const sorted = [...candidates].sort((a, b) => a.length - b.length || a.localeCompare(b));
    return sorted[0];
  };

  return (target: string) => {
    const trimmed = target.trim();
    if (!trimmed) return null;
    const lower = trimmed.toLowerCase().replace(/\.md$/, "");

    // 1. Exact path match
    if (pathMap.has(lower)) {
      return pathMap.get(lower)!;
    }

    // 2. Folded name match against filename, title, or alias
    const folded = foldName(trimmed);
    const matches = nameMap.get(folded);
    if (matches && matches.length > 0) return pickBest(matches);

    // 3. Singular / Plural variation (e.g. kb-convention -> kb-conventions)
    const altFolded = folded.endsWith("s") ? folded.slice(0, -1) : `${folded}s`;
    const altMatches = nameMap.get(altFolded);
    if (altMatches && altMatches.length > 0) return pickBest(altMatches);

    // 4. Substring end-of-path match
    for (const [normPath, fullPath] of pathMap.entries()) {
      if (normPath.endsWith(`/${lower}`) || normPath.endsWith(`/${altFolded}`)) {
        return fullPath;
      }
    }

    return null;
  };
}

export type OutgoingLinkItem = { path: string; title: string; isMissing: boolean };

export function extractOutgoingLinks(body: string, notes?: NoteCandidate[]): OutgoingLinkItem[] {
  if (!body) return [];
  const matches = body.matchAll(/(?<!!)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g);
  const resolver = notes ? createTargetResolver(notes) : null;
  const seen = new Set<string>();
  const results: OutgoingLinkItem[] = [];

  for (const match of matches) {
    const target = match[1].trim();
    if (isAssetFile(target)) continue;
    const label = match[2]?.trim();
    const resolvedPath = resolver ? resolver(target) : null;
    const isMissing = !resolvedPath;
    const finalPath = resolvedPath || (target.endsWith(".md") ? target : `${target}.md`);

    if (seen.has(finalPath)) continue;
    seen.add(finalPath);

    const matchedNote = notes?.find((n) => n.path === finalPath);
    const title = label || matchedNote?.title || target.split("/").pop()?.replace(/\.md$/, "") || target;

    results.push({ path: finalPath, title, isMissing });
  }

  return results;
}
