//! miku — a filesystem-owned personal Markdown wiki.
//!
//! Markdown files under `miku_docs/` are the source of truth; the Postgres index
//! is a disposable cache rebuildable from `miku_docs/**/*.md`.

pub use anyhow::{bail, Context, Result};

pub mod indexer;
pub use miku_markdown as markdown;
pub mod content_search;
