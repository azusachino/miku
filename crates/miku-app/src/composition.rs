//! Runtime composition for durable and hot projections.

use async_trait::async_trait;
use miku_domain::{
    Backlink, DurableProjection, HotProjection, IndexCapabilities, IndexEvent, IndexReader,
    IndexWriter, MentionRecord, PageIndex, PageSummary, SearchHit, SearchRequest, StoreResult,
    TagCount,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Compose a durable projection with a rebuildable hot projection.
pub fn compose_projections<D, H>(durable: Arc<D>, hot: Arc<H>) -> super::IndexApi
where
    D: DurableProjection + 'static,
    H: HotProjection + 'static,
{
    let ready = Arc::new(AtomicBool::new(false));
    super::IndexApi::from_parts(
        Arc::new(ComposedReader {
            durable: durable.clone(),
            hot: hot.clone(),
            ready: ready.clone(),
        }),
        Arc::new(ComposedWriter {
            durable,
            hot,
            ready,
        }),
    )
}

struct ComposedReader {
    durable: Arc<dyn DurableProjection>,
    hot: Arc<dyn HotProjection>,
    ready: Arc<AtomicBool>,
}

impl ComposedReader {
    fn active(&self) -> &dyn IndexReader {
        if self.ready.load(Ordering::Acquire) {
            self.hot.as_ref()
        } else {
            self.durable.as_ref()
        }
    }
}

#[async_trait]
impl IndexReader for ComposedReader {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        let durable = self.durable.capabilities().await?;
        let hot = self.hot.capabilities().await?;
        Ok(IndexCapabilities {
            durable: durable.durable,
            full_text_search: durable.full_text_search || hot.full_text_search,
            fuzzy_page_search: durable.fuzzy_page_search || hot.fuzzy_page_search,
            transactions: durable.transactions && hot.transactions,
            remote_sync: durable.remote_sync,
        })
    }

    async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        self.active().list_pages().await
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        self.active().page(path).await
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        self.active().search(request).await
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        self.active().backlinks(path).await
    }

    async fn mentions_for_target(&self, path: &str) -> StoreResult<Vec<MentionRecord>> {
        self.active().mentions_for_target(path).await
    }

    async fn mentions_ready(&self) -> StoreResult<bool> {
        self.active().mentions_ready().await
    }

    async fn index_metadata(&self, key: &str) -> StoreResult<Option<String>> {
        self.durable.index_metadata(key).await
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        self.active().tags().await
    }

    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
        self.active().pages_with_tag(tag).await
    }
}

struct ComposedWriter {
    durable: Arc<dyn DurableProjection>,
    hot: Arc<dyn HotProjection>,
    ready: Arc<AtomicBool>,
}

impl ComposedWriter {
    fn degrade(&self) {
        self.ready.store(false, Ordering::Release);
    }

    async fn hot_result<T>(&self, result: StoreResult<T>) -> StoreResult<T> {
        if result.is_err() {
            self.degrade();
        }
        result
    }
}

#[async_trait]
impl IndexWriter for ComposedWriter {
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        let event = self.durable.replace_page(page.clone()).await?;
        self.hot_result(self.hot.replace_page(page).await).await?;
        Ok(event)
    }

    async fn replace_pages(&self, pages: Vec<PageIndex>) -> StoreResult<Vec<IndexEvent>> {
        let events = self.durable.replace_pages(pages.clone()).await?;
        self.hot_result(self.hot.replace_pages(pages).await).await?;
        Ok(events)
    }

    async fn rebuild_search_index(&self) -> StoreResult<()> {
        self.durable.rebuild_search_index().await?;
        self.hot_result(self.hot.rebuild_search_index().await).await
    }

    async fn replace_mentions_for_source(
        &self,
        source_path: &str,
        mentions: Vec<MentionRecord>,
    ) -> StoreResult<()> {
        self.durable
            .replace_mentions_for_source(source_path, mentions.clone())
            .await?;
        self.hot_result(
            self.hot
                .replace_mentions_for_source(source_path, mentions)
                .await,
        )
        .await
    }

    async fn replace_mentions_for_sources(
        &self,
        entries: Vec<(String, Vec<MentionRecord>)>,
    ) -> StoreResult<()> {
        self.durable
            .replace_mentions_for_sources(entries.clone())
            .await?;
        self.hot_result(self.hot.replace_mentions_for_sources(entries).await)
            .await
    }

    async fn delete_mentions_for_source(&self, source_path: &str) -> StoreResult<()> {
        self.durable.delete_mentions_for_source(source_path).await?;
        self.hot_result(self.hot.delete_mentions_for_source(source_path).await)
            .await
    }

    async fn delete_mentions_for_target(&self, target_path: &str) -> StoreResult<()> {
        self.durable.delete_mentions_for_target(target_path).await?;
        self.hot_result(self.hot.delete_mentions_for_target(target_path).await)
            .await
    }

    async fn delete_mentions_for_targets(&self, target_paths: Vec<String>) -> StoreResult<()> {
        self.durable
            .delete_mentions_for_targets(target_paths.clone())
            .await?;
        self.hot_result(self.hot.delete_mentions_for_targets(target_paths).await)
            .await
    }

    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        self.durable.mark_mentions_ready().await?;
        self.hot.mark_mentions_ready().await?;
        self.ready.store(true, Ordering::Release);
        Ok(())
    }

    async fn set_index_metadata(&self, key: &str, value: &str) -> StoreResult<()> {
        self.durable.set_index_metadata(key, value).await
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        let event = self.durable.delete_page(path).await?;
        self.hot_result(self.hot.delete_page(path).await).await?;
        Ok(event)
    }
}
