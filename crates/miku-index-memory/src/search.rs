//! Tantivy-backed disposable full-text projection.

use miku_domain::{PageIndex, SearchHit, SearchRequest, SearchScope, StoreError, StoreResult};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, Value, STORED, TEXT};
use tantivy::{doc, Index, IndexReader, ReloadPolicy, TantivyDocument};

/// Rebuildable Tantivy search projection owned by [`super::MemoryIndex`].
pub struct SearchProjection {
    index: Index,
    reader: IndexReader,
    path: tantivy::schema::Field,
    title: tantivy::schema::Field,
    body: tantivy::schema::Field,
}

impl SearchProjection {
    /// Creates an empty in-memory Tantivy index.
    pub fn new() -> StoreResult<Self> {
        let mut schema_builder = Schema::builder();
        let path = schema_builder.add_text_field("path", TEXT | STORED);
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let body = schema_builder.add_text_field("body", TEXT | STORED);
        let index = Index::create_in_ram(schema_builder.build());
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|error| StoreError::Operation(format!("tantivy reader: {error}")))?;
        Ok(Self {
            index,
            reader,
            path,
            title,
            body,
        })
    }

    /// Deletes all search documents and rebuilds from the current page graph.
    pub fn rebuild(&mut self, pages: &[PageIndex]) -> StoreResult<()> {
        let mut writer = self
            .index
            .writer(15_000_000)
            .map_err(|error| StoreError::Operation(format!("tantivy writer: {error}")))?;
        writer
            .delete_all_documents()
            .map_err(|error| StoreError::Operation(format!("tantivy clear: {error}")))?;
        for page in pages {
            let _ = writer.add_document(doc!(
                self.path => page.summary.path.clone(),
                self.title => page.summary.title.clone(),
                self.body => page.body.clone(),
            ));
        }
        writer
            .commit()
            .map_err(|error| StoreError::Operation(format!("tantivy commit: {error}")))?;
        self.reader
            .reload()
            .map_err(|error| StoreError::Operation(format!("tantivy reload: {error}")))?;
        Ok(())
    }

    /// Searches the projection according to the backend-neutral request.
    pub fn search(&self, request: &SearchRequest) -> StoreResult<Vec<SearchHit>> {
        if request.query.trim().is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }
        let fields = match request.scope {
            SearchScope::All => vec![self.path, self.title, self.body],
            SearchScope::Title => vec![self.path, self.title],
            SearchScope::Body => vec![self.body],
        };
        let parser = QueryParser::for_index(&self.index, fields);
        let query = parser
            .parse_query(&request.query)
            .map_err(|error| StoreError::InvalidInput(format!("invalid search query: {error}")))?;
        let searcher = self.reader.searcher();
        let docs = searcher
            .search(&query, &TopDocs::with_limit(request.limit))
            .map_err(|error| StoreError::Operation(format!("tantivy search: {error}")))?;
        docs.into_iter()
            .map(|(_, address)| {
                let document = searcher
                    .doc::<TantivyDocument>(address)
                    .map_err(|error| StoreError::Operation(format!("tantivy document: {error}")))?;
                let path = document
                    .get_first(self.path)
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        StoreError::Operation("tantivy document missing path".to_string())
                    })?;
                let title = document
                    .get_first(self.title)
                    .and_then(|value| value.as_str())
                    .unwrap_or(path);
                let body = document
                    .get_first(self.body)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                Ok(SearchHit {
                    path: path.to_string(),
                    title: title.to_string(),
                    snippet: snippet(body, &request.query),
                })
            })
            .collect()
    }
}

fn snippet(body: &str, query: &str) -> String {
    let lower = body.to_lowercase();
    let start = query
        .split_whitespace()
        .find_map(|term| lower.find(&term.to_lowercase()))
        .unwrap_or(0);
    let start_chars = lower[..start].chars().count();
    body.chars().skip(start_chars).take(160).collect()
}
