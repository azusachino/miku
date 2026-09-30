//! Baseline for `note_context` on a small fixture vault.
//!
//! Files are written to disk as a user would write them, indexed through the
//! production `miku_indexer::build_page_index`, and served by
//! `FileMikuApplication`. Every backend must give the same answers, and each
//! backend is also rebuilt from the same files to check that a fresh
//! projection answers identically (ADR-0023 goal 4). Parents and children
//! come from folder paths (ADR-0024); frontmatter `parents` is an ordinary
//! property with no effect on the hierarchy.

use std::path::Path;
use std::sync::Arc;

use miku_app::{
    FileMikuApplication, FileWorkspaceService, IndexApi, NoteContext, NotePath, NoteRef,
    VaultReader, WorkspaceService,
};
use miku_domain::PageIndex;
use miku_index_memory::MemoryIndex;
use miku_vault::Vault;
use tempfile::TempDir;

/// The fixture vault: relative path and raw file contents.
const FIXTURE: &[(&str, &str)] = &[
    ("index.md", "# Home\n\nStart here.\n"),
    (
        "projects/index.md",
        "---\nid: note-projects\ntitle: Projects\n---\n# Projects\n\nHub for [[alpha]] work.\n",
    ),
    (
        "projects/alpha.md",
        "---\nid: note-alpha\ntitle: Alpha\naliases:\n  - Alpha Project\nparents:\n  - note-projects\ntags:\n  - work\n---\n# Alpha\n\nDepends on [[beta]] and [the beta doc](beta.md). See [[Missing Page]].\n",
    ),
    (
        "projects/beta.md",
        "---\nid: note-beta\ntitle: Beta\n---\n# Beta\n\nBack to [[Alpha Project]].\n",
    ),
    (
        "ideas/gamma.md",
        "---\nid: note-gamma\ntitle: Gamma\nparents:\n  - note-alpha\n---\n# Gamma\n\nAn idea placed under [[alpha]] by frontmatter.\n",
    ),
    ("inbox.md", "# Inbox\n\nCapture for [[projects/alpha]].\n"),
];

/// The parts of a `NoteContext` this baseline pins. Parents keep their
/// root-to-nearest order; the other sets are sorted so the comparison does
/// not depend on backend iteration order.
#[derive(Debug, PartialEq, Eq)]
struct Summary {
    parents: Vec<String>,
    children: Vec<String>,
    backlinks: Vec<String>,
    /// `(target as written, resolved path, is_missing)` in document order.
    outgoing: Vec<(String, String, bool)>,
}

fn summarize(context: &NoteContext) -> Summary {
    let parents: Vec<String> = context
        .parents
        .iter()
        .map(|node| node.path.as_str().to_string())
        .collect();
    let mut children: Vec<String> = context
        .children
        .iter()
        .map(|node| node.path.as_str().to_string())
        .collect();
    children.sort();
    let mut backlinks: Vec<String> = context
        .backlinks
        .iter()
        .map(|backlink| backlink.path.clone())
        .collect();
    backlinks.sort();
    let outgoing = context
        .outgoing
        .iter()
        .map(|link| (link.target.clone(), link.path.clone(), link.is_missing))
        .collect();
    Summary {
        parents,
        children,
        backlinks,
        outgoing,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn link(target: &str, path: &str, is_missing: bool) -> (String, String, bool) {
    (target.to_string(), path.to_string(), is_missing)
}

fn write_fixture(root: &Path) {
    for (path, contents) in FIXTURE {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("fixture parent")).expect("create folder");
        std::fs::write(file, contents).expect("write fixture file");
    }
}

/// Index every fixture file the way the production reconciler does.
fn page_indexes() -> Vec<PageIndex> {
    FIXTURE
        .iter()
        .map(|(path, contents)| miku_indexer::build_page_index(path, contents.as_bytes(), 1))
        .collect()
}

fn application(root: &Path, index: IndexApi) -> FileMikuApplication {
    let vault = Arc::new(Vault::new(root));
    let workspace: Arc<dyn WorkspaceService> =
        Arc::new(FileWorkspaceService::new(Arc::clone(&vault), true));
    FileMikuApplication::new(vault, workspace, index)
}

async fn memory_index() -> IndexApi {
    let index = IndexApi::from_store(Arc::new(MemoryIndex::new()));
    let writer = index.writer();
    writer.replace_pages(page_indexes()).await.expect("index");
    writer.rebuild_search_index().await.expect("rebuild");
    index
}

#[cfg(feature = "sqlite")]
async fn composed_index(dir: &Path) -> IndexApi {
    let path = dir.join("index.sqlite");
    let durable = Arc::new(
        miku_index_sqlite::SqliteIndex::open(path.to_str().expect("utf-8 path"))
            .await
            .expect("open sqlite"),
    );
    let index = miku_app::compose_projections(durable, Arc::new(MemoryIndex::new()));
    let writer = index.writer();
    writer.replace_pages(page_indexes()).await.expect("index");
    writer.rebuild_search_index().await.expect("rebuild");
    index
}

async fn context(application: &FileMikuApplication, path: &str) -> Summary {
    let context = application
        .note_context(NoteRef::Path(NotePath::new(path).expect("note path")))
        .await
        .expect("note context");
    summarize(&context)
}

/// Answers every backend must give.
async fn assert_context_answers(application: &FileMikuApplication) {
    let alpha = context(application, "projects/alpha.md").await;
    assert_eq!(
        alpha.parents,
        strings(&["index.md", "projects/index.md"]),
        "alpha parents"
    );
    assert!(alpha.children.is_empty(), "alpha children");
    assert_eq!(
        alpha.backlinks,
        strings(&[
            "ideas/gamma.md",
            "inbox.md",
            "projects/beta.md",
            "projects/index.md"
        ]),
        "alpha backlinks"
    );
    // Outgoing links are deduplicated by folded target text, and folding
    // strips `.md`, so the Markdown link `beta.md` shares `[[beta]]`'s entry.
    assert_eq!(
        alpha.outgoing,
        vec![
            link("beta", "projects/beta.md", false),
            link("Missing Page", "projects/Missing Page.md", true),
        ],
        "alpha outgoing"
    );

    // Gamma's frontmatter `parents: [note-alpha]` is a plain property now:
    // its parent is its folder chain, which has only the root index.
    let gamma = context(application, "ideas/gamma.md").await;
    assert_eq!(gamma.parents, strings(&["index.md"]), "gamma parents");
    assert!(gamma.children.is_empty(), "gamma children");

    let projects = context(application, "projects/index.md").await;
    assert_eq!(projects.parents, strings(&["index.md"]), "projects parents");
    assert_eq!(
        projects.children,
        strings(&["projects/alpha.md", "projects/beta.md"]),
        "projects children"
    );

    let home = context(application, "index.md").await;
    assert!(home.parents.is_empty(), "home parents");
    assert_eq!(
        home.children,
        strings(&["ideas", "inbox.md", "projects"]),
        "home children"
    );

    let beta = context(application, "projects/beta.md").await;
    assert_eq!(
        beta.backlinks,
        strings(&["projects/alpha.md"]),
        "beta backlinks"
    );
    assert_eq!(
        beta.outgoing,
        vec![link("Alpha Project", "projects/alpha.md", false)],
        "beta outgoing"
    );

    let inbox = context(application, "inbox.md").await;
    assert_eq!(inbox.parents, strings(&["index.md"]), "inbox parents");
    assert!(inbox.backlinks.is_empty(), "inbox backlinks");
    // A path-qualified target resolves by vault-root path, the same rule the
    // backlink graph uses (alpha's backlinks above include inbox.md).
    assert_eq!(
        inbox.outgoing,
        vec![link("projects/alpha", "projects/alpha.md", false)],
        "inbox outgoing"
    );
}

async fn all_contexts(application: &FileMikuApplication) -> Vec<Summary> {
    let mut summaries = Vec::new();
    for (path, _) in FIXTURE {
        summaries.push(context(application, path).await);
    }
    summaries
}

#[tokio::test]
async fn memory_backend_note_context_baseline() {
    let root = TempDir::new().expect("vault dir");
    write_fixture(root.path());
    let application = application(root.path(), memory_index().await);

    assert_context_answers(&application).await;

    let rebuilt = self::application(root.path(), memory_index().await);
    assert_eq!(
        all_contexts(&application).await,
        all_contexts(&rebuilt).await,
        "a rebuilt memory index answers identically"
    );
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn composed_sqlite_backend_note_context_baseline() {
    let root = TempDir::new().expect("vault dir");
    write_fixture(root.path());
    let first = TempDir::new().expect("index dir");
    let application = application(root.path(), composed_index(first.path()).await);

    assert_context_answers(&application).await;

    let second = TempDir::new().expect("rebuilt index dir");
    let rebuilt = self::application(root.path(), composed_index(second.path()).await);
    assert_eq!(
        all_contexts(&application).await,
        all_contexts(&rebuilt).await,
        "a rebuilt composed index answers identically"
    );
}
