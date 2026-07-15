use super::*;

// Redirect root "/" to "/p/Index"
pub(super) async fn redirect_to_index() -> impl IntoResponse {
    Redirect::temporary("/p/Index")
}

// Server-Sent Events stream of re-indexed page paths. One-way server->client:
// Optional Server-Sent Events stream of re-indexed page paths. Reader mode uses
// low-frequency conditional API checks instead, so normal reading never holds
// this connection open. The handler only subscribes to the broadcast channel;
// it never writes the Postgres index.
pub(super) async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<sse::Event, std::convert::Infallible>>> {
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| {
        // Drop Lagged errors gracefully: the client refetches on the next event.
        item.ok().map(|path| Ok(sse::Event::default().data(path)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Helper to get safe path under miku_docs/ and check for directory traversal
pub(super) fn safe_file_path(path: &str) -> Result<PathBuf, AppError> {
    if path.contains("..") || path.starts_with('/') {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "Invalid path: path traversal detected"
        )));
    }
    Ok(StdPath::new("miku_docs").join(format!("{}.md", reader_page_path(path))))
}

pub(super) fn validate_folder_path(path: &str) -> Result<String, AppError> {
    let trimmed = path.trim_matches('/');
    if trimmed.contains("..") || path.starts_with('/') {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "Invalid folder path: path traversal detected"
        )));
    }
    Ok(trimmed.to_string())
}

// Helper to compute SHA-256 hash of content
pub(super) fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn format_modified_time(file_path: &StdPath) -> String {
    fs::metadata(file_path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(|modified| {
            let local: DateTime<Local> = modified.into();
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(super) fn first_plain_mention_range(body: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let needle_len = needle.len();

    // Search the ORIGINAL body so the returned offsets are always valid char
    // boundaries. We compare with `eq_ignore_ascii_case` (case-insensitive for
    // ASCII, exact bytes otherwise — correct for caseless scripts like CJK);
    // this keeps the match the same byte length as the needle, avoiding the
    // offset drift you'd get from indexing a `to_lowercase()` copy.
    for (start, _) in body.char_indices() {
        let end = start + needle_len;
        if end > body.len() || !body.is_char_boundary(end) {
            continue;
        }
        if !body[start..end].eq_ignore_ascii_case(needle) {
            continue;
        }
        let before = body[..start].chars().rev().take(2).collect::<String>();
        let after = body[end..].chars().take(2).collect::<String>();
        let starts_link = before.chars().rev().collect::<String>() == "[[";
        let ends_link = after == "]]";
        if !starts_link && !ends_link {
            return Some((start, end));
        }
    }

    None
}

pub(super) fn promote_first_plain_mention(
    raw: &str,
    mention: &str,
    target: &str,
) -> Option<String> {
    let (frontmatter, body) = parse_frontmatter(raw);
    let (start, end) = first_plain_mention_range(body, mention)?;
    let mut promoted_body = String::new();
    promoted_body.push_str(&body[..start]);
    promoted_body.push_str("[[");
    promoted_body.push_str(target);
    promoted_body.push('|');
    promoted_body.push_str(&body[start..end]);
    promoted_body.push_str("]]");
    promoted_body.push_str(&body[end..]);

    if frontmatter.is_some() {
        raw.split_once("---\n")
            .and_then(|(_, rest)| rest.split_once("---\n"))
            .map(|(yaml, _)| format!("---\n{yaml}---\n{promoted_body}"))
    } else {
        Some(promoted_body)
    }
}

pub(super) fn breadcrumb_parent(path: &str) -> Option<String> {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .filter(|parent| !parent.is_empty())
}

pub(super) fn breadcrumb_items(path: &str, title: &str) -> Vec<BreadcrumbItem> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let mut items = Vec::new();
    let mut current_path = String::new();

    for (index, part) in parts.iter().enumerate() {
        if !current_path.is_empty() {
            current_path.push('/');
        }
        current_path.push_str(part);
        let current = index + 1 == parts.len();
        items.push(BreadcrumbItem {
            label: if current {
                title.to_string()
            } else {
                (*part).to_string()
            },
            path: current_path.clone(),
            current,
        });
    }

    items
}

// Helper pub(super) struct for building nav tree (internal use only)
#[derive(Debug)]
pub(super) struct TreeNode {
    title: String,
    stem: String,
    children: std::collections::BTreeMap<String, TreeNode>,
    is_leaf: bool,
}

pub(super) fn file_browser_sort_key(stem: &str, is_leaf: bool) -> String {
    let normalized = stem.to_lowercase();
    if is_leaf && matches!(normalized.as_str(), "readme" | "index") {
        format!("!{normalized}")
    } else {
        normalized
    }
}

// Convert TreeNode BTreeMap tree into Vec<NavNode> with file-browser sorting.
// Folders come first, then pages; both groups order by path segment/stem.
pub(super) fn tree_to_nav_nodes(
    tree: std::collections::BTreeMap<String, TreeNode>,
    prefix: String,
) -> Vec<NavNode> {
    let mut folders = Vec::new();
    let mut pages = Vec::new();

    for (name, node) in tree {
        let current_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        let children = tree_to_nav_nodes(node.children, current_path.clone());

        if node.is_leaf {
            pages.push(NavNode {
                name: node.title.clone(),
                path: Some(current_path.clone()),
                stem: node.stem.clone(),
                sort_key: file_browser_sort_key(&node.stem, true),
                children,
            });
        } else {
            folders.push(NavNode {
                name: node.title.clone(),
                path: None,
                stem: node.stem.clone(),
                sort_key: file_browser_sort_key(&node.stem, false),
                children,
            });
        }
    }

    // Sort like a file browser: folders first, then files, each by path segment
    // rather than page title. README/index stay near the top of their folder.
    folders.sort_by_key(|a| a.sort_key.clone());
    pages.sort_by_key(|a| a.sort_key.clone());

    let mut result = folders;
    result.extend(pages);
    result
}

// Build a nested tree structure from page rows (path_without_md, title).
// Pure function, no DB, no async. Folders come first (sorted alphabetically),
// then pages (sorted alphabetically by name). Each row's path is like "a" or
// "b/c" or "b/d/e" (no .md). The final segment is a page leaf with path =
// Some(full path) and name = title; intermediate segments are folders with
// path = None.
pub(super) fn build_nav_tree(rows: Vec<(String, String)>) -> Vec<NavNode> {
    use std::collections::BTreeMap;

    let mut root: BTreeMap<String, TreeNode> = BTreeMap::new();

    for (path, title) in rows {
        let parts: Vec<&str> = path.split('/').collect();

        // Navigate/create the tree structure
        let mut current = &mut root;
        for (i, &part) in parts.iter().enumerate() {
            let is_final = i == parts.len() - 1;

            if !current.contains_key(part) {
                current.insert(
                    part.to_string(),
                    TreeNode {
                        title: if is_final {
                            title.clone()
                        } else {
                            part.to_string()
                        },
                        stem: part.to_string(),
                        children: BTreeMap::new(),
                        is_leaf: is_final,
                    },
                );
            }

            current = &mut current.get_mut(part).expect("just inserted").children;
        }
    }

    tree_to_nav_nodes(root, String::new())
}

// Prune a built tree for lazy rendering: keep folder children only along the
// active page's ancestor chain; every other folder is emptied so the template
// emits a collapsed stub that lazy-loads via /api/v1/nav/children on first expand.
// This keeps the page payload to root level + the open page's path, not O(N).
pub(super) fn prune_nav_tree(nodes: &mut [NavNode], active: &str, prefix: &str) {
    for node in nodes.iter_mut() {
        if node.path.is_some() {
            continue; // leaf page, no children to prune
        }
        let folder_path = if prefix.is_empty() {
            node.name.clone()
        } else {
            format!("{prefix}/{}", node.name)
        };
        let is_ancestor = active == folder_path || active.starts_with(&format!("{folder_path}/"));
        if is_ancestor {
            prune_nav_tree(&mut node.children, active, &folder_path);
        } else {
            node.children.clear();
        }
    }
}

// Descend a built tree to the direct children of `dir` (slash-separated folder
// path). Returns an empty vec if the folder is absent.
pub(super) fn nav_folder_children(nodes: Vec<NavNode>, dir: &str) -> Vec<NavNode> {
    let mut current = nodes;
    for seg in dir.split('/') {
        match current
            .into_iter()
            .find(|n| n.path.is_none() && n.name == seg)
        {
            Some(folder) => current = folder.children,
            None => return Vec::new(),
        }
    }
    current
}

// Sidebar nav: every page in the index, title-sorted, for the explorer list
// rendered by base.html. The index is the disposable read model; a freshly
// saved page appears once the background indexer catches up.
pub(super) async fn nav_pages(index: &IndexApi, active: &str) -> Result<Vec<NavNode>, AppError> {
    let rows = index
        .list_pages()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load nav pages")?;
    let stripped_rows: Vec<(String, String)> = rows
        .into_iter()
        .map(|page| {
            (
                page.path
                    .strip_suffix(".md")
                    .unwrap_or(&page.path)
                    .to_string(),
                page.title,
            )
        })
        .collect();
    let mut tree = build_nav_tree(stripped_rows);
    // Render only the root level plus the active page's ancestor folders; all
    // other folders lazy-load on expand. Avoids serializing the whole vault.
    prune_nav_tree(&mut tree, active, "");
    Ok(tree)
}

// GET /api/v1/nav/children?dir=<folder> — htmx partial: the direct children of one
// folder, each subfolder itself collapsed/lazy. Lets the sidebar expand folders
// on demand instead of rendering the entire tree up front.
#[derive(serde::Deserialize)]
pub(super) struct NavChildrenQuery {
    dir: Option<String>,
}

pub(super) async fn nav_children_handler(
    Query(params): Query<NavChildrenQuery>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let started = Instant::now();
    let dir = params.dir.unwrap_or_default();
    let db_started = Instant::now();
    let rows = state
        .index
        .list_pages()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load nav children")?;
    let db_ms = timing_ms(db_started);
    let stripped_rows: Vec<(String, String)> = rows
        .into_iter()
        .map(|page| {
            (
                page.path
                    .strip_suffix(".md")
                    .unwrap_or(&page.path)
                    .to_string(),
                page.title,
            )
        })
        .collect();
    let tree = build_nav_tree(stripped_rows);
    let mut nodes = if dir.is_empty() {
        tree
    } else {
        nav_folder_children(tree, &dir)
    };
    // Show one level; grandchildren stay lazy (active="" => nothing pre-expands).
    prune_nav_tree(&mut nodes, "", &dir);

    let template = state.templates.get_template("nav_children.html")?;
    let rendered = template.render(context! { nodes => nodes, prefix => dir.clone() })?;
    let total_ms = timing_ms(started);
    info!(
        dir = %dir,
        db_ms,
        total_ms,
        "nav_children rendered"
    );
    let mut response = Html(rendered).into_response();
    attach_server_timing(&mut response, "nav_children", total_ms);
    Ok(response)
}

pub(super) async fn folder_children(
    index: &IndexApi,
    folder_path: &str,
) -> Result<(Vec<FolderChild>, Vec<FolderPage>), AppError> {
    let prefix = if folder_path.is_empty() {
        String::new()
    } else {
        format!("{folder_path}/")
    };
    let rows = index
        .list_pages()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load folder children")?;

    let mut folders = std::collections::BTreeMap::<String, String>::new();
    let mut pages = Vec::new();

    for page in rows {
        let page_path = page.path.strip_suffix(".md").unwrap_or(&page.path);
        let Some(rest) = page_path.strip_prefix(&prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        if let Some((folder, _)) = rest.split_once('/') {
            folders
                .entry(folder.to_string())
                .or_insert_with(|| format!("{prefix}{folder}"));
        } else {
            pages.push(FolderPage {
                title: page.title,
                path: page_path.to_string(),
            });
        }
    }

    pages.sort_by_key(|page| {
        page.path
            .rsplit('/')
            .next()
            .map(|stem| file_browser_sort_key(stem, true))
            .unwrap_or_default()
    });
    Ok((
        folders
            .into_iter()
            .map(|(name, path)| FolderChild { name, path })
            .collect(),
        pages,
    ))
}

pub(super) async fn folder_view(
    Path(path): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let started = Instant::now();
    let path = validate_folder_path(&path)?;
    let template = state.templates.get_template("folder.html")?;
    let db_started = Instant::now();
    let nav = nav_pages(&state.index, &path).await?;
    let (folders, pages) = folder_children(&state.index, &path).await?;
    let db_ms = timing_ms(db_started);
    let folder_count = folders.len();
    let page_count = pages.len();
    let title = path
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("Files")
        .to_string();

    let rendered = template.render(context! {
        title => title,
        path => path,
        folders => folders,
        pages => pages,
        nav_pages => nav,
        breadcrumbs => breadcrumb_items(&path, &title),
    })?;
    let total_ms = timing_ms(started);
    info!(
        path = %path,
        folders = folder_count,
        pages = page_count,
        db_ms,
        total_ms,
        "folder_view rendered"
    );
    let mut response = Html(rendered).into_response();
    attach_server_timing(&mut response, "folder_view", total_ms);
    Ok(response)
}

// Dispatch to view or edit based on the path suffix
pub(super) async fn page_handler(
    Path(path): Path<String>,
    Query(params): Query<EditQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(stripped_path) = path.strip_suffix("/edit") {
        page_edit(reader_page_path(stripped_path), params.template, state).await
    } else {
        page_view(reader_page_path(&path), state).await
    }
}

// Optional `?template=<id>` from the create-page modal, used to seed a brand-new
// page's editor body. Ignored for existing pages.
#[derive(serde::Deserialize)]
pub(super) struct EditQuery {
    template: Option<String>,
}

// Seed bodies for the create-page modal's "start from" templates. The server is
// the single source of truth for this content (the modal only passes the id),
// so a freshly created page opens prefilled without a client-side markdown lib.
pub(super) fn template_seed(id: &str) -> &'static str {
    match id {
        "meeting" => "# Meeting\n\n## Agenda\n\n## Notes\n\n## Actions\n",
        "reading" => "# Reading Notes\n\n## Summary\n\n## Highlights\n\n## Questions\n",
        "project" => "# Project\n\n## Goal\n\n## Tasks\n\n## Status\n",
        _ => "",
    }
}

pub(super) async fn load_slug_map(
    index: &IndexApi,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let pages = index
        .list_pages()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load pages for wikilink resolution")?;

    let mut slug_map = std::collections::HashMap::new();
    for page in pages {
        let path_without_md = page
            .path
            .strip_suffix(".md")
            .unwrap_or(&page.path)
            .to_string();
        let slug = std::path::Path::new(&path_without_md)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(&path_without_md);
        slug_map.insert(slug.to_lowercase(), path_without_md.clone());
        slug_map.insert(path_without_md.to_lowercase(), path_without_md.clone());
        if let Some(aliases) = page
            .frontmatter
            .get("aliases")
            .and_then(|value| value.as_array())
        {
            for alias in aliases.iter().filter_map(|value| value.as_str()) {
                slug_map.insert(alias.to_lowercase(), path_without_md.clone());
            }
        }
    }
    Ok(slug_map)
}

pub(super) async fn reader_page_payload(
    path: &str,
    state: &AppState,
) -> Result<ReaderPagePayload, AppError> {
    let path = reader_page_path(path);
    let file_path = safe_file_path(&path)?;
    if !file_path.exists() {
        let title = format!("Create Page: {path}");
        let mut payload = ReaderPagePayload {
            path: path.clone(),
            title: title.clone(),
            exists: false,
            html: String::new(),
            content_html: String::new(),
            toc: Vec::new(),
            backlinks: Vec::new(),
            unlinked_mentions: Vec::new(),
            word_count: 0,
            backlink_count: 0,
            updated: "Missing".to_string(),
            frontmatter: serde_json::Value::Object(serde_json::Map::new()),
            breadcrumbs: breadcrumb_items(&path, &title),
        };
        payload.html = render_reader_fragment(state, &payload)?;
        return Ok(payload);
    }

    let raw_content = fs::read_to_string(&file_path)
        .context(format!("Failed to read file: {}", file_path.display()))?;
    let (frontmatter, body) = parse_frontmatter(&raw_content);
    let title = extract_title(&path, frontmatter.as_ref(), body);
    let word_count = body.split_whitespace().count();
    let updated = format_modified_time(&file_path);
    let slug_map = load_slug_map(&state.index).await?;
    let (content_html, toc) = render_html_with_toc(body, &|norm| slug_map.get(norm).cloned());
    let backlinks = state
        .index
        .backlinks(&path)
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load backlinks")?
        .into_iter()
        .map(|backlink| Backlink {
            path: reader_page_path(&backlink.path),
            title: backlink.title,
        })
        .collect::<Vec<_>>();
    let unlinked_mentions = state
        .index
        .mentions_for_target(&path)
        .await
        .map_err(|error| anyhow::anyhow!(error))?
        .into_iter()
        .map(|mention| UnlinkedMention {
            path: mention
                .source_path
                .strip_suffix(".md")
                .unwrap_or(&mention.source_path)
                .to_string(),
            title: mention.source_title,
            snippet: mention.snippet,
        })
        .collect();
    let frontmatter =
        frontmatter.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    let mut payload = ReaderPagePayload {
        path: path.clone(),
        title: title.clone(),
        exists: true,
        html: String::new(),
        content_html,
        toc,
        backlink_count: backlinks.len(),
        backlinks,
        unlinked_mentions,
        word_count,
        updated,
        frontmatter,
        breadcrumbs: breadcrumb_items(&path, &title),
    };
    payload.html = render_reader_fragment(state, &payload)?;
    Ok(payload)
}

pub(super) async fn reader_page_api(
    Path(path): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let started = Instant::now();
    let canonical_path = reader_page_path(&path);
    let payload = reader_page_payload(&canonical_path, &state).await?;
    let total_ms = timing_ms(started);
    info!(path = %canonical_path, total_ms, "reader page API rendered");
    let mut response = Json(payload).into_response();
    attach_server_timing(&mut response, "reader_page", total_ms);
    Ok(response)
}

// Render the read-only page view
pub(super) async fn page_view(path: String, state: AppState) -> Result<Response, AppError> {
    let started = Instant::now();
    info!("Rendering page view for path: {}", path);
    let file_path = safe_file_path(&path)?;
    let template = state.templates.get_template("page.html")?;
    let nav_started = Instant::now();
    let nav = nav_pages(&state.index, &path).await?;
    let nav_ms = timing_ms(nav_started);

    if !file_path.exists() {
        let title = format!("Create Page: {path}");
        let rendered = template.render(context! {
            title => title,
            path => path,
            exists => false,
            content_html => "",
            body => "",
            loaded_hash => "",
            has_mermaid => false,
            backlinks => Vec::<Backlink>::new(),
            unlinked_mentions => Vec::<UnlinkedMention>::new(),
            toc => Vec::<Heading>::new(),
            word_count => 0usize,
            backlink_count => 0usize,
            updated => "Missing",
            frontmatter => serde_json::Value::Object(serde_json::Map::new()),
            breadcrumb_parent => breadcrumb_parent(&path),
            nav_pages => nav,
            breadcrumbs => breadcrumb_items(&path, &title),
        })?;
        let total_ms = timing_ms(started);
        info!(path = %path, exists = false, nav_ms, total_ms, "page_view rendered");
        let mut response = Html(rendered).into_response();
        attach_server_timing(&mut response, "page_view", total_ms);
        return Ok(response);
    }

    let file_started = Instant::now();
    let raw_content = fs::read_to_string(&file_path)
        .context(format!("Failed to read file: {}", file_path.display()))?;
    let file_ms = timing_ms(file_started);
    let loaded_hash = compute_hash(&raw_content);
    let (frontmatter, body) = parse_frontmatter(&raw_content);
    let title = extract_title(&path, frontmatter.as_ref(), body);
    let word_count = body.split_whitespace().count();
    let updated = format_modified_time(&file_path);

    // Resolve wikilink targets against the index so missing pages render
    // distinctly. The index is a disposable read model; a freshly saved page
    // may briefly resolve as missing until the background indexer catches up.
    let render_started = Instant::now();
    let slug_map = load_slug_map(&state.index).await?;
    let (content_html, toc) = render_html_with_toc(body, &|norm| slug_map.get(norm).cloned());
    let markdown_ms = timing_ms(render_started);

    // Check has_mermaid
    let has_mermaid = raw_content.contains("```mermaid");

    // Load backlinks: pages that link TO this page
    let backlinks_started = Instant::now();
    let backlinks = state
        .index
        .backlinks(&path)
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load backlinks")?;
    let backlink_count = backlinks.len();
    let backlinks = backlinks
        .into_iter()
        .map(|backlink| Backlink {
            path: reader_page_path(&backlink.path),
            title: backlink.title,
        })
        .collect::<Vec<_>>();
    let backlinks_ms = timing_ms(backlinks_started);
    let mentions_started = Instant::now();
    let unlinked_mentions = state
        .index
        .mentions_for_target(&path)
        .await
        .map_err(|error| anyhow::anyhow!(error))?
        .into_iter()
        .map(|mention| UnlinkedMention {
            path: mention
                .source_path
                .strip_suffix(".md")
                .unwrap_or(&mention.source_path)
                .to_string(),
            title: mention.source_title,
            snippet: mention.snippet,
        })
        .collect::<Vec<_>>();
    let mentions_ms = timing_ms(mentions_started);
    let unlinked_mention_count = unlinked_mentions.len();
    let frontmatter =
        frontmatter.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    let rendered = template.render(context! {
        title => title,
        path => path,
        exists => true,
        content_html => content_html,
        body => raw_content,
        loaded_hash => loaded_hash,
        has_mermaid => has_mermaid,
        backlinks => backlinks,
        unlinked_mentions => unlinked_mentions,
        toc => toc,
        word_count => word_count,
        backlink_count => backlink_count,
        updated => updated,
        frontmatter => frontmatter,
        breadcrumb_parent => breadcrumb_parent(&path),
        nav_pages => nav,
        breadcrumbs => breadcrumb_items(&path, &title),
    })?;

    let total_ms = timing_ms(started);
    info!(
        path = %path,
        word_count,
        backlink_count,
        unlinked_mentions = unlinked_mention_count,
        nav_ms,
        file_ms,
        markdown_ms,
        backlinks_ms,
        mentions_ms,
        total_ms,
        "page_view rendered"
    );
    let mut response = Html(rendered).into_response();
    attach_server_timing(&mut response, "page_view", total_ms);
    Ok(response)
}

// Render the edit page
pub(super) async fn page_edit(
    path: String,
    template_id: Option<String>,
    state: AppState,
) -> Result<Response, AppError> {
    info!("Rendering edit page for path: {}", path);
    let file_path = safe_file_path(&path)?;
    let template = state.templates.get_template("edit.html")?;

    let (body, loaded_hash) = if file_path.exists() {
        let raw_content = fs::read_to_string(&file_path)
            .context(format!("Failed to read file: {}", file_path.display()))?;
        let hash = compute_hash(&raw_content);
        (raw_content, hash)
    } else {
        // New page: seed the editor from the chosen create-modal template (if
        // any). loaded_hash stays empty so the save path treats it as a create.
        let seed = template_id.as_deref().map(template_seed).unwrap_or("");
        (seed.to_string(), String::new())
    };

    let nav = nav_pages(&state.index, &path).await?;
    let rendered = template.render(context! {
        path => path,
        body => body,
        loaded_hash => loaded_hash,
        nav_pages => nav,
    })?;

    Ok(Html(rendered).into_response())
}

#[derive(serde::Deserialize)]
pub(super) struct EditForm {
    body: String,
    loaded_hash: String,
}

#[derive(serde::Deserialize)]
pub(super) struct PreviewForm {
    body: String,
}

pub(super) async fn preview(
    State(state): State<AppState>,
    Form(form): Form<PreviewForm>,
) -> Result<impl IntoResponse, AppError> {
    let slug_map = load_slug_map(&state.index).await?;
    let (_, body) = parse_frontmatter(&form.body);
    let (content_html, _) = render_html_with_toc(body, &|norm| slug_map.get(norm).cloned());

    Ok(Html(content_html))
}

// Handle the saving of a page
pub(super) async fn page_save(
    Path(path): Path<String>,
    State(state): State<AppState>,
    Form(form): Form<EditForm>,
) -> Result<Response, AppError> {
    info!("Saving page path: {}", path);
    let file_path = safe_file_path(&path)?;

    // If file exists, do optimistic concurrency check
    if file_path.exists() {
        let disk_content = fs::read_to_string(&file_path).context(format!(
            "Failed to read file for hash check: {}",
            file_path.display()
        ))?;
        let disk_hash = compute_hash(&disk_content);

        if disk_hash != form.loaded_hash {
            warn!("Conflict detected on page save: path={}", path);
            let template = state.templates.get_template("conflict.html")?;
            let nav = nav_pages(&state.index, &path).await?;
            let rendered = template.render(context! {
                path => path,
                current_content => disk_content,
                submitted_content => form.body,
                current_hash => disk_hash,
                nav_pages => nav,
            })?;
            return Ok((StatusCode::CONFLICT, Html(rendered)).into_response());
        }
    }

    // Atomic write
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).context(format!(
            "Failed to create parent directories: {}",
            parent.display()
        ))?;
    }

    let temp_path = file_path.with_extension("tmp");
    {
        let mut file = fs::File::create(&temp_path).context(format!(
            "Failed to create temp file: {}",
            temp_path.display()
        ))?;
        file.write_all(form.body.as_bytes())
            .context("Failed to write to temp file")?;
        file.sync_all()
            .context("Failed to sync temp file to disk")?;
    }

    fs::rename(&temp_path, &file_path).context(format!(
        "Failed to rename temp file to target: {}",
        file_path.display()
    ))?;

    info!("Saved page path={} successfully", path);
    Ok(Redirect::to(&format!("/p/{path}")).into_response())
}

#[derive(serde::Deserialize)]
pub(super) struct PromoteMentionForm {
    source_path: String,
    target_path: String,
    mention: String,
    return_to: String,
}

pub(super) async fn promote_mention(
    Form(form): Form<PromoteMentionForm>,
) -> Result<Response, AppError> {
    let source = safe_file_path(&form.source_path)?;
    let raw = fs::read_to_string(&source).context(format!(
        "Failed to read source page for mention promotion: {}",
        source.display()
    ))?;
    let Some(promoted) = promote_first_plain_mention(&raw, &form.mention, &form.target_path) else {
        return Ok(Redirect::to(&format!("/p/{}", form.return_to)).into_response());
    };

    let temp_path = source.with_extension("tmp");
    {
        let mut file = fs::File::create(&temp_path).context(format!(
            "Failed to create temp file: {}",
            temp_path.display()
        ))?;
        file.write_all(promoted.as_bytes())
            .context("Failed to write promoted mention to temp file")?;
        file.sync_all()
            .context("Failed to sync promoted mention temp file")?;
    }

    fs::rename(&temp_path, &source).context(format!(
        "Failed to replace source page after mention promotion: {}",
        source.display()
    ))?;

    Ok(Redirect::to(&format!("/p/{}", form.return_to)).into_response())
}

// Search handler: the full Markdown content-search page. Body search itself is
// performed by the embedded ripgrep implementation in content_search.rs.
#[derive(serde::Deserialize)]
pub(super) struct SearchParams {
    q: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct ContentSearchParams {
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    regex: Option<bool>,
}

pub(super) async fn quickswitch(
    Query(params): Query<SearchParams>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let started = Instant::now();
    let query = params.q.as_deref().unwrap_or("").trim();
    let mut pages = state
        .index
        .list_pages()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load quickswitch pages")?;
    if !query.is_empty() {
        let needle = query.to_ascii_lowercase();
        pages.retain(|page| {
            page.title.to_ascii_lowercase().contains(&needle)
                || page.path.to_ascii_lowercase().contains(&needle)
        });
        pages.sort_by_key(|page| {
            let title = page.title.to_ascii_lowercase();
            let path = page.path.to_ascii_lowercase();
            if title == needle {
                0
            } else if title.starts_with(&needle) {
                1
            } else if title.contains(&needle) {
                2
            } else if path.starts_with(&needle) {
                3
            } else {
                4
            }
        });
    }
    let rows: Vec<(String, String, String)> = pages
        .into_iter()
        .take(20)
        .map(|page| (page.path, page.title, String::new()))
        .collect();

    let result_count = rows.len();
    let total_ms = timing_ms(started);
    info!(query, result_count, total_ms, "quickswitch searched");
    let mut response = Json(
        rows.into_iter()
            .map(|(path, title, snippet)| QuickSwitchResult {
                path: path.strip_suffix(".md").unwrap_or(&path).to_string(),
                title,
                snippet,
            })
            .collect::<Vec<_>>(),
    )
    .into_response();
    attach_server_timing(&mut response, "quickswitch", total_ms);
    Ok(response)
}

pub(super) async fn content_search_api(
    Query(params): Query<ContentSearchParams>,
) -> Result<Json<miku::content_search::ContentSearchPage>, AppError> {
    let query = params.q.unwrap_or_default();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(10);
    let regex = params.regex.unwrap_or(false);
    let result = tokio::task::spawn_blocking(move || {
        miku::content_search::search(StdPath::new("miku_docs"), &query, offset, limit, regex)
    })
    .await
    .map_err(|error| anyhow::anyhow!(error))?
    .map_err(|error| anyhow::anyhow!(error))
    .context("Failed to search Markdown content")?;
    Ok(Json(result))
}

pub(super) async fn search(
    Query(params): Query<SearchParams>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let started = Instant::now();
    let template = state.templates.get_template("search.html")?;

    let query_str = params.q.as_deref().unwrap_or("").trim().to_string();

    let nav = nav_pages(&state.index, "").await?;
    let rendered = template.render(context! {
        query => query_str,
        nav_pages => nav,
        section => "search",
    })?;

    let total_ms = timing_ms(started);
    info!(total_ms, "content search page rendered");
    let mut response = Html(rendered).into_response();
    attach_server_timing(&mut response, "search", total_ms);
    Ok(response)
}

// Tags are a secondary sidebar surface. Keep them off the page render path and
// fetch them only when the user opens the Tags tab.
pub(super) async fn tags_api(
    Query(params): Query<TagsQuery>,
    State(state): State<AppState>,
) -> Result<Json<TagsPage>, AppError> {
    let tags = state
        .index
        .tags()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load tags")?
        .into_iter()
        .map(|tag| TagCount {
            tag: tag.tag,
            count: tag.count,
        })
        .collect::<Vec<_>>();
    Ok(Json(paginate_tags(
        tags,
        params.offset.unwrap_or(0),
        params.limit.unwrap_or(TAG_PAGE_SIZE),
    )))
}

// Tags index handler: list all tags with their counts
pub(super) async fn tags_index(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("Rendering tags index");
    let template = state.templates.get_template("tags.html")?;

    let all_tags: Vec<TagCount> = state
        .index
        .tags()
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load tags")?
        .into_iter()
        .map(|tag| TagCount {
            tag: tag.tag,
            count: tag.count,
        })
        .collect();

    let tags_page = paginate_tags(all_tags, 0, TAG_PAGE_SIZE);
    let nav = nav_pages(&state.index, "").await?;
    let rendered = template.render(context! {
        tags => tags_page.tags,
        total_tags => tags_page.total,
        has_more => tags_page.has_more,
        next_offset => tags_page.next_offset,
        tag_page_size => tags_page.limit,
        nav_pages => nav,
        section => "tags",
    })?;

    Ok(Html(rendered).into_response())
}

// Tag filter handler: list all pages with a specific tag
pub(super) async fn tag_filter(
    Path(tag): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("Rendering tag filter for tag: {}", tag);
    let template = state.templates.get_template("tag.html")?;

    let pages_page =
        paginate_page_refs(pages_for_tag(&state, &tag).await?, 0, TAG_RESULT_PAGE_SIZE);

    let nav = nav_pages(&state.index, "").await?;
    let rendered = template.render(context! {
        tag => tag,
        pages => pages_page.pages,
        total_pages => pages_page.total,
        has_more => pages_page.has_more,
        next_offset => pages_page.next_offset,
        page_size => pages_page.limit,
        nav_pages => nav,
        section => "tags",
    })?;

    Ok(Html(rendered).into_response())
}

pub(super) async fn tag_pages_api(
    Path(tag): Path<String>,
    Query(params): Query<TagsQuery>,
    State(state): State<AppState>,
) -> Result<Json<TagPagesPage>, AppError> {
    let pages = pages_for_tag(&state, &tag).await?;
    Ok(Json(paginate_page_refs(
        pages,
        params.offset.unwrap_or(0),
        params.limit.unwrap_or(TAG_RESULT_PAGE_SIZE),
    )))
}
