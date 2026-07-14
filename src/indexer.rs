use crate::markdown::{
    comrak_options, extract_title, is_asset_path, normalize_target, parse_frontmatter, EMBED_REGEX,
};
use anyhow::Result;
use comrak::nodes::NodeValue;
use notify::Watcher;
use regex::Regex;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{error, info, warn};

pub const BULK_SSE_EVENT: &str = "__miku_bulk_index_refresh__";
const DEFAULT_BULK_SSE_THRESHOLD: usize = 256;

// Skip dot-dirs/files (.trash soft-delete archive, .git, etc.) anywhere in the
// relative path so trashed pages and VCS metadata never enter the index.
fn is_hidden_rel(rel_path: &Path) -> bool {
    rel_path
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

fn extract_tags_from_frontmatter(fm: &serde_json::Value) -> HashSet<String> {
    let mut tags = HashSet::new();
    if let Some(tags_val) = fm.get("tags") {
        if let Some(arr) = tags_val.as_array() {
            for v in arr {
                if let Some(s) = v.as_str() {
                    tags.insert(s.trim().to_string());
                }
            }
        } else if let Some(s) = tags_val.as_str() {
            for t in s.split(',') {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    tags.insert(trimmed.to_string());
                }
            }
        }
    }
    tags
}

fn extract_aliases(fm: &serde_json::Value) -> HashSet<String> {
    let mut aliases = HashSet::new();
    if let Some(val) = fm.get("aliases") {
        if let Some(arr) = val.as_array() {
            for v in arr {
                if let Some(s) = v.as_str() {
                    aliases.insert(s.trim().to_string());
                }
            }
        } else if let Some(s) = val.as_str() {
            for a in s.split(',') {
                let trimmed = a.trim();
                if !trimmed.is_empty() {
                    aliases.insert(trimmed.to_string());
                }
            }
        }
    }
    aliases
}

struct ExtractedLink {
    kind: String,
    is_embed: bool,
    target: String,
    target_norm: String,
    alias: Option<String>,
}

struct PageMetadata {
    title: String,
    has_mermaid: bool,
    tags: HashSet<String>,
    links: Vec<ExtractedLink>,
    aliases: HashSet<String>,
}

struct PageIndexData {
    frontmatter_json: serde_json::Value,
    metadata: PageMetadata,
    body: String,
}

fn push_without_nuls(out: &mut String, value: &str) {
    out.extend(value.chars().filter(|c| *c != '\0'));
}

fn sanitize_text_for_postgres(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    push_without_nuls(&mut sanitized, value);
    sanitized
}

fn sanitize_json_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = sanitize_text_for_postgres(s);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                sanitize_json_strings(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                sanitize_json_strings(value);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn decode_indexable_markdown(bytes: &[u8]) -> String {
    let mut content = String::with_capacity(bytes.len());
    let mut remaining = bytes;

    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(valid) => {
                push_without_nuls(&mut content, valid);
                break;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                let valid = std::str::from_utf8(&remaining[..valid_up_to])
                    .expect("valid_up_to must split at a UTF-8 boundary");
                push_without_nuls(&mut content, valid);

                let invalid_len = err.error_len().unwrap_or(1);
                remaining = &remaining[valid_up_to + invalid_len..];
            }
        }
    }

    content
}

fn extract_metadata(
    path: &str,
    frontmatter: Option<&serde_json::Value>,
    body: &str,
) -> PageMetadata {
    let arena = comrak::Arena::new();
    let options = comrak_options();
    let root = comrak::parse_document(&arena, body, &options);

    let mut tags = frontmatter
        .map(extract_tags_from_frontmatter)
        .unwrap_or_default();
    let aliases = frontmatter.map(extract_aliases).unwrap_or_default();
    let mut links = Vec::new();

    let has_mermaid = body.contains("```mermaid");

    lazy_static::lazy_static! {
        static ref TAG_REGEX: Regex = Regex::new(r"(?:\s|^|['`\(])#([\p{L}\p{N}][\p{L}\p{N}_\-/]*)").unwrap();
    }

    // Skip `#tags` inside code (literal) and links (URL fragments), but allow
    // them inside headings (Obsidian-style) — kept in sync with the renderer's
    // tag_skipped so the indexed tag set matches the rendered tag links.
    let is_skipped = |node: &comrak::nodes::AstNode| -> bool {
        let mut current = node.parent();
        while let Some(parent) = current {
            let val = &parent.data.borrow().value;
            match val {
                NodeValue::CodeBlock(_) | NodeValue::Link(_) => return true,
                _ => {}
            }
            current = parent.parent();
        }
        false
    };

    for node in root.descendants() {
        if is_skipped(node) {
            continue;
        }

        match &node.data.borrow().value {
            // Comrak tokenizes `[[Target]]` page links (but never `![[...]]`
            // embeds — those stay as text and are handled below).
            NodeValue::WikiLink(w) => {
                let target = w.url.clone();
                let target_norm = normalize_target(&target, false);

                let mut alias_text = String::new();
                for child in node.children() {
                    for desc in child.descendants() {
                        if let NodeValue::Text(t) = &desc.data.borrow().value {
                            alias_text.push_str(t);
                        }
                    }
                }

                let alias = if alias_text.is_empty() || alias_text == target {
                    None
                } else {
                    Some(alias_text)
                };

                links.push(ExtractedLink {
                    kind: "page".to_string(),
                    is_embed: false,
                    target,
                    target_norm,
                    alias,
                });
            }
            // Text nodes carry both inline `#tags` and `![[embed]]` runs, which
            // comrak does not parse into dedicated nodes.
            NodeValue::Text(t) => {
                for cap in TAG_REGEX.captures_iter(t) {
                    if let Some(m) = cap.get(1) {
                        tags.insert(m.as_str().to_string());
                    }
                }

                for cap in EMBED_REGEX.captures_iter(t) {
                    if let Some(target_match) = cap.get(1) {
                        let target = target_match.as_str().trim().to_string();
                        let alias = cap.get(2).map(|m| m.as_str().trim().to_string());

                        let is_asset = is_asset_path(&target);
                        let kind = if is_asset { "asset" } else { "page" }.to_string();
                        let target_norm = normalize_target(&target, is_asset);

                        links.push(ExtractedLink {
                            kind,
                            is_embed: true,
                            target,
                            target_norm,
                            alias,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let title = extract_title(path, frontmatter, body);

    PageMetadata {
        title,
        has_mermaid,
        tags,
        links,
        aliases,
    }
}

fn prepare_page_index_data(path: &str, raw_content: &str) -> PageIndexData {
    let (frontmatter, body) = parse_frontmatter(raw_content);
    let mut frontmatter_json =
        frontmatter.unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    sanitize_json_strings(&mut frontmatter_json);

    let mut metadata = extract_metadata(path, Some(&frontmatter_json), body);
    metadata.title = sanitize_text_for_postgres(&metadata.title);

    PageIndexData {
        frontmatter_json,
        metadata,
        body: body.to_string(),
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum WatcherEvent {
    Modified(PathBuf),
    Deleted(PathBuf),
}

pub struct IndexerQueue {
    sender: mpsc::Sender<WatcherEvent>,
    reconcile_queued: Arc<AtomicBool>,
    _watcher: notify::RecommendedWatcher,
    tasks: Vec<JoinHandle<()>>,
}

impl IndexerQueue {
    pub fn new(
        db_pool: PgPool,
        content_root: PathBuf,
        events: broadcast::Sender<String>,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(1024);
        let reconcile_queued = Arc::new(AtomicBool::new(false));

        // 1. Run startup reconcile sweep in the background
        let db_clone = db_pool.clone();
        let content_root_clone = content_root.clone();
        let events_clone = events.clone();
        let reconcile_queued_for_consumer = Arc::clone(&reconcile_queued);
        let consumer_task = tokio::spawn(async move {
            let mut index_failures = HashMap::new();
            info!("Starting startup reconcile sweep...");
            if let Err(e) = Self::reconcile_all(
                &db_clone,
                &content_root_clone,
                &events_clone,
                &mut index_failures,
            )
            .await
            {
                error!("Startup reconcile sweep failed: {:?}", e);
            } else {
                info!("Startup reconcile sweep completed successfully.");
            }

            // Start consumer loop
            Self::run_consumer(
                receiver,
                db_clone,
                content_root_clone,
                events_clone,
                reconcile_queued_for_consumer,
                index_failures,
            )
            .await;
        });

        // 2. Set up notify watcher
        let sender_for_watcher = sender.clone();
        let content_root_for_watcher = content_root.clone();
        let mut watcher = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| match res {
                Ok(event) => {
                    for path in event.paths {
                        // Check if file is markdown
                        if path.extension().map_or(false, |ext| ext == "md") {
                            // Also exclude temporary files
                            if path.to_string_lossy().ends_with(".tmp") {
                                continue;
                            }
                            if let Ok(rel_path) = path.strip_prefix(&content_root_for_watcher) {
                                // Skip dot-dirs (.trash soft-delete archive, .git, etc.)
                                if is_hidden_rel(rel_path) {
                                    continue;
                                }
                                let w_event = if path.exists() {
                                    WatcherEvent::Modified(rel_path.to_path_buf())
                                } else {
                                    WatcherEvent::Deleted(rel_path.to_path_buf())
                                };
                                if let Err(e) = sender_for_watcher.try_send(w_event) {
                                    warn!("Failed to send watcher event to channel (might be full/closed): {:?}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Watcher error: {:?}", e);
                }
            },
            notify::Config::default(),
        )?;

        // Exclude .git and .trash if they exist in content_root (they shouldn't be under miku_docs/ but let's be safe)
        if content_root.exists() {
            watcher.watch(&content_root, notify::RecursiveMode::Recursive)?;
            info!(
                "Directory watcher successfully initialized on {:?}",
                content_root
            );
        } else {
            // Auto-create content root if not present
            fs::create_dir_all(&content_root)?;
            watcher.watch(&content_root, notify::RecursiveMode::Recursive)?;
            info!("Created and watched content root {:?}", content_root);
        }

        // 3. Periodic reconcile fallback. inotify events do not propagate across
        // some container bind mounts (e.g. podman), so the notify watcher alone
        // can miss every change after startup. Poll-reconcile on an interval as a
        // safety net: reconcile_all is idempotent (mtime-based upserts +
        // delete-missing) and runs on the same single-writer consumer via the
        // __reconcile__ sentinel, so this never races the watcher or the handlers.
        let reconcile_sender = sender.clone();
        let reconcile_queued_for_ticker = Arc::clone(&reconcile_queued);
        let reconcile_interval = Self::reconcile_interval();
        let ticker_task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(reconcile_interval);
            ticker.tick().await; // consume the immediate first tick (startup sweep covers it)
            loop {
                ticker.tick().await;
                Self::try_queue_reconcile(&reconcile_sender, &reconcile_queued_for_ticker);
            }
        });

        Ok(Self {
            sender,
            reconcile_queued,
            _watcher: watcher,
            tasks: vec![consumer_task, ticker_task],
        })
    }

    /// Stop background indexing and await task termination.
    pub async fn shutdown(self) {
        let IndexerQueue { tasks, .. } = self;
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
    }

    pub fn trigger_reconcile(&self) {
        // If channel fills or other safety guards trigger
        Self::try_queue_reconcile(&self.sender, &self.reconcile_queued);
    }

    fn reconcile_interval() -> Duration {
        env::var("MIKU_RECONCILE_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .map_or_else(|| Duration::from_secs(30), Duration::from_secs)
    }

    fn bulk_sse_threshold() -> usize {
        env::var("MIKU_BULK_SSE_THRESHOLD")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|threshold| *threshold > 0)
            .unwrap_or(DEFAULT_BULK_SSE_THRESHOLD)
    }

    fn broadcast_affected(events: &broadcast::Sender<String>, affected: Vec<String>) {
        if affected.len() > Self::bulk_sse_threshold() {
            let _ = events.send(BULK_SSE_EVENT.to_string());
            return;
        }

        for path in affected {
            let _ = events.send(path);
        }
    }

    fn try_queue_reconcile(
        sender: &mpsc::Sender<WatcherEvent>,
        reconcile_queued: &AtomicBool,
    ) -> bool {
        if reconcile_queued
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }

        match sender.try_send(WatcherEvent::Modified(PathBuf::from("__reconcile__"))) {
            Ok(()) => true,
            Err(e) => {
                reconcile_queued.store(false, Ordering::Release);
                warn!("Failed to queue reconcile event: {:?}", e);
                false
            }
        }
    }

    async fn run_consumer(
        mut receiver: mpsc::Receiver<WatcherEvent>,
        db_pool: PgPool,
        content_root: PathBuf,
        events: broadcast::Sender<String>,
        reconcile_queued: Arc<AtomicBool>,
        mut index_failures: HashMap<PathBuf, i64>,
    ) {
        let mut debounce_buffer = HashSet::new();
        let debounce_duration = Duration::from_millis(300);

        while let Some(event) = receiver.recv().await {
            if event == WatcherEvent::Modified(PathBuf::from("__reconcile__")) {
                info!("Manual reconcile event triggered, running reconciliation...");
                if let Err(e) =
                    Self::reconcile_all(&db_pool, &content_root, &events, &mut index_failures).await
                {
                    error!("Reconciliation sweep failed: {:?}", e);
                }
                reconcile_queued.store(false, Ordering::Release);
                continue;
            }

            debounce_buffer.insert(event);

            // Drain any pending events immediately available
            while let Ok(evt) = receiver.try_recv() {
                if evt == WatcherEvent::Modified(PathBuf::from("__reconcile__")) {
                    debounce_buffer.clear();
                    let _ =
                        Self::reconcile_all(&db_pool, &content_root, &events, &mut index_failures)
                            .await;
                    reconcile_queued.store(false, Ordering::Release);
                    break;
                }
                debounce_buffer.insert(evt);
            }

            if debounce_buffer.is_empty() {
                continue;
            }

            // Sleep to let more changes accumulate (debounce window)
            sleep(debounce_duration).await;

            // Drain again to capture anything that arrived during sleep
            while let Ok(evt) = receiver.try_recv() {
                if evt == WatcherEvent::Modified(PathBuf::from("__reconcile__")) {
                    debounce_buffer.clear();
                    let _ =
                        Self::reconcile_all(&db_pool, &content_root, &events, &mut index_failures)
                            .await;
                    reconcile_queued.store(false, Ordering::Release);
                    break;
                }
                debounce_buffer.insert(evt);
            }

            // Process the accumulated batch
            if !debounce_buffer.is_empty() {
                let batch: Vec<WatcherEvent> = debounce_buffer.drain().collect();
                info!("Processing batch of {} filesystem events...", batch.len());
                match Self::process_batch(batch, &db_pool, &content_root, &mut index_failures).await
                {
                    // Stream changed page paths to connected browsers AFTER the
                    // index commit succeeds. This is a read-only broadcast: it
                    // does not touch the Postgres index (single-writer invariant
                    // is preserved). `send` errors only when there are no
                    // subscribers, which is fine to ignore.
                    Ok(affected) => Self::broadcast_affected(&events, affected),
                    Err(e) => error!("Failed to index batch: {:?}", e),
                }
            }
        }
    }

    /// Index a batch of filesystem events. Returns the affected page paths
    /// (relative, `.md` stripped) for downstream SSE broadcast. Sole writer of
    /// the Postgres index.
    async fn process_batch(
        batch: Vec<WatcherEvent>,
        db_pool: &PgPool,
        content_root: &PathBuf,
        index_failures: &mut HashMap<PathBuf, i64>,
    ) -> Result<Vec<String>> {
        let mut to_upsert = Vec::new();
        let mut to_delete = Vec::new();

        for event in batch {
            match event {
                WatcherEvent::Modified(p) => to_upsert.push(p),
                WatcherEvent::Deleted(p) => to_delete.push(p),
            }
        }

        // Collect affected page paths (relative, `.md` stripped) so the consumer
        // can broadcast them to connected browsers after the index commit. This
        // is purely for the SSE read-side; it never writes the index.
        let mut affected: Vec<String> = Vec::new();
        let push_affected = |affected: &mut Vec<String>, path_str: &str| {
            let stripped = path_str.strip_suffix(".md").unwrap_or(path_str);
            affected.push(stripped.to_string());
        };

        // 1. Process deletions
        for path in to_delete {
            let path_str = path.to_string_lossy().to_string();
            info!("Indexing: removing page={}", path_str);

            let result = async {
                let mut tx = db_pool.begin().await?;
                sqlx::query("DELETE FROM tb_pages WHERE path = $1")
                    .bind(&path_str)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                Result::<()>::Ok(())
            }
            .await;

            match result {
                Ok(()) => {
                    index_failures.remove(&path);
                    push_affected(&mut affected, &path_str);
                }
                Err(e) => error!("Failed to remove indexed page {}: {:?}", path_str, e),
            }
        }

        // 2. Process upserts
        for path in to_upsert {
            let file_path = content_root.join(&path);
            let path_str = path.to_string_lossy().to_string();

            if !file_path.exists() {
                // If it was modified but deleted before we read it
                let result = async {
                    let mut tx = db_pool.begin().await?;
                    sqlx::query("DELETE FROM tb_pages WHERE path = $1")
                        .bind(&path_str)
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    Result::<()>::Ok(())
                }
                .await;

                match result {
                    Ok(()) => {
                        index_failures.remove(&path);
                        push_affected(&mut affected, &path_str);
                    }
                    Err(e) => error!("Failed to remove vanished page {}: {:?}", path_str, e),
                }
                continue;
            }

            let mtime = match fs::metadata(&file_path) {
                Ok(meta) => meta.modified().map_or(0, |time| {
                    time.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                }),
                Err(_) => 0,
            };

            if index_failures
                .get(&path)
                .is_some_and(|failed_mtime| *failed_mtime == mtime)
            {
                warn!("Indexing: skipping quarantined unchanged page={}", path_str);
                continue;
            }

            info!("Indexing: parsing/saving page={}", path_str);
            let raw_content = match fs::read(&file_path) {
                Ok(bytes) => decode_indexable_markdown(&bytes),
                Err(e) => {
                    error!("Failed to read file {:?}: {:?}", file_path, e);
                    index_failures.insert(path, mtime);
                    continue;
                }
            };

            let page_data = prepare_page_index_data(&path_str, &raw_content);
            let slug = Path::new(&path_str)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| path_str.clone());

            let result = async {
                let mut tx = db_pool.begin().await?;

                // Upsert tb_pages
                let row: (i64,) = sqlx::query_as(
                    "INSERT INTO tb_pages (path, slug, title, frontmatter, has_mermaid, mtime, body_tsv) \
                     VALUES ($1, $2, $3, $4, $5, $6, setweight(to_tsvector('english', COALESCE($3, '')), 'A') || setweight(to_tsvector('english', COALESCE($7, '')), 'B')) \
                     ON CONFLICT (path) \
                     DO UPDATE SET slug = EXCLUDED.slug, title = EXCLUDED.title, frontmatter = EXCLUDED.frontmatter, \
                                   has_mermaid = EXCLUDED.has_mermaid, mtime = EXCLUDED.mtime, body_tsv = EXCLUDED.body_tsv \
                     RETURNING id"
                )
                .bind(&path_str)
                .bind(&slug)
                .bind(&page_data.metadata.title)
                .bind(&page_data.frontmatter_json)
                .bind(page_data.metadata.has_mermaid)
                .bind(mtime)
                .bind(&page_data.body)
                .fetch_one(&mut *tx)
                .await?;
                let page_id = row.0;

                // Clear old links, tags, aliases
                sqlx::query("DELETE FROM tb_links WHERE src_id = $1")
                    .bind(page_id)
                    .execute(&mut *tx)
                    .await?;

                sqlx::query("DELETE FROM tb_tags WHERE page_id = $1")
                    .bind(page_id)
                    .execute(&mut *tx)
                    .await?;

                sqlx::query("DELETE FROM tb_page_aliases WHERE page_id = $1")
                    .bind(page_id)
                    .execute(&mut *tx)
                    .await?;

                // Insert tags
                for tag in &page_data.metadata.tags {
                    sqlx::query("INSERT INTO tb_tags (page_id, tag) VALUES ($1, $2) ON CONFLICT (page_id, tag) DO NOTHING")
                        .bind(page_id)
                        .bind(tag)
                        .execute(&mut *tx)
                        .await?;
                }

                // Insert aliases
                for alias in &page_data.metadata.aliases {
                    sqlx::query("INSERT INTO tb_page_aliases (page_id, alias) VALUES ($1, $2) ON CONFLICT (page_id, alias) DO NOTHING")
                        .bind(page_id)
                        .bind(alias)
                        .execute(&mut *tx)
                        .await?;
                }

                // Insert links (we'll resolve target_ids after all pages are saved/indexed)
                for link in &page_data.metadata.links {
                    let alias_opt = link.alias.as_deref();
                    sqlx::query(
                        "INSERT INTO tb_links (src_id, kind, is_embed, target, target_norm, target_id, alias) \
                         VALUES ($1, $2, $3, $4, $5, NULL, $6) \
                         ON CONFLICT (src_id, kind, target_norm, is_embed) DO NOTHING"
                    )
                    .bind(page_id)
                    .bind(&link.kind)
                    .bind(link.is_embed)
                    .bind(&link.target)
                    .bind(&link.target_norm)
                    .bind(alias_opt)
                    .execute(&mut *tx)
                    .await?;
                }

                // Resolve this page's outgoing links against pages that already
                // exist, then resolve older dangling links pointing here.
                sqlx::query(
                    "UPDATE tb_links l \
                     SET target_id = p.id \
                     FROM tb_pages p \
                     WHERE l.src_id = $1 AND l.kind = 'page' AND l.target_id IS NULL AND l.target_norm = p.slug",
                )
                .bind(page_id)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    "UPDATE tb_links SET target_id = $1 WHERE kind = 'page' AND target_norm = $2 AND target_id IS NULL"
                )
                .bind(page_id)
                .bind(&slug)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;
                Result::<()>::Ok(())
            }
            .await;

            match result {
                Ok(()) => {
                    index_failures.remove(&path);
                    push_affected(&mut affected, &path_str);
                }
                Err(e) => {
                    error!("Failed to index page {}: {:?}", path_str, e);
                    index_failures.insert(path, mtime);
                }
            }
        }
        Ok(affected)
    }

    async fn reconcile_all(
        db_pool: &PgPool,
        content_root: &Path,
        events: &broadcast::Sender<String>,
        index_failures: &mut HashMap<PathBuf, i64>,
    ) -> Result<()> {
        info!(
            "Walking content root {:?} to reconcile page database index...",
            content_root
        );
        let mut local_files = HashSet::new();

        // 1. Gather all local files
        if content_root.exists() {
            fn walk_dir(dir: &Path, files: &mut HashSet<PathBuf>) -> io::Result<()> {
                if dir.is_dir() {
                    for entry in fs::read_dir(dir)? {
                        let entry = entry?;
                        let path = entry.path();
                        // Skip dot-dirs (.trash soft-delete archive, .git, etc.).
                        let is_dot = path
                            .file_name()
                            .map_or(false, |n| n.to_string_lossy().starts_with('.'));
                        if is_dot {
                            continue;
                        }
                        if path.is_dir() {
                            walk_dir(&path, files)?;
                        } else if path.extension().map_or(false, |ext| ext == "md") {
                            // Exclude temp files
                            if !path.to_string_lossy().ends_with(".tmp") {
                                files.insert(path);
                            }
                        }
                    }
                }
                Ok(())
            }
            let _ = walk_dir(content_root, &mut local_files);
        }

        // 2. Query all database pages
        let db_pages: Vec<(String, i64)> = sqlx::query_as("SELECT path, mtime FROM tb_pages")
            .fetch_all(db_pool)
            .await?;

        let mut to_upsert = Vec::new();
        let mut to_delete = Vec::new();

        let mut db_paths_set = HashSet::new();
        for (path, mtime) in db_pages {
            let full_path = content_root.join(&path);
            db_paths_set.insert(path.clone());

            if !full_path.exists() {
                to_delete.push(WatcherEvent::Deleted(PathBuf::from(&path)));
            } else {
                let local_mtime = fs::metadata(&full_path).map_or(0, |meta| {
                    meta.modified().map_or(0, |time| {
                        time.duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0)
                    })
                });

                if local_mtime > mtime {
                    to_upsert.push(WatcherEvent::Modified(PathBuf::from(&path)));
                }
            }
        }

        // Check for new files not in database
        for file in local_files {
            if let Ok(rel_path) = file.strip_prefix(content_root) {
                let rel_path_str = rel_path.to_string_lossy().to_string();
                if !db_paths_set.contains(&rel_path_str) {
                    to_upsert.push(WatcherEvent::Modified(rel_path.to_path_buf()));
                }
            }
        }

        if !to_upsert.is_empty() || !to_delete.is_empty() {
            info!(
                "Reconcile details: {} updates, {} deletions",
                to_upsert.len(),
                to_delete.len()
            );
            let mut batch = to_upsert;
            batch.extend(to_delete);
            // The reconcile batch contains only genuinely-changed pages
            // (mtime-newer upserts, missing deletions, new files), so broadcast
            // them too. This is the sole SSE trigger under podman bind mounts,
            // where inotify events do not propagate to the watcher.
            let affected =
                Self::process_batch(batch, db_pool, &content_root.to_path_buf(), index_failures)
                    .await?;
            Self::broadcast_affected(events, affected);
        } else {
            info!("Database index is fully in sync with filesystem.");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_page_index_data_sanitizes_nul_and_invalid_utf8() {
        let raw_content =
            decode_indexable_markdown(include_bytes!("fixtures/indexer/nul_invalid.md"));
        let page_data = prepare_page_index_data("nul_invalid.md", &raw_content);
        let frontmatter =
            serde_json::to_string(&page_data.frontmatter_json).expect("frontmatter is JSON");

        assert_eq!(page_data.metadata.title, "NUL Title");
        assert!(page_data
            .body
            .contains("Body with NUL byte and invalid utf8."));
        assert!(page_data.metadata.tags.contains("nul-tag"));
        assert!(!frontmatter.contains('\0'));
        assert!(!page_data.metadata.title.contains('\0'));
        assert!(!page_data.body.contains('\0'));
        assert!(!page_data.body.contains('\u{fffd}'));
    }

    #[test]
    fn test_reconcile_interval_defaults_and_reads_env() {
        env::remove_var("MIKU_RECONCILE_INTERVAL_SECS");
        assert_eq!(IndexerQueue::reconcile_interval(), Duration::from_secs(30));

        env::set_var("MIKU_RECONCILE_INTERVAL_SECS", "45");
        assert_eq!(IndexerQueue::reconcile_interval(), Duration::from_secs(45));

        env::set_var("MIKU_RECONCILE_INTERVAL_SECS", "0");
        assert_eq!(IndexerQueue::reconcile_interval(), Duration::from_secs(30));

        env::remove_var("MIKU_RECONCILE_INTERVAL_SECS");
    }

    #[test]
    fn test_bulk_sse_threshold_defaults_and_reads_env() {
        env::remove_var("MIKU_BULK_SSE_THRESHOLD");
        assert_eq!(
            IndexerQueue::bulk_sse_threshold(),
            DEFAULT_BULK_SSE_THRESHOLD
        );

        env::set_var("MIKU_BULK_SSE_THRESHOLD", "42");
        assert_eq!(IndexerQueue::bulk_sse_threshold(), 42);

        env::set_var("MIKU_BULK_SSE_THRESHOLD", "0");
        assert_eq!(
            IndexerQueue::bulk_sse_threshold(),
            DEFAULT_BULK_SSE_THRESHOLD
        );

        env::remove_var("MIKU_BULK_SSE_THRESHOLD");
    }
}
