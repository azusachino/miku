use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{
        header::{self, HeaderName},
        HeaderValue, StatusCode,
    },
    response::{
        sse::{self, KeepAlive, Sse},
        Html, IntoResponse, Redirect, Response,
    },
    Form, Json,
};
use chrono::{DateTime, Local};
use miku::markdown::{extract_title, parse_frontmatter, render_html_with_toc, Heading};
use miku_app::{compose_index, resolve_runtime, IndexApi};
use miku_domain::IndexCapabilities;
use minijinja::{context, Environment};
use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tracing::{info, warn};

mod http;
mod routes;

const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");

struct LocalLogTimer;

impl tracing_subscriber::fmt::time::FormatTime for LocalLogTimer {
    fn format_time(
        &self,
        writer: &mut tracing_subscriber::fmt::format::Writer<'_>,
    ) -> std::fmt::Result {
        write!(writer, "{}", Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

/// Templates baked into the binary (see the rust-embed dep in Cargo.toml) so the
/// app renders without a source tree next to the executable.
#[derive(rust_embed::RustEmbed)]
#[folder = "src/templates"]
struct TemplateAssets;

/// Static assets (CSS/JS/icons) baked into the binary and served from `/static`.
#[derive(rust_embed::RustEmbed)]
#[folder = "static"]
pub(crate) struct StaticAssets;

struct HttpMetrics {
    started_at: Instant,
    requests_total: AtomicU64,
    duration_microseconds_sum: AtomicU64,
    duration_buckets: [AtomicU64; 7],
}

impl HttpMetrics {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            requests_total: AtomicU64::new(0),
            duration_microseconds_sum: AtomicU64::new(0),
            duration_buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    fn observe(&self, duration: Duration) {
        let microseconds =
            u64::try_from(duration.as_micros().min(u128::from(u64::MAX))).unwrap_or(u64::MAX);
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.duration_microseconds_sum
            .fetch_add(microseconds, Ordering::Relaxed);
        for (bucket, limit) in self
            .duration_buckets
            .iter()
            .zip([1_000, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000])
        {
            if microseconds <= limit {
                bucket.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

static HTTP_METRICS: OnceLock<HttpMetrics> = OnceLock::new();

fn http_metrics() -> &'static HttpMetrics {
    HTTP_METRICS.get_or_init(HttpMetrics::new)
}

fn observe_http_response<B>(
    _response: &axum::http::Response<B>,
    latency: Duration,
    _span: &tracing::Span,
) {
    http_metrics().observe(latency);
}

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

#[derive(Clone, serde::Serialize)]
struct TagCount {
    tag: String,
    count: i64,
}

#[derive(serde::Deserialize)]
struct TagsQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(serde::Serialize)]
struct TagsPage {
    tags: Vec<TagCount>,
    offset: usize,
    limit: usize,
    total: usize,
    has_more: bool,
    next_offset: usize,
}

#[derive(serde::Serialize)]
struct TagPagesPage {
    pages: Vec<PageRef>,
    offset: usize,
    limit: usize,
    total: usize,
    has_more: bool,
    next_offset: usize,
}

const TAG_PAGE_SIZE: usize = 50;
const TAG_RESULT_PAGE_SIZE: usize = 50;

fn paginate_tags(tags: Vec<TagCount>, offset: usize, requested_limit: usize) -> TagsPage {
    let total = tags.len();
    let limit = requested_limit.clamp(1, 100);
    let offset = offset.min(total);
    let page_tags: Vec<TagCount> = tags.into_iter().skip(offset).take(limit).collect();
    let next_offset = offset + page_tags.len();
    TagsPage {
        tags: page_tags,
        offset,
        limit,
        total,
        has_more: next_offset < total,
        next_offset,
    }
}

fn paginate_page_refs(pages: Vec<PageRef>, offset: usize, requested_limit: usize) -> TagPagesPage {
    let total = pages.len();
    let limit = requested_limit.clamp(1, 100);
    let offset = offset.min(total);
    let page_pages: Vec<PageRef> = pages.into_iter().skip(offset).take(limit).collect();
    let next_offset = offset + page_pages.len();
    TagPagesPage {
        pages: page_pages,
        offset,
        limit,
        total,
        has_more: next_offset < total,
        next_offset,
    }
}

async fn pages_for_tag(state: &AppState, tag: &str) -> Result<Vec<PageRef>, AppError> {
    Ok(state
        .index
        .pages_with_tag(tag)
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
        .collect())
}

#[derive(serde::Serialize)]
struct PageRef {
    path: String,
    title: String,
}

#[derive(serde::Serialize)]
struct QuickSwitchResult {
    path: String,
    title: String,
    snippet: String,
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

#[derive(serde::Serialize)]
struct ReaderPagePayload {
    path: String,
    title: String,
    exists: bool,
    html: String,
    content_html: String,
    toc: Vec<Heading>,
    backlinks: Vec<Backlink>,
    unlinked_mentions: Vec<UnlinkedMention>,
    word_count: usize,
    backlink_count: usize,
    updated: String,
    frontmatter: serde_json::Value,
    breadcrumbs: Vec<BreadcrumbItem>,
}

fn reader_page_path(path: &str) -> String {
    path.strip_suffix(".md").unwrap_or(path).to_string()
}

fn render_reader_fragment(
    state: &AppState,
    payload: &ReaderPagePayload,
) -> Result<String, AppError> {
    let template = state.templates.get_template("reader_fragment.html")?;
    Ok(template.render(context! {
        path => &payload.path,
        title => &payload.title,
        exists => payload.exists,
        content_html => &payload.content_html,
        toc => &payload.toc,
        backlinks => &payload.backlinks,
        unlinked_mentions => &payload.unlinked_mentions,
        word_count => payload.word_count,
        backlink_count => payload.backlink_count,
        updated => &payload.updated,
        frontmatter => &payload.frontmatter,
        breadcrumbs => &payload.breadcrumbs,
    })?)
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
    reconcile: miku::indexer::ReconcileTrigger,
    templates: Arc<Environment<'static>>,
    index_ready: Arc<AtomicBool>,
    // Broadcasts the relative path (`.md` stripped) of each page the background
    // indexer just re-indexed, so connected browsers can live-refresh via SSE.
    // Read-only fan-out: the SSE layer never writes the Postgres index.
    events: tokio::sync::broadcast::Sender<String>,
}

// Custom error handling for Axum route handlers
struct AppError {
    error: anyhow::Error,
    status: StatusCode,
}

impl AppError {
    fn bad_request(error: impl Into<anyhow::Error>) -> Self {
        Self {
            error: error.into(),
            status: StatusCode::BAD_REQUEST,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        warn!(error = ?self.error, status = %self.status, "Handler error");
        (self.status, format!("Something went wrong: {}", self.error)).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self {
            error: err.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize tracing. Default to plain `info`; verbose debug logging is
    //    opt-in via RUST_LOG, which the Makefile's `run`/`dev` targets set for
    //    local dev. Release binaries and the container stay quiet by default.
    tracing_subscriber::fmt()
        .with_timer(LocalLogTimer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Miku Server...");

    // 2. Resolve and compose the selected runtime. Backend drivers stay below
    // miku-app; this binary only owns process startup and HTTP concerns.
    let runtime = resolve_runtime()
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to resolve the selected Miku runtime")?;
    let index = compose_index(runtime)
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("Failed to compose the selected Miku runtime")?;

    // 5. Initialize Minijinja template environment from the embedded templates
    // so the binary renders without a source tree beside it.
    let mut templates_env = Environment::new();
    templates_env.set_loader(|name| {
        Ok(TemplateAssets::get(name).map(|file| String::from_utf8_lossy(&file.data).into_owned()))
    });

    // SSE broadcast channel: the indexer is the sole sender; each browser /events
    // connection is a subscriber. Capacity 256 bounds backpressure; slow
    // subscribers see Lagged and resync on the next event.
    let (events_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    // 6. Initialize background indexer
    let indexer = miku::indexer::IndexerQueue::new_with_writer(
        index.reader(),
        index.writer(),
        std::path::PathBuf::from("miku_docs"),
        events_tx.clone(),
    )
    .context("Failed to initialize background indexer")?;

    let state = AppState {
        index: index.clone(),
        reconcile: indexer.reconcile_trigger(),
        templates: Arc::new(templates_env),
        index_ready: indexer.ready_handle(),
        events: events_tx,
    };

    // 7. Build Router & Configure axum routes
    let app = http::router(state);

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
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    indexer.shutdown().await;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("Shutdown requested by Ctrl-C"),
        () = terminate => info!("Shutdown requested by SIGTERM"),
    }
}

#[derive(serde::Serialize)]
struct HealthResponse {
    status: &'static str,
    capabilities: IndexCapabilities,
    index_ready: bool,
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn readyz(State(state): State<AppState>) -> Response {
    let capabilities = state
        .index
        .capabilities()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE);
    let Ok(capabilities) = capabilities else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let ready = state.index_ready.load(std::sync::atomic::Ordering::Acquire);
    let response = Json(HealthResponse {
        status: "ok",
        capabilities,
        index_ready: ready,
    });
    if ready {
        (StatusCode::OK, response).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, response).into_response()
    }
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = http_metrics();
    let bucket_limits = [
        ("1000", 0),
        ("5000", 1),
        ("10000", 2),
        ("25000", 3),
        ("50000", 4),
        ("100000", 5),
        ("250000", 6),
    ];
    let mut body = format!(
        "# HELP miku_process_uptime_seconds Process uptime.\n# TYPE miku_process_uptime_seconds gauge\nmiku_process_uptime_seconds {}\n# HELP miku_index_ready Whether the initial index reconcile has completed.\n# TYPE miku_index_ready gauge\nmiku_index_ready {}\n# HELP miku_http_requests_total Total HTTP responses.\n# TYPE miku_http_requests_total counter\nmiku_http_requests_total {}\n# HELP miku_http_request_duration_microseconds_sum Sum of HTTP response durations in microseconds.\n# TYPE miku_http_request_duration_microseconds_sum counter\nmiku_http_request_duration_microseconds_sum {}\n# HELP miku_http_request_duration_microseconds HTTP response duration distribution.\n# TYPE miku_http_request_duration_microseconds histogram\n",
        metrics.started_at.elapsed().as_secs_f64(),
        u8::from(state.index_ready.load(Ordering::Acquire)),
        metrics.requests_total.load(Ordering::Relaxed),
        metrics.duration_microseconds_sum.load(Ordering::Relaxed),
    );
    for (limit, index) in bucket_limits {
        let _ = writeln!(
            body,
            "miku_http_request_duration_microseconds_bucket{{le=\"{limit}\"}} {}",
            metrics.duration_buckets[index].load(Ordering::Relaxed)
        );
    }
    let _ = writeln!(
        body,
        "miku_http_request_duration_microseconds_bucket{{le=\"+Inf\"}} {}",
        metrics.requests_total.load(Ordering::Relaxed),
    );
    let _ = writeln!(
        body,
        "miku_http_request_duration_microseconds_count {}",
        metrics.requests_total.load(Ordering::Relaxed),
    );
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4"),
        )],
        body,
    )
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
