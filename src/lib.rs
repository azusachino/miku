//! miku — a filesystem-owned personal Markdown wiki.
//!
//! Markdown files under `miku/` are the source of truth; the Postgres index
//! is a disposable cache rebuildable from `miku/**/*.md`.

pub use anyhow::{bail, Context, Result};

pub mod indexer;
pub use miku_markdown as markdown;
