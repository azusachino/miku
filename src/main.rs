use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{header::HeaderName, HeaderValue, StatusCode},
    response::{
        sse::{self, KeepAlive, Sse},
        Html, IntoResponse, Redirect, Response,
    },
    routing::{get, post},
    Form, Json, Router,
};
use chrono::{DateTime, Local};
use miku::markdown::{extract_title, parse_frontmatter, render_html_with_toc, Heading};
use miku_app::{compose_index, IndexApi, IndexConfig};
use minijinja::{context, Environment};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{info, warn};

const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");

#[derive(serde::Serialize)]
struct Backlink {
    path: String,
    title: String,
}

#[derive(serde::Serialize)]
struct UnlinkedMention {
    path: String,
    title: String,
    snippet: String,
}

#[derive(serde::Serialize)]
struct TagCount {
    tag: String,
    count: i64,
}

#[derive(serde::Serialize)]
struct PageRef {
    path: String,
    title: String,
}

#[derive(serde::Serialize)]
struct SearchResult {
    path: String,
    title: String,
    snippet: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchScope {
    All,
    Title,
    Body,
}

impl SearchScope {
    fn from_param(scope: Option<&str>) -> Self {
        match scope {
            Some("title") => Self::Title,
            Some("body") => Self::Body,
            _ => Self::All,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Title => "title",
            Self::Body => "body",
        }
    }
}

#[derive(serde::Serialize)]
struct QuickSwitchResult {
    path: String,
    title: String,
}

#[derive(serde::Serialize)]
struct BreadcrumbItem {
    label: String,
    path: String,
    current: bool,
}

#[derive(serde::Serialize)]
struct FolderChild {
    name: String,
    path: String,
}

#[derive(serde::Serialize)]
struct FolderPage {
    title: String,
    path: String,
}

fn search_snippet(body: &str, query: &str) -> String {
    let plain = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if plain.is_empty() {
        return "No preview available.".to_string();
    }

    let needle = query
        .split_whitespace()
        .find(|word| word.chars().any(char::is_alphanumeric))
        .unwrap_or(query)
        .to_ascii_lowercase();
    let plain_lower = plain.to_ascii_lowercase();
    let match_at = plain_lower.find(&needle).unwrap_or(0);
    let start = plain[..match_at]
        .rfind(' ')
        .and_then(|idx| plain[..idx].rfind(' '))
        .and_then(|idx| plain[..idx].rfind(' '))
        .unwrap_or(0);
    let end = plain[match_at..]
        .char_indices()
        .filter(|(_, ch)| ch.is_whitespace())
        .nth(30)
        .map(|(idx, _)| match_at + idx)
        .unwrap_or(plain.len());
    let snippet = plain[start..end].trim();

    if end < plain.len() {
        format!("{snippet}...")
    } else {
        snippet.to_string()
    }
}

fn timing_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn timing_header_value(name: &str, elapsed_ms: f64) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!("{name};dur={elapsed_ms:.1}")).ok()
}

fn attach_server_timing(response: &mut Response, name: &str, elapsed_ms: f64) {
    if let Some(value) = timing_header_value(name, elapsed_ms) {
        response.headers_mut().insert(SERVER_TIMING, value);
    }
}

#[derive(serde::Serialize)]
struct NavNode {
    name: String,         // folder segment name, or page title for leaves
    path: Option<String>, // Some(slug-path without .md) for pages; None for folders
    stem: String,         // folder segment or filename stem; used for file-browser sorting
    sort_key: String,
    children: Vec<NavNode>,
}

#[derive(Clone)]
struct AppState {
    index: IndexApi,
    templates: Arc<Environment<'static>>,
    // Broadcasts the relative path (`.md` stripped) of each page the background
    // indexer just re-indexed, so connected browsers can live-refresh via SSE.
    // Read-only fan-out: the SSE layer never writes the Postgres index.
    events: tokio::sync::broadcast::Sender<String>,
}

// Custom error handling for Axum route handlers
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        warn!("Handler error: {:?}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize tracing with an env filter
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,miku=debug,tower_http=debug")
            }),
        )
        .init();

    info!("Starting Miku Server...");

    // 2. Load DATABASE_URL from environment variable
    let database_url =
        env::var("DATABASE_URL").context("DATABASE_URL environment variable must be set")?;

    // 3. Connect to PostgreSQL database using sqlx PgPool
    info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    // 4. Run migrations on startup
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Database migrations failed")?;
    info!("Migrations complete.");

    let index = compose_index(IndexConfig::Postgres {
        database_url: database_url.clone(),
    })
    .await
    .map_err(|error| anyhow::anyhow!(error))
    .context("Failed to compose Postgres index API")?;

    // 5. Initialize Minijinja template environment
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    // SSE broadcast channel: the indexer is the sole sender; each browser /events
    // connection is a subscriber. Capacity 256 bounds backpressure; slow
    // subscribers see Lagged and resync on the next event.
    let (events_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    let state = AppState {
        index,
        templates: Arc::new(templates_env),
        events: events_tx.clone(),
    };

    // 6. Initialize background indexer
    let indexer =
        miku::indexer::IndexerQueue::new(pool, std::path::PathBuf::from("miku_docs"), events_tx)
            .context("Failed to initialize background indexer")?;

    // 7. Build Router & Configure axum routes
    let app = app(state);

    // 8. Bind and run the listener. Defaults to 0.0.0.0:3000 so the server is
    //    reachable from other hosts on the LAN / Tailscale tailnet (visit
    //    http://<tailscale-ip-or-magicdns>:3000), not just localhost. Override
    //    with MIKU_BIND, e.g. MIKU_BIND=127.0.0.1:3000 to restrict to local.
    let addr: SocketAddr = env::var("MIKU_BIND")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()
        .context("MIKU_BIND must be a valid socket address, e.g. 0.0.0.0:3000")?;
    info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    indexer.shutdown().await;

    Ok(())
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(redirect_to_index))
        .route("/search", get(search))
        .route("/tags", get(tags_index))
        .route("/tags/{tag}", get(tag_filter))
        .route("/folders/{*path}", get(folder_view))
        .route("/preview", post(preview))
        .route("/p/{*path}", get(page_handler).post(page_save))
        .route("/events", get(events))
        .route("/api/move", post(page_move))
        .route("/api/trash", post(page_trash).get(trash_list))
        .route("/api/trash/restore", post(trash_restore))
        .route("/api/trash/purge", post(trash_purge))
        .route("/api/promote-mention", post(promote_mention))
        .route("/api/nav/children", get(nav_children_handler))
        .route("/api/quickswitch", get(quickswitch))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/assets", ServeDir::new("miku_docs/assets"))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// Redirect root "/" to "/p/Index"
async fn redirect_to_index() -> impl IntoResponse {
    Redirect::temporary("/p/Index")
}

// Server-Sent Events stream of re-indexed page paths. One-way server->client:
// the browser opens `new EventSource('/events')` and live-refreshes the open
// page when it sees its own path. This handler only SUBSCRIBES to the broadcast
// channel filled by the background indexer; it never writes the Postgres index,
// preserving the single-writer invariant.
async fn events(
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
fn safe_file_path(path: &str) -> Result<PathBuf, AppError> {
    if path.contains("..") || path.starts_with('/') {
        return Err(anyhow::anyhow!("Invalid path: path traversal detected").into());
    }
    Ok(StdPath::new("miku").join(format!("{path}.md")))
}

fn validate_folder_path(path: &str) -> Result<String, AppError> {
    let trimmed = path.trim_matches('/');
    if trimmed.contains("..") || path.starts_with('/') {
        return Err(anyhow::anyhow!("Invalid folder path: path traversal detected").into());
    }
    Ok(trimmed.to_string())
}

// Helper to compute SHA-256 hash of content
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn format_modified_time(file_path: &StdPath) -> String {
    fs::metadata(file_path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(|modified| {
            let local: DateTime<Local> = modified.into();
            local.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn first_plain_mention_range(body: &str, needle: &str) -> Option<(usize, usize)> {
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

fn promote_first_plain_mention(raw: &str, mention: &str, target: &str) -> Option<String> {
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

fn breadcrumb_parent(path: &str) -> Option<String> {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .filter(|parent| !parent.is_empty())
}

fn breadcrumb_items(path: &str, title: &str) -> Vec<BreadcrumbItem> {
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

// Helper struct for building nav tree (internal use only)
#[derive(Debug)]
struct TreeNode {
    title: String,
    stem: String,
    children: std::collections::BTreeMap<String, TreeNode>,
    is_leaf: bool,
}

fn file_browser_sort_key(stem: &str, is_leaf: bool) -> String {
    let normalized = stem.to_lowercase();
    if is_leaf && matches!(normalized.as_str(), "readme" | "index") {
        format!("!{normalized}")
    } else {
        normalized
    }
}

// Convert TreeNode BTreeMap tree into Vec<NavNode> with file-browser sorting.
// Folders come first, then pages; both groups order by path segment/stem.
fn tree_to_nav_nodes(
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
fn build_nav_tree(rows: Vec<(String, String)>) -> Vec<NavNode> {
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
// emits a collapsed stub that lazy-loads via /api/nav/children on first expand.
// This keeps the page payload to root level + the open page's path, not O(N).
fn prune_nav_tree(nodes: &mut [NavNode], active: &str, prefix: &str) {
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
fn nav_folder_children(nodes: Vec<NavNode>, dir: &str) -> Vec<NavNode> {
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
async fn nav_pages(index: &IndexApi, active: &str) -> Result<Vec<NavNode>, AppError> {
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

// GET /api/nav/children?dir=<folder> — htmx partial: the direct children of one
// folder, each subfolder itself collapsed/lazy. Lets the sidebar expand folders
// on demand instead of rendering the entire tree up front.
#[derive(serde::Deserialize)]
struct NavChildrenQuery {
    dir: Option<String>,
}

async fn nav_children_handler(
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

async fn folder_children(
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

async fn folder_view(
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
async fn page_handler(
    Path(path): Path<String>,
    Query(params): Query<EditQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(stripped_path) = path.strip_suffix("/edit") {
        page_edit(stripped_path.to_string(), params.template, state).await
    } else {
        page_view(path, state).await
    }
}

// Optional `?template=<id>` from the create-page modal, used to seed a brand-new
// page's editor body. Ignored for existing pages.
#[derive(serde::Deserialize)]
struct EditQuery {
    template: Option<String>,
}

// Seed bodies for the create-page modal's "start from" templates. The server is
// the single source of truth for this content (the modal only passes the id),
// so a freshly created page opens prefilled without a client-side markdown lib.
fn template_seed(id: &str) -> &'static str {
    match id {
        "meeting" => "# Meeting\n\n## Agenda\n\n## Notes\n\n## Actions\n",
        "reading" => "# Reading Notes\n\n## Summary\n\n## Highlights\n\n## Questions\n",
        "project" => "# Project\n\n## Goal\n\n## Tasks\n\n## Status\n",
        _ => "",
    }
}

async fn load_slug_map(
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

// Render the read-only page view
async fn page_view(path: String, state: AppState) -> Result<Response, AppError> {
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
    let linked_paths: std::collections::HashSet<String> =
        backlinks.iter().map(|link| link.path.clone()).collect();
    let unlinked_mentions = unlinked_mentions(&state.index, &path, &title, &linked_paths).await?;
    let backlinks_ms = timing_ms(backlinks_started);
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
        total_ms,
        "page_view rendered"
    );
    let mut response = Html(rendered).into_response();
    attach_server_timing(&mut response, "page_view", total_ms);
    Ok(response)
}

async fn unlinked_mentions(
    index: &IndexApi,
    current_path: &str,
    current_title: &str,
    linked_paths: &std::collections::HashSet<String>,
) -> Result<Vec<UnlinkedMention>, AppError> {
    // Narrow candidates with FTS first (mirrors `search`) so a page view reads
    // only the handful of pages that actually mention the title — never the
    // whole vault from disk on every render.
    let rows = index
        .search(miku_domain::SearchRequest {
            query: current_title.to_string(),
            scope: miku_domain::SearchScope::Body,
            limit: 100,
        })
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load pages for unlinked mentions")?;

    let mut mentions = Vec::new();
    for hit in rows {
        let display_path = hit
            .path
            .strip_suffix(".md")
            .unwrap_or(&hit.path)
            .to_string();
        if display_path == current_path || linked_paths.contains(&display_path) {
            continue;
        }
        let Some(raw) = safe_file_path(&display_path)
            .ok()
            .and_then(|file_path| fs::read_to_string(file_path).ok())
        else {
            continue;
        };
        let (_, body) = parse_frontmatter(&raw);
        if first_plain_mention_range(body, current_title).is_none() {
            continue;
        }
        mentions.push(UnlinkedMention {
            path: display_path,
            title: hit.title,
            snippet: search_snippet(body, current_title),
        });
        if mentions.len() >= 20 {
            break;
        }
    }

    Ok(mentions)
}

// Render the edit page
async fn page_edit(
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
struct EditForm {
    body: String,
    loaded_hash: String,
}

#[derive(serde::Deserialize)]
struct PreviewForm {
    body: String,
}

async fn preview(
    State(state): State<AppState>,
    Form(form): Form<PreviewForm>,
) -> Result<impl IntoResponse, AppError> {
    let slug_map = load_slug_map(&state.index).await?;
    let (_, body) = parse_frontmatter(&form.body);
    let (content_html, _) = render_html_with_toc(body, &|norm| slug_map.get(norm).cloned());

    Ok(Html(content_html))
}

// Handle the saving of a page
async fn page_save(
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
struct MoveForm {
    from: String,
    to: String,
}

// Handle moving/renaming a page. Serves both rename (context menu) and
// move-into-folder (drag): the client always sends the full destination path.
// Returns JSON so the tree controller can react without a full-page navigation;
// expected outcomes (collision, missing source) use explicit status codes.
async fn page_move(Json(form): Json<MoveForm>) -> Result<Response, AppError> {
    info!("Moving page from: {} to: {}", form.from, form.to);
    let src = safe_file_path(&form.from)?;
    let dst = safe_file_path(&form.to)?;

    if !src.exists() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "missing", "from": form.from })),
        )
            .into_response());
    }

    if dst.exists() {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "exists", "target": form.to })),
        )
            .into_response());
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).context(format!(
            "Failed to create parent directories: {}",
            parent.display()
        ))?;
    }

    fs::rename(&src, &dst).context(format!(
        "Failed to move file from {} to {}",
        src.display(),
        dst.display()
    ))?;

    info!("Moved page from {} to {} successfully", form.from, form.to);
    Ok(Json(serde_json::json!({ "ok": true, "path": form.to })).into_response())
}

#[derive(serde::Deserialize)]
struct TrashForm {
    path: String,
}

#[derive(serde::Deserialize)]
struct TrashIdForm {
    id: String,
}

// Sidecar manifest written next to each trashed `.md` (as `<id>.json`) so a
// soft-deleted page can be restored to its original location. Lives under
// `miku_docs/.trash`, which the indexer skips via `is_hidden_rel`, so manifests
// never enter the index.
#[derive(serde::Serialize, serde::Deserialize)]
struct TrashManifest {
    id: String,
    original_path: String,
    title: String,
    trashed_at: u64,
}

#[derive(serde::Deserialize)]
struct PromoteMentionForm {
    source_path: String,
    target_path: String,
    mention: String,
    return_to: String,
}

fn trash_dir() -> PathBuf {
    StdPath::new("miku").join(".trash")
}

// A trash id is a bare filename stem we join onto `miku_docs/.trash`; reject anything
// that could escape that directory.
fn safe_trash_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(anyhow::anyhow!("Invalid trash id").into());
    }
    Ok(())
}

// Handle trashing a page: move it into `miku_docs/.trash/<id>.md` and write a sidecar
// `<id>.json` manifest recording its original path so it can be restored. Returns
// JSON (no redirect) so the UI can offer an Undo without navigating away.
async fn page_trash(Json(form): Json<TrashForm>) -> Result<Response, AppError> {
    info!("Trashing page: {}", form.path);
    let src = safe_file_path(&form.path)?;

    if !src.exists() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "missing", "path": form.path })),
        )
            .into_response());
    }

    let dir = trash_dir();
    fs::create_dir_all(&dir).context(format!(
        "Failed to create trash directory: {}",
        dir.display()
    ))?;

    let raw = fs::read_to_string(&src)
        .context(format!("Failed to read page for trash: {}", src.display()))?;
    let (frontmatter, body) = parse_frontmatter(&raw);
    let title = extract_title(&form.path, frontmatter.as_ref(), body);

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("Failed to get current time")?
        .as_secs();
    let flattened = form.path.replace('/', "-");

    // Collision-proof id: append a counter if the base id is already taken.
    let base = format!("{flattened}-{ts}");
    let mut id = base.clone();
    let mut n = 1;
    while dir.join(format!("{id}.md")).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }

    let trash_md = dir.join(format!("{id}.md"));
    fs::rename(&src, &trash_md).context(format!(
        "Failed to move file to trash: {}",
        trash_md.display()
    ))?;

    let manifest = TrashManifest {
        id: id.clone(),
        original_path: form.path.clone(),
        title,
        trashed_at: ts,
    };
    let manifest_json =
        serde_json::to_string_pretty(&manifest).context("Failed to serialize trash manifest")?;
    fs::write(dir.join(format!("{id}.json")), manifest_json)
        .context("Failed to write trash manifest")?;

    info!("Trashed page {} as {}", form.path, id);
    Ok(
        Json(serde_json::json!({ "ok": true, "id": id, "original_path": form.path }))
            .into_response(),
    )
}

// List trashed pages (newest first) for the sidebar Trash view.
async fn trash_list() -> Result<Response, AppError> {
    let dir = trash_dir();
    let mut items: Vec<TrashManifest> = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir).context("Failed to read trash directory")? {
            let path = entry.context("Failed to read trash entry")?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(raw) = fs::read_to_string(&path) {
                    if let Ok(manifest) = serde_json::from_str::<TrashManifest>(&raw) {
                        items.push(manifest);
                    }
                }
            }
        }
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.trashed_at));
    Ok(Json(items).into_response())
}

// Restore a trashed page to its original path (409 if a live file now occupies it).
async fn trash_restore(Json(form): Json<TrashIdForm>) -> Result<Response, AppError> {
    safe_trash_id(&form.id)?;
    let dir = trash_dir();
    let manifest_path = dir.join(format!("{}.json", form.id));
    let trash_md = dir.join(format!("{}.md", form.id));

    if !manifest_path.exists() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "missing", "id": form.id })),
        )
            .into_response());
    }

    let raw = fs::read_to_string(&manifest_path).context("Failed to read trash manifest")?;
    let manifest: TrashManifest =
        serde_json::from_str(&raw).context("Failed to parse trash manifest")?;

    let dst = safe_file_path(&manifest.original_path)?;
    if dst.exists() {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "exists", "target": manifest.original_path })),
        )
            .into_response());
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directories for restore")?;
    }
    fs::rename(&trash_md, &dst).context("Failed to restore page from trash")?;
    fs::remove_file(&manifest_path).context("Failed to remove trash manifest after restore")?;

    info!("Restored {} from trash", manifest.original_path);
    Ok(Json(serde_json::json!({ "ok": true, "path": manifest.original_path })).into_response())
}

// Permanently delete a trashed page and its manifest.
async fn trash_purge(Json(form): Json<TrashIdForm>) -> Result<Response, AppError> {
    safe_trash_id(&form.id)?;
    let dir = trash_dir();
    for ext in ["md", "json"] {
        let path = dir.join(format!("{}.{ext}", form.id));
        if path.exists() {
            fs::remove_file(&path)
                .context(format!("Failed to purge trash file: {}", path.display()))?;
        }
    }
    info!("Purged trash item {}", form.id);
    Ok(Json(serde_json::json!({ "ok": true })).into_response())
}

async fn promote_mention(Form(form): Form<PromoteMentionForm>) -> Result<Response, AppError> {
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

// Search handler: full-text search over pages
#[derive(serde::Deserialize)]
struct SearchParams {
    q: Option<String>,
    scope: Option<String>,
}

async fn quickswitch(
    Query(params): Query<SearchParams>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let started = Instant::now();
    let query = params.q.as_deref().unwrap_or("").trim();
    let rows: Vec<(String, String)> = if query.is_empty() {
        state
            .index
            .list_pages()
            .await
            .map_err(|error| anyhow::anyhow!(error))
            .context("Failed to load quickswitch pages")?
            .into_iter()
            .take(20)
            .map(|page| (page.path, page.title))
            .collect()
    } else {
        state
            .index
            .search(miku_domain::SearchRequest {
                query: query.to_string(),
                scope: miku_domain::SearchScope::Title,
                limit: 20,
            })
            .await
            .map_err(|error| anyhow::anyhow!(error))
            .context("Failed to execute quickswitch search")?
            .into_iter()
            .map(|hit| (hit.path, hit.title))
            .collect()
    };

    let result_count = rows.len();
    let total_ms = timing_ms(started);
    info!(query, result_count, total_ms, "quickswitch searched");
    let mut response = Json(
        rows.into_iter()
            .map(|(path, title)| QuickSwitchResult {
                path: path.strip_suffix(".md").unwrap_or(&path).to_string(),
                title,
            })
            .collect::<Vec<_>>(),
    )
    .into_response();
    attach_server_timing(&mut response, "quickswitch", total_ms);
    Ok(response)
}

async fn search(
    Query(params): Query<SearchParams>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let started = Instant::now();
    let template = state.templates.get_template("search.html")?;

    let query_str = params.q.as_deref().unwrap_or("").trim().to_string();
    let scope = SearchScope::from_param(params.scope.as_deref());

    let results = if query_str.is_empty() {
        Vec::new()
    } else {
        let db_started = Instant::now();
        let rows = state
            .index
            .search(miku_domain::SearchRequest {
                query: query_str.clone(),
                scope: match scope {
                    SearchScope::All => miku_domain::SearchScope::All,
                    SearchScope::Title => miku_domain::SearchScope::Title,
                    SearchScope::Body => miku_domain::SearchScope::Body,
                },
                limit: 50,
            })
            .await
            .map_err(|error| anyhow::anyhow!(error))
            .context("Failed to execute search")?;
        let db_ms = timing_ms(db_started);

        let snippet_started = Instant::now();
        let results = rows
            .into_iter()
            .map(|hit| {
                let display_path = hit
                    .path
                    .strip_suffix(".md")
                    .unwrap_or(&hit.path)
                    .to_string();
                // The index stores only body_tsv, not the raw body, so build the
                // snippet from the file on disk (the source of truth).
                let snippet = safe_file_path(&display_path)
                    .ok()
                    .and_then(|file_path| fs::read_to_string(file_path).ok())
                    .map(|raw| {
                        let (_, body) = parse_frontmatter(&raw);
                        search_snippet(body, &query_str)
                    })
                    .unwrap_or_else(|| "No preview available.".to_string());

                SearchResult {
                    path: display_path,
                    title: hit.title,
                    snippet,
                }
            })
            .collect::<Vec<_>>();
        let snippet_ms = timing_ms(snippet_started);
        info!(
            query = %query_str,
            scope = scope.as_str(),
            result_count = results.len(),
            db_ms,
            snippet_ms,
            "search queried"
        );
        results
    };

    let nav = nav_pages(&state.index, "").await?;
    let rendered = template.render(context! {
        query => query_str,
        scope => scope.as_str(),
        results => results,
        nav_pages => nav,
        section => "search",
    })?;

    let total_ms = timing_ms(started);
    info!(scope = scope.as_str(), total_ms, "search rendered");
    let mut response = Html(rendered).into_response();
    attach_server_timing(&mut response, "search", total_ms);
    Ok(response)
}

// Tags index handler: list all tags with their counts
async fn tags_index(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    info!("Rendering tags index");
    let template = state.templates.get_template("tags.html")?;

    let tags: Vec<TagCount> = state
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

    let nav = nav_pages(&state.index, "").await?;
    let rendered = template.render(context! {
        tags => tags,
        nav_pages => nav,
        section => "tags",
    })?;

    Ok(Html(rendered).into_response())
}

// Tag filter handler: list all pages with a specific tag
async fn tag_filter(
    Path(tag): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    info!("Rendering tag filter for tag: {}", tag);
    let template = state.templates.get_template("tag.html")?;

    let pages: Vec<PageRef> = state
        .index
        .pages_with_tag(&tag)
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to load pages for tag")?
        .into_iter()
        .map(|page| PageRef {
            path: page
                .path
                .strip_suffix(".md")
                .unwrap_or(&page.path)
                .to_string(),
            title: page.title,
        })
        .collect();

    let nav = nav_pages(&state.index, "").await?;
    let rendered = template.render(context! {
        tag => tag,
        pages => pages,
        nav_pages => nav,
        section => "tags",
    })?;

    Ok(Html(rendered).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_breadcrumbs() -> Vec<BreadcrumbItem> {
        breadcrumb_items("Notes/Daily", "Daily")
    }

    #[test]
    fn test_template_rendering() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));

        let template = templates_env
            .get_template("page.html")
            .expect("Failed to get page.html template");
        let rendered = template
            .render(context! {
                title => "Test Title",
                path => "TestPath",
                exists => true,
                content_html => "<p>Test content</p>",
                body => "# Test Title\n\nTest content",
                loaded_hash => "abc",
                has_mermaid => false,
                backlinks => Vec::<Backlink>::new(),
                unlinked_mentions => Vec::<UnlinkedMention>::new(),
                toc => Vec::<Heading>::new(),
                word_count => 2usize,
                backlink_count => 0usize,
                updated => "2026-06-27 12:00",
                frontmatter => serde_json::Value::Object(serde_json::Map::new()),
                breadcrumb_parent => Option::<String>::None,
                breadcrumbs => test_breadcrumbs(),
            })
            .expect("Failed to render template");

        assert!(rendered.contains("Test Title"));
        assert!(rendered.contains("miku"));
        assert!(rendered.contains("data-inline-editor"));
        assert!(rendered.contains("data-inline-body"));
        assert!(rendered.contains("@codemirror/autocomplete"));
        assert!(rendered.contains("/api/quickswitch?q="));
        assert!(rendered.contains("class=\"mk-breadcrumb-link\""));
        assert!(rendered.contains("href=\"/folders/Notes\""));
        assert!(rendered.contains("href=\"/p/Notes&#x2f;Daily\""));
        assert!(!rendered.contains("mermaid.min.js"));
    }

    #[test]
    fn test_folder_template_renders_folder_browser() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));

        let template = templates_env
            .get_template("folder.html")
            .expect("Failed to get folder.html template");
        let rendered = template
            .render(context! {
                title => "Notes",
                path => "Notes",
                folders => vec![FolderChild {
                    name: "Daily".to_string(),
                    path: "Notes/Daily".to_string(),
                }],
                pages => vec![FolderPage {
                    title: "Overview".to_string(),
                    path: "Notes/Overview".to_string(),
                }],
                nav_pages => Vec::<NavNode>::new(),
                breadcrumbs => breadcrumb_items("Notes", "Notes"),
            })
            .expect("Failed to render folder.html template");

        assert!(rendered.contains("FOLDER"));
        assert!(rendered.contains("href=\"/folders/Notes&#x2f;Daily\""));
        assert!(rendered.contains("href=\"/p/Notes&#x2f;Overview\""));
        assert!(rendered.contains("Create note here"));
    }

    #[test]
    fn test_quickswitch_clears_loading_state_defensively() {
        let base = std::fs::read_to_string("src/templates/base.html")
            .expect("Failed to read base.html template");

        assert!(base.contains("queuePaletteRefresh()"));
        assert!(base.contains("paletteRequestId"));
        assert!(base.contains("AbortController"));
        assert!(base.contains("this.paletteLoading = false"));
        assert!(!base.contains("@input=\"refreshPalette()\""));
        assert!(!base.contains("x-model.debounce.120ms=\"paletteQuery\""));
    }

    #[test]
    fn test_shell_has_resizable_panes_without_repeated_page_marks() {
        let base = std::fs::read_to_string("src/templates/base.html")
            .expect("Failed to read base.html template");
        let page =
            std::fs::read_to_string("src/templates/page.html").expect("Failed to read page.html");
        let css = std::fs::read_to_string("static/miku.css").expect("Failed to read miku.css");

        assert!(base.contains("mk-sidebar-resizer"));
        assert!(page.contains("mk-rail-resizer"));
        assert!(base.contains("miku:sidebar-width"));
        assert!(base.contains("miku:rail-width"));
        assert!(css.contains("grid-template-columns: minmax(0, 1fr) 8px var(--rail-w)"));
        assert!(!page.contains("mk-page-mark"));
    }

    #[test]
    fn test_template_rendering_with_mermaid() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));

        let template = templates_env
            .get_template("page.html")
            .expect("Failed to get page.html template");
        let rendered = template
            .render(context! {
                title => "Test Title",
                path => "TestPath",
                exists => true,
                content_html => "<p>Test content</p>",
                body => "# Test Title\n\nTest content",
                loaded_hash => "abc",
                has_mermaid => true,
                backlinks => Vec::<Backlink>::new(),
                unlinked_mentions => Vec::<UnlinkedMention>::new(),
                toc => Vec::<Heading>::new(),
                word_count => 2usize,
                backlink_count => 0usize,
                updated => "2026-06-27 12:00",
                frontmatter => serde_json::Value::Object(serde_json::Map::new()),
                breadcrumb_parent => Option::<String>::None,
                breadcrumbs => test_breadcrumbs(),
            })
            .expect("Failed to render template");

        assert!(rendered.contains("mermaid.min.js"));
    }

    #[test]
    fn test_page_template_renders_frontmatter_properties() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));
        let template = templates_env
            .get_template("page.html")
            .expect("Failed to get page.html template");

        let frontmatter = serde_json::json!({
            "status": "draft",
            "tags": ["miku", "wiki"],
        });
        let rendered = template
            .render(context! {
                title => "Test Title",
                path => "Notes/Daily",
                exists => true,
                content_html => "<p>Test</p>",
                body => "# Test Title\n\nTest",
                loaded_hash => "abc",
                has_mermaid => false,
                backlinks => Vec::<Backlink>::new(),
                unlinked_mentions => Vec::<UnlinkedMention>::new(),
                toc => Vec::<Heading>::new(),
                word_count => 1usize,
                backlink_count => 0usize,
                updated => "2026-06-27 12:00",
                frontmatter => frontmatter,
                breadcrumb_parent => Option::<String>::None,
                breadcrumbs => test_breadcrumbs(),
            })
            .expect("Failed to render template");

        assert!(rendered.contains("status"));
        assert!(rendered.contains("draft"));
        // Sequence values render as chips.
        assert!(rendered.contains("miku"));
        assert!(rendered.contains("wiki"));
    }

    #[test]
    fn test_page_template_has_collapsible_cards_and_order() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));
        let template = templates_env
            .get_template("page.html")
            .expect("Failed to get page.html template");

        let frontmatter = serde_json::json!({
            "status": "draft",
        });

        let toc_headings = vec![Heading {
            level: 2,
            text: "Section A".to_string(),
            id: "section-a".to_string(),
        }];

        let rendered = template
            .render(context! {
                title => "Test Title",
                path => "Notes/Daily",
                exists => true,
                content_html => "<p>Test</p>",
                body => "# Test Title\n\nTest",
                loaded_hash => "abc",
                has_mermaid => false,
                backlinks => Vec::<Backlink>::new(),
                unlinked_mentions => Vec::<UnlinkedMention>::new(),
                toc => toc_headings,
                word_count => 10usize,
                backlink_count => 1usize,
                updated => "2026-06-27 12:00",
                frontmatter => frontmatter,
                breadcrumb_parent => Option::<String>::None,
                breadcrumbs => test_breadcrumbs(),
            })
            .expect("Failed to render template");

        // Verify the presence of collapsible elements
        assert!(rendered.contains("x-data=\"{ collapsed: false }\""));
        assert!(rendered.contains("mk-collapse-chevron"));

        // Verify the reordered elements (PAGE INFO then ON THIS PAGE)
        let idx_page_info = rendered.find("PAGE INFO").expect("PAGE INFO not found");
        let idx_on_this_page = rendered
            .find("ON THIS PAGE")
            .expect("ON THIS PAGE not found");

        assert!(
            idx_page_info < idx_on_this_page,
            "PAGE INFO should be rendered before ON THIS PAGE"
        );
    }

    #[test]
    fn test_template_seed_maps_ids_to_bodies() {
        assert!(template_seed("meeting").contains("## Agenda"));
        assert!(template_seed("reading").contains("## Highlights"));
        assert!(template_seed("project").contains("## Tasks"));
        // Blank and unknown ids both produce an empty page.
        assert_eq!(template_seed("blank"), "");
        assert_eq!(template_seed("bogus"), "");
    }

    #[test]
    fn test_edit_template_renders_seed_body_into_textarea() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));

        let template = templates_env
            .get_template("edit.html")
            .expect("Failed to get edit.html template");
        // Mirrors page_edit seeding a new page from ?template=meeting.
        let rendered = template
            .render(context! {
                path => "Notes/Standup",
                body => template_seed("meeting"),
                loaded_hash => "",
                nav_pages => Vec::<NavNode>::new(),
            })
            .expect("Failed to render template");

        assert!(rendered.contains("## Agenda"));
        assert!(rendered.contains("## Actions"));
    }

    #[test]
    fn test_edit_template_has_live_preview_editor() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));

        let template = templates_env
            .get_template("edit.html")
            .expect("Failed to get edit.html template");
        let rendered = template
            .render(context! {
                path => "TestPath",
                body => "# Draft",
                loaded_hash => "abc",
                nav_pages => Vec::<NavNode>::new(),
            })
            .expect("Failed to render template");

        assert!(rendered.contains("mk-edit"));
        assert!(rendered.contains("mk-edit-split"));
        assert!(rendered.contains("mk-preview mk-prose"));
        assert!(rendered.contains("name=\"loaded_hash\" value=\"abc\""));
        assert!(rendered.contains("fetch('/preview'"));
        assert!(rendered.contains("action=\"/p/TestPath\" method=\"POST\""));
    }

    #[test]
    fn test_build_nav_tree_nested_structure() {
        let rows = vec![
            ("a".to_string(), "A".to_string()),
            ("b/c".to_string(), "C".to_string()),
            ("b/d".to_string(), "D".to_string()),
        ];
        let result = build_nav_tree(rows);

        // Folders first, then pages
        assert_eq!(result.len(), 2);

        // First should be folder "b" (folders come first)
        assert_eq!(result[0].name, "b");
        assert_eq!(result[0].path, None);
        assert_eq!(result[0].children.len(), 2);

        // Folder b's children should be sorted: c, d (both pages)
        assert_eq!(result[0].children[0].name, "C");
        assert_eq!(result[0].children[0].path, Some("b/c".to_string()));
        assert_eq!(result[0].children[0].children.len(), 0);

        assert_eq!(result[0].children[1].name, "D");
        assert_eq!(result[0].children[1].path, Some("b/d".to_string()));
        assert_eq!(result[0].children[1].children.len(), 0);

        // Second should be page "a" (pages come after folders)
        assert_eq!(result[1].name, "A");
        assert_eq!(result[1].path, Some("a".to_string()));
        assert_eq!(result[1].children.len(), 0);
    }

    #[test]
    fn test_build_nav_tree_leaf_uses_title() {
        let rows = vec![("mypage".to_string(), "My Page Title".to_string())];
        let result = build_nav_tree(rows);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "My Page Title");
        assert_eq!(result[0].path, Some("mypage".to_string()));
    }

    #[test]
    fn test_build_nav_tree_folder_uses_segment() {
        let rows = vec![
            ("docs/api".to_string(), "API Reference".to_string()),
            ("docs/guide".to_string(), "User Guide".to_string()),
        ];
        let result = build_nav_tree(rows);

        // Root should have one folder
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "docs");
        assert_eq!(result[0].path, None);
        assert_eq!(result[0].children.len(), 2);

        // Children should be sorted alphabetically by name (case-insensitive)
        assert_eq!(result[0].children[0].name, "API Reference");
        assert_eq!(result[0].children[1].name, "User Guide");
    }

    #[test]
    fn test_build_nav_tree_sorting_case_insensitive() {
        let rows = vec![
            ("zebra".to_string(), "Zebra".to_string()),
            ("apple".to_string(), "Apple".to_string()),
            ("Banana".to_string(), "Banana".to_string()),
        ];
        let result = build_nav_tree(rows);

        // Should be sorted case-insensitively
        assert_eq!(result[0].name, "Apple");
        assert_eq!(result[1].name, "Banana");
        assert_eq!(result[2].name, "Zebra");
    }

    #[test]
    fn test_build_nav_tree_sorts_pages_by_file_stem_not_title() {
        let rows = vec![
            ("docs/02-setup".to_string(), "Apple Title".to_string()),
            ("docs/README".to_string(), "Folder Overview".to_string()),
            ("docs/01-intro".to_string(), "Zebra Title".to_string()),
        ];
        let result = build_nav_tree(rows);
        let docs = &result[0];
        let names: Vec<&str> = docs
            .children
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        let stems: Vec<&str> = docs
            .children
            .iter()
            .map(|node| node.stem.as_str())
            .collect();

        assert_eq!(names, vec!["Folder Overview", "Zebra Title", "Apple Title"]);
        assert_eq!(stems, vec!["README", "01-intro", "02-setup"]);
    }

    #[test]
    fn test_build_nav_tree_empty() {
        let rows = vec![];
        let result = build_nav_tree(rows);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_build_nav_tree_deep_nesting() {
        let rows = vec![
            ("a/b/c/d".to_string(), "Deep Page".to_string()),
            ("a/b/e".to_string(), "E".to_string()),
        ];
        let result = build_nav_tree(rows);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "a");
        assert_eq!(result[0].path, None);

        let level1 = &result[0].children;
        assert_eq!(level1.len(), 1);
        assert_eq!(level1[0].name, "b");
        assert_eq!(level1[0].path, None);

        let level2 = &level1[0].children;
        assert_eq!(level2.len(), 2);
        // c folder should come before e page
        assert_eq!(level2[0].name, "c");
        assert_eq!(level2[0].path, None);
        assert_eq!(level2[1].name, "E");
        assert_eq!(level2[1].path, Some("a/b/e".to_string()));

        let level3 = &level2[0].children;
        assert_eq!(level3.len(), 1);
        assert_eq!(level3[0].name, "Deep Page");
        assert_eq!(level3[0].path, Some("a/b/c/d".to_string()));
    }

    #[test]
    fn test_prune_nav_tree_keeps_only_active_ancestors() {
        let rows = vec![
            ("a/b/c".to_string(), "C".to_string()),
            ("a/x".to_string(), "X".to_string()),
            ("d/e".to_string(), "E".to_string()),
        ];
        let mut tree = build_nav_tree(rows);
        prune_nav_tree(&mut tree, "a/b/c", "");

        // Roots "a" and "d" both present, but only "a" (ancestor) keeps children.
        let a = tree.iter().find(|n| n.name == "a").expect("a present");
        let d = tree.iter().find(|n| n.name == "d").expect("d present");
        assert!(d.children.is_empty(), "non-ancestor folder pruned to stub");
        // "a" keeps child folder "b"; sibling page "x" stays (already loaded).
        let b = a.children.iter().find(|n| n.name == "b").expect("b kept");
        assert_eq!(b.children.len(), 1, "ancestor folder b keeps its child");
        assert_eq!(b.children[0].path, Some("a/b/c".to_string()));
    }

    #[test]
    fn test_nav_folder_children_descends_to_dir() {
        let rows = vec![
            ("a/b/c".to_string(), "C".to_string()),
            ("a/b/f".to_string(), "F".to_string()),
            ("a/x".to_string(), "X".to_string()),
        ];
        let tree = build_nav_tree(rows);
        let children = nav_folder_children(tree, "a/b");
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["C", "F"]);
    }

    #[test]
    fn test_nav_folder_children_missing_dir_is_empty() {
        let rows = vec![("a/b".to_string(), "B".to_string())];
        let tree = build_nav_tree(rows);
        assert!(nav_folder_children(tree, "nope/here").is_empty());
    }

    // The SSE feature is a read-only broadcast fan-out: the indexer sends a
    // page path, every subscriber's stream yields it. This proves the
    // broadcast -> BroadcastStream wiring in isolation (no DB, no HTTP server),
    // mirroring exactly what the `/events` handler does internally.
    #[tokio::test]
    async fn test_events_broadcast_reaches_subscriber_stream() {
        let (tx, _) = tokio::sync::broadcast::channel::<String>(256);

        // Subscribe BEFORE sending (mirrors a connected browser).
        let rx = tx.subscribe();
        let mut stream = BroadcastStream::new(rx)
            .filter_map(|item| item.ok().map(Ok::<_, std::convert::Infallible>));

        // The indexer broadcasts a re-indexed page path (`.md` stripped form).
        tx.send("Notes/Daily".to_string())
            .expect("subscriber present");

        let received = stream.next().await.expect("stream item").expect("ok item");
        assert_eq!(received, "Notes/Daily");
    }

    // `send` returns Err only when there are no subscribers; the indexer ignores
    // that with `let _ =`. Confirm the no-subscriber case is an error (so the
    // ignore is correct) and does not panic.
    #[test]
    fn test_events_send_with_no_subscribers_is_err() {
        let (tx, rx) = tokio::sync::broadcast::channel::<String>(256);
        drop(rx);
        assert!(tx.send("Orphan".to_string()).is_err());
    }

    #[test]
    fn test_promote_first_plain_mention_preserves_label() {
        let promoted = promote_first_plain_mention(
            "# Source\n\nThis references Target Page in prose.",
            "Target Page",
            "Notes/Target",
        )
        .expect("mention promoted");

        assert!(promoted.contains("[[Notes/Target|Target Page]]"));
    }

    #[test]
    fn test_promote_first_plain_mention_case_insensitive_after_multibyte() {
        // A multi-byte (CJK) prefix must not corrupt the byte offsets, and the
        // match is case-insensitive while preserving the original-cased label.
        let promoted = promote_first_plain_mention(
            "日本語 about target page in prose.",
            "Target Page",
            "Notes/Target",
        )
        .expect("mention promoted");

        assert!(promoted.contains("[[Notes/Target|target page]]"));
    }

    #[test]
    fn test_promote_first_plain_mention_skips_existing_wikilink() {
        let promoted =
            promote_first_plain_mention("Already [[Target Page]] here.", "Target Page", "Target");

        assert!(promoted.is_none());
    }

    #[tokio::test]
    async fn test_app_router_registers_events_route() {
        // Build the router with a dummy AppState (no DB connection is made until
        // a handler runs). This proves `/events` is wired into `fn app`.
        let (events, _) = tokio::sync::broadcast::channel::<String>(256);
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));
        let state = AppState {
            index: compose_index(IndexConfig::Memory)
                .await
                .expect("memory index API"),
            templates: Arc::new(templates_env),
            events,
        };
        // If `/events` (or any route) were malformed, `app` would panic here.
        let _router = app(state);
    }

    #[test]
    fn test_page_template_has_sse_live_preview() {
        let mut templates_env = Environment::new();
        templates_env.set_loader(minijinja::path_loader("src/templates"));

        let template = templates_env
            .get_template("page.html")
            .expect("Failed to get page.html template");
        let rendered = template
            .render(context! {
                title => "Test Title",
                path => "Notes/Daily",
                exists => true,
                content_html => "<p>Test content</p>",
                body => "# Test Title\n\nTest content",
                loaded_hash => "abc",
                has_mermaid => false,
                backlinks => Vec::<Backlink>::new(),
                unlinked_mentions => Vec::<UnlinkedMention>::new(),
                toc => Vec::<Heading>::new(),
                word_count => 2usize,
                backlink_count => 0usize,
                updated => "2026-06-27 12:00",
                frontmatter => serde_json::Value::Object(serde_json::Map::new()),
                breadcrumb_parent => Option::<String>::None,
                breadcrumbs => test_breadcrumbs(),
            })
            .expect("Failed to render template");

        // minijinja HTML-escapes `/` to `&#x2f;` inside the attribute value; the
        // browser's getAttribute decodes it back to "Notes/Daily", matching the
        // unescaped path the SSE broadcast sends. Assert on the escaped form.
        assert!(rendered.contains("data-page-path=\"Notes&#x2f;Daily\""));
        assert!(rendered.contains("new EventSource(\"/events\")"));
        assert!(rendered.contains("__miku_bulk_index_refresh__"));
        assert!(rendered.contains("class=\"mk-synced\""));
        assert!(rendered.contains("data-sync-indicator"));
    }

    #[test]
    fn test_safe_file_path_rejects_traversal() {
        let result = safe_file_path("../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_file_path_rejects_absolute() {
        let result = safe_file_path("/abs");
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_trash_id_accepts_generated_id() {
        // Ids are `<flattened-path>-<ts>` (with an optional `-<n>` suffix).
        assert!(safe_trash_id("Notes-Daily-1719800000").is_ok());
        assert!(safe_trash_id("Index-1719800000-2").is_ok());
    }

    #[test]
    fn test_safe_trash_id_rejects_traversal_and_separators() {
        assert!(safe_trash_id("").is_err());
        assert!(safe_trash_id("../secret").is_err());
        assert!(safe_trash_id("nested/id").is_err());
        assert!(safe_trash_id("back\\slash").is_err());
    }

    #[test]
    fn test_trash_manifest_round_trips() {
        let manifest = TrashManifest {
            id: "Notes-Daily-1719800000".to_string(),
            original_path: "Notes/Daily".to_string(),
            title: "Daily".to_string(),
            trashed_at: 1_719_800_000,
        };
        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: TrashManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, manifest.id);
        assert_eq!(back.original_path, "Notes/Daily");
        assert_eq!(back.title, "Daily");
        assert_eq!(back.trashed_at, 1_719_800_000);
    }
}
