use anyhow::Result;
use miku_domain::{IndexReader, IndexWriter, PageIndex, PageSummary};
use miku_indexer::build_page_index;
use notify::Watcher;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::{error, info};

const RECONCILE_SENTINEL: &str = "__reconcile__";
const BULK_INDEX_REFRESH: &str = "__miku_bulk_index_refresh__";
const DEFAULT_RECONCILE_BATCH_SIZE: usize = 512;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum WatcherEvent {
    Modified(PathBuf),
    Deleted(PathBuf),
}

pub struct IndexerQueue {
    sender: mpsc::Sender<WatcherEvent>,
    reconcile_queued: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    _watcher: notify::RecommendedWatcher,
    tasks: Vec<JoinHandle<()>>,
}

async fn index_store_file(
    writer: &Arc<dyn IndexWriter>,
    content_root: &Path,
    events: &broadcast::Sender<String>,
    relative: &Path,
) -> miku_domain::StoreResult<()> {
    let path = relative.to_string_lossy().to_string();
    let file = content_root.join(relative);
    if !file.exists() {
        writer.delete_page(&path).await?;
        let _ = events.send(path.strip_suffix(".md").unwrap_or(&path).to_string());
        return Ok(());
    }

    let bytes =
        fs::read(&file).map_err(|error| miku_domain::StoreError::Operation(error.to_string()))?;
    let mtime = fs::metadata(&file)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64);
    writer
        .replace_page(build_page_index(&path, &bytes, mtime))
        .await?;
    let _ = events.send(path.strip_suffix(".md").unwrap_or(&path).to_string());
    Ok(())
}

async fn reconcile_store(
    reader: &Arc<dyn IndexReader>,
    writer: &Arc<dyn IndexWriter>,
    content_root: &Path,
    events: &broadcast::Sender<String>,
) -> miku_domain::StoreResult<()> {
    let reconcile_started = Instant::now();
    let walk_started = Instant::now();
    let mut files = Vec::new();
    walk_store_tree(content_root, &mut files)
        .map_err(|error| miku_domain::StoreError::Operation(error.to_string()))?;
    let walk_duration = walk_started.elapsed();
    let existing_started = Instant::now();
    let existing = reader
        .list_pages()
        .await?
        .into_iter()
        .map(|page| (page.path.clone(), page))
        .collect::<HashMap<String, PageSummary>>();
    let existing_duration = existing_started.elapsed();
    let scanned_files = files.len();
    let mut seen = HashSet::with_capacity(files.len());
    let batch_size = IndexerQueue::reconcile_batch_size();
    let mut pages = Vec::with_capacity(batch_size);
    let mut indexed_pages = 0usize;
    let mut unchanged_pages = 0usize;
    let mut batches = 0usize;
    let mut parse_duration = Duration::ZERO;
    let mut write_duration = Duration::ZERO;
    let mut metadata_duration = Duration::ZERO;
    for file in files {
        let relative = file
            .strip_prefix(content_root)
            .map_err(|error| miku_domain::StoreError::Operation(error.to_string()))?;
        let metadata_started = Instant::now();
        let metadata = fs::metadata(&file)
            .map_err(|error| miku_domain::StoreError::Operation(error.to_string()))?;
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs() as i64);
        metadata_duration += metadata_started.elapsed();
        let path = relative.to_string_lossy().into_owned();
        seen.insert(path.clone());
        if existing
            .get(&path)
            .is_some_and(|indexed| indexed.mtime == mtime)
        {
            unchanged_pages += 1;
            continue;
        }
        let parse_started = Instant::now();
        let bytes = fs::read(&file)
            .map_err(|error| miku_domain::StoreError::Operation(error.to_string()))?;
        pages.push(build_page_index(&relative.to_string_lossy(), &bytes, mtime));
        parse_duration += parse_started.elapsed();
        if pages.len() == batch_size {
            let batch = std::mem::take(&mut pages);
            indexed_pages += batch.len();
            batches += 1;
            write_duration += flush_reconcile_batch(writer, events, batch, batches).await?;
        }
    }
    if !pages.is_empty() {
        indexed_pages += pages.len();
        batches += 1;
        write_duration += flush_reconcile_batch(writer, events, pages, batches).await?;
    }
    let mut deleted = false;
    let mut deleted_pages = 0usize;
    for path in existing.keys().filter(|path| !seen.contains(*path)) {
        writer.delete_page(path).await?;
        deleted = true;
        deleted_pages += 1;
    }
    if deleted {
        let _ = events.send(BULK_INDEX_REFRESH.to_string());
    }
    info!(
        scanned_files,
        indexed_pages,
        unchanged_pages,
        deleted_pages,
        batches,
        walk_ms = walk_duration.as_secs_f64() * 1000.0,
        existing_ms = existing_duration.as_secs_f64() * 1000.0,
        metadata_ms = metadata_duration.as_secs_f64() * 1000.0,
        parse_ms = parse_duration.as_secs_f64() * 1000.0,
        write_ms = write_duration.as_secs_f64() * 1000.0,
        total_ms = reconcile_started.elapsed().as_secs_f64() * 1000.0,
        "index reconcile finished"
    );
    Ok(())
}

async fn flush_reconcile_batch(
    writer: &Arc<dyn IndexWriter>,
    events: &broadcast::Sender<String>,
    pages: Vec<PageIndex>,
    batch_number: usize,
) -> miku_domain::StoreResult<Duration> {
    let page_count = pages.len();
    let started = Instant::now();
    writer.replace_pages(pages).await?;
    let _ = events.send(BULK_INDEX_REFRESH.to_string());
    let elapsed = started.elapsed();
    info!(
        batch_number,
        page_count,
        write_ms = elapsed.as_secs_f64() * 1000.0,
        "index reconcile batch committed"
    );
    Ok(elapsed)
}

fn walk_store_tree(root: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            walk_store_tree(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    files.sort();
    Ok(())
}

impl IndexerQueue {
    /// Start the backend-neutral filesystem indexer.
    pub fn new_with_writer(
        reader: Arc<dyn IndexReader>,
        writer: Arc<dyn IndexWriter>,
        content_root: PathBuf,
        events: broadcast::Sender<String>,
    ) -> Result<Self> {
        if !content_root.exists() {
            fs::create_dir_all(&content_root)?;
        }

        let (sender, mut receiver) = mpsc::channel(1024);
        let reconcile_queued = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let writer_task = Arc::clone(&writer);
        let reader_task = Arc::clone(&reader);
        let root_task = content_root.clone();
        let events_task = events.clone();
        let reconcile_flag = Arc::clone(&reconcile_queued);
        let ready_flag = Arc::clone(&ready);
        let consumer_task = tokio::spawn(async move {
            let startup_started = Instant::now();
            let startup_result =
                reconcile_store(&reader_task, &writer_task, &root_task, &events_task).await;
            if let Err(error) = startup_result {
                error!(
                    ?error,
                    elapsed_ms = startup_started.elapsed().as_secs_f64() * 1000.0,
                    "startup index reconcile failed"
                );
            } else {
                ready_flag.store(true, Ordering::Release);
                info!(
                    elapsed_ms = startup_started.elapsed().as_secs_f64() * 1000.0,
                    "startup index reconcile ready"
                );
            }
            while let Some(event) = receiver.recv().await {
                if event == WatcherEvent::Modified(PathBuf::from(RECONCILE_SENTINEL)) {
                    if let Err(error) =
                        reconcile_store(&reader_task, &writer_task, &root_task, &events_task).await
                    {
                        error!(?error, "periodic index reconcile failed");
                    }
                    reconcile_flag.store(false, Ordering::Release);
                    continue;
                }

                let result = match event {
                    WatcherEvent::Modified(path) | WatcherEvent::Deleted(path) => {
                        index_store_file(&writer_task, &root_task, &events_task, &path).await
                    }
                };
                if let Err(error) = result {
                    error!(?error, "index update failed");
                }
            }
        });

        let sender_for_watcher = sender.clone();
        let root_for_watcher = content_root.clone();
        let mut watcher = notify::RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| match result {
                Ok(event) => {
                    for path in event.paths {
                        if path.extension().is_some_and(|extension| extension == "md")
                            && !path.to_string_lossy().ends_with(".tmp")
                        {
                            let Some(relative) = path.strip_prefix(&root_for_watcher).ok() else {
                                continue;
                            };
                            let _ = sender_for_watcher.try_send(if path.exists() {
                                WatcherEvent::Modified(relative.to_path_buf())
                            } else {
                                WatcherEvent::Deleted(relative.to_path_buf())
                            });
                        }
                    }
                }
                Err(error) => error!(?error, "filesystem watcher failed"),
            },
            notify::Config::default(),
        )?;
        watcher.watch(&content_root, notify::RecursiveMode::Recursive)?;

        let reconcile_sender = sender.clone();
        let reconcile_flag = Arc::clone(&reconcile_queued);
        let ticker_task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Self::reconcile_interval());
            ticker.tick().await;
            loop {
                ticker.tick().await;
                Self::try_queue_reconcile(&reconcile_sender, &reconcile_flag);
            }
        });

        Ok(Self {
            sender,
            reconcile_queued,
            ready,
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
        Self::try_queue_reconcile(&self.sender, &self.reconcile_queued);
    }

    #[must_use]
    pub fn ready_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.ready)
    }

    fn reconcile_interval() -> Duration {
        env::var("MIKU_RECONCILE_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .map_or_else(|| Duration::from_secs(30), Duration::from_secs)
    }

    fn reconcile_batch_size() -> usize {
        env::var("MIKU_RECONCILE_BATCH_SIZE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|size| *size > 0)
            .unwrap_or(DEFAULT_RECONCILE_BATCH_SIZE)
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
        if sender
            .try_send(WatcherEvent::Modified(PathBuf::from(RECONCILE_SENTINEL)))
            .is_err()
        {
            reconcile_queued.store(false, Ordering::Release);
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconcile_interval_defaults_and_reads_env() {
        env::remove_var("MIKU_RECONCILE_INTERVAL_SECS");
        assert_eq!(IndexerQueue::reconcile_interval(), Duration::from_secs(30));
        env::set_var("MIKU_RECONCILE_INTERVAL_SECS", "45");
        assert_eq!(IndexerQueue::reconcile_interval(), Duration::from_secs(45));
        env::remove_var("MIKU_RECONCILE_INTERVAL_SECS");
        env::remove_var("MIKU_RECONCILE_BATCH_SIZE");
        assert_eq!(IndexerQueue::reconcile_batch_size(), 512);
        env::set_var("MIKU_RECONCILE_BATCH_SIZE", "1000");
        assert_eq!(IndexerQueue::reconcile_batch_size(), 1000);
        env::remove_var("MIKU_RECONCILE_BATCH_SIZE");
    }
}
