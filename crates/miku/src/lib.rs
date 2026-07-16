//! Miku backend bootstrap.
//!
//! The separate `miku-web` project owns the user interface. This crate owns
//! process composition, the filesystem indexer, and the HTTP API surface.

pub mod indexer;
pub use anyhow::{Context, Result};

use axum::{extract::State, response::IntoResponse, response::Response, Json};
use chrono::Local;
use miku_app::{
    compose_index, resolve_runtime, FileMikuApplication, FileWorkspaceService, IndexApi,
    MikuApplication, WorkspaceService,
};
use miku_domain::IndexCapabilities;
use miku_vault::Vault;
use std::env;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};
use tracing::{info, warn};

mod http;
mod openapi;
mod workspace_api;

const SERVER_TIMING: axum::http::header::HeaderName =
    axum::http::header::HeaderName::from_static("server-timing");

struct LocalLogTimer;

impl tracing_subscriber::fmt::time::FormatTime for LocalLogTimer {
    fn format_time(
        &self,
        writer: &mut tracing_subscriber::fmt::format::Writer<'_>,
    ) -> std::fmt::Result {
        write!(writer, "{}", Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

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

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) index: IndexApi,
    pub(crate) application: Arc<dyn MikuApplication>,
    pub(crate) index_ready: Arc<AtomicBool>,
    pub(crate) events: tokio::sync::broadcast::Sender<String>,
}

pub(crate) struct AppError {
    pub(crate) error: anyhow::Error,
    pub(crate) status: axum::http::StatusCode,
}

impl AppError {
    pub(crate) fn not_found(error: impl Into<anyhow::Error>) -> Self {
        Self {
            error: error.into(),
            status: axum::http::StatusCode::NOT_FOUND,
        }
    }

    pub(crate) fn forbidden(error: impl Into<anyhow::Error>) -> Self {
        Self {
            error: error.into(),
            status: axum::http::StatusCode::FORBIDDEN,
        }
    }

    pub(crate) fn conflict(error: impl Into<anyhow::Error>) -> Self {
        Self {
            error: error.into(),
            status: axum::http::StatusCode::CONFLICT,
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
    fn from(error: E) -> Self {
        Self {
            error: error.into(),
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Run the API server and embedded filesystem indexer.
pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_timer(LocalLogTimer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Miku API server...");
    let runtime = resolve_runtime()
        .map_err(|error| anyhow::anyhow!(error))
        .context("failed to resolve Miku runtime")?;
    let index = compose_index(runtime)
        .await
        .map_err(|error| anyhow::anyhow!(error))
        .context("failed to compose Miku index")?;
    let (events, _) = tokio::sync::broadcast::channel::<String>(256);
    let indexer = indexer::IndexerQueue::new_with_writer(
        index.reader(),
        index.writer(),
        std::path::PathBuf::from("miku_docs"),
        events.clone(),
    )
    .context("failed to initialize filesystem indexer")?;
    let vault = Arc::new(Vault::new("miku_docs"));
    let readonly = env::var("MIKU_READONLY")
        .map(|value| value != "0" && value != "false")
        .unwrap_or(false);
    let workspace: Arc<dyn WorkspaceService> =
        Arc::new(FileWorkspaceService::new(Arc::clone(&vault), readonly));
    let application_service = Arc::new(FileMikuApplication::new(
        Arc::clone(&vault),
        Arc::clone(&workspace),
        index.clone(),
    ));
    let application: Arc<dyn MikuApplication> = application_service.clone();
    let mut event_receiver = events.subscribe();
    let application_for_events = application_service.clone();
    tokio::spawn(async move {
        while event_receiver.recv().await.is_ok() {
            application_for_events.invalidate_documents().await;
        }
    });
    let state = AppState {
        index: index.clone(),
        application,
        index_ready: indexer.ready_handle(),
        events,
    };
    let app = http::router(state);
    let address: SocketAddr = env::var("MIKU_BIND")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()
        .context("MIKU_BIND must be a valid socket address")?;
    info!("Listening on {}", address);
    let listener = tokio::net::TcpListener::bind(address).await?;
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

pub(crate) async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

pub(crate) async fn readyz(State(state): State<AppState>) -> Response {
    let Ok(capabilities) = state.index.capabilities().await else {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let ready = state.index_ready.load(Ordering::Acquire);
    let response = Json(HealthResponse {
        status: "ok",
        capabilities,
        index_ready: ready,
    });
    if ready {
        (axum::http::StatusCode::OK, response).into_response()
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, response).into_response()
    }
}

pub(crate) async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = http_metrics();
    let limits = [
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
        u8::from(state.index_ready.load(Ordering::Relaxed)),
        metrics.requests_total.load(Ordering::Relaxed),
        metrics.duration_microseconds_sum.load(Ordering::Relaxed),
    );
    for (limit, index) in limits {
        let _ = writeln!(
            body,
            "miku_http_request_duration_microseconds_bucket{{le=\"{limit}\"}} {}",
            metrics.duration_buckets[index].load(Ordering::Relaxed)
        );
    }
    let _ = writeln!(
        body,
        "miku_http_request_duration_microseconds_bucket{{le=\"+Inf\"}} {}",
        metrics.requests_total.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        body,
        "miku_http_request_duration_microseconds_count {}",
        metrics.requests_total.load(Ordering::Relaxed)
    );
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4",
            ),
            (SERVER_TIMING, ""),
        ],
        body,
    )
}
