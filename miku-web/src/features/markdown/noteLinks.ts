export type NoteCandidate = { id?: string; path: string; title?: string; aliases?: string[] };
export type TargetResolver = (target: string) => string | null;

function proseSegments(markdown: string): string[] {
  return markdown.split(/(```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]*`)/g).filter((_, index) => index % 2 === 0);
}

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
export function foldName(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/\.md$/, "")
    .replace(/[\s_-]+/g, "");
}

function cleanPath(p: string): string {
  try {
    p = decodeURIComponent(p);
  } catch {
    // keep raw if decode fails
  }
  return p.trim().replace(/^\/+/, "").toLowerCase().replace(/\.md$/, "");
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
    const normPath = cleanPath(note.path);
    pathMap.set(normPath, note.path);
    if (note.id) {
      pathMap.set(cleanPath(note.id), note.path);
    }

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
    const lower = cleanPath(trimmed);

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

/**
 * Builds a resolver from the backend's already-resolved outgoing links
 * (ADR-0022: `resolve_named_path` run server-side against the full page
 * index, not the frontend's lazily-loaded, possibly-incomplete notes
 * list). Keyed by folded target text so it matches exactly what the
 * backend used to resolve the same link in the first place.
 */
export function createResolverFromOutgoingLinks(links: { target: string; path: string }[]): TargetResolver {
  const map = new Map<string, string>();
  for (const link of links) {
    const folded = foldName(link.target);
    if (folded && !map.has(folded)) map.set(folded, link.path);
  }
  return (target: string) => map.get(foldName(target)) ?? null;
}

/** Tries `primary` first, falling back to `secondary` when it finds nothing. */
export function combineResolvers(primary: TargetResolver, secondary?: TargetResolver): TargetResolver {
  if (!secondary) return primary;
  return (target: string) => primary(target) ?? secondary(target);
}

export type OutgoingLinkItem = { path: string; title: string; isMissing: boolean };

export function extractOutgoingLinks(body: string, notes?: NoteCandidate[], currentPath?: string): OutgoingLinkItem[] {
  if (!body) return [];
  const resolver = notes ? createTargetResolver(notes, currentPath) : null;
  const seen = new Set<string>();
  const results: OutgoingLinkItem[] = [];

  const addLink = (target: string, label?: string) => {
    let cleanTarget = target.trim();
    if (!cleanTarget || isAssetFile(cleanTarget) || cleanTarget.startsWith("#") || /^[a-z][a-z\d+.-]*:/i.test(cleanTarget)) {
      return;
    }
    if (cleanTarget.startsWith("/p/")) {
      cleanTarget = cleanTarget.slice(3).split("#")[0];
    } else {
      cleanTarget = cleanTarget.split("#")[0];
    }
    if (!cleanTarget) return;

    const resolvedPath = resolver ? resolver(cleanTarget) : null;
    const isMissing = !resolvedPath;

    let finalPath: string;
    if (resolvedPath) {
      finalPath = resolvedPath.replace(/^\/+/, "");
    } else if (currentPath && !cleanTarget.includes("/")) {
      const folderDir = currentPath.split("/").slice(0, -1).join("/");
      const subPath = cleanTarget.endsWith(".md") ? cleanTarget : `${cleanTarget}.md`;
      finalPath = folderDir ? `${folderDir}/${subPath}` : subPath;
    } else {
      finalPath = (cleanTarget.endsWith(".md") ? cleanTarget : `${cleanTarget}.md`).replace(/^\/+/, "");
    }

    if (seen.has(finalPath)) return;
    seen.add(finalPath);

    const matchedNote = notes?.find((n) => n.path === finalPath || cleanPath(n.path) === cleanPath(finalPath));
    const displayLabel = label?.trim();
    const title = displayLabel || matchedNote?.title || finalPath.split("/").pop()?.replace(/\.md$/, "") || finalPath;

    results.push({ path: finalPath, title, isMissing });
  };

  for (const segment of proseSegments(body)) {
    // 1. [[wikilink|label]]
    for (const match of segment.matchAll(/(?<!!)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g)) {
      addLink(match[1], match[2]);
    }

    // 2. [label](/p/target) or [label](target.md)
    for (const match of segment.matchAll(/(?<!!)\[([^\]]+)\]\(([^)]+)\)/g)) {
      addLink(match[2], match[1]);
    }
  }

  return results;
}
