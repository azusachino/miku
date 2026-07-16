//! File-backed Markdown vault operations.
//!
//! The vault owns source files only. Search and tree indexes are projections
//! and can be rebuilt from the documents returned by this crate.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use miku_domain::workspace::{Note, NoteId, RevisionToken, WorkspaceError, WorkspaceFrontmatter};
use miku_markdown::{extract_title, parse_frontmatter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A normalized Markdown path relative to a vault root.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct VaultPath(String);

impl VaultPath {
    /// Normalizes an extensionless or `.md` relative path.
    pub fn new(path: &str) -> Result<Self, VaultError> {
        if path.starts_with('/') || path.starts_with('\\') {
            return Err(VaultError::InvalidPath(path.to_string()));
        }
        let trimmed = path.trim().trim_matches('/');
        if trimmed.is_empty() {
            return Err(VaultError::InvalidPath(path.to_string()));
        }
        let path = Path::new(trimmed);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(VaultError::InvalidPath(path.to_string_lossy().into_owned()));
        }

        let normalized = path.to_string_lossy().replace('\\', "/");
        let normalized = normalized
            .strip_suffix(".md")
            .map_or(normalized.as_str(), |value| value)
            .to_string();
        if normalized.is_empty() || normalized.ends_with('/') {
            return Err(VaultError::InvalidPath(path.to_string_lossy().into_owned()));
        }
        Ok(Self(format!("{normalized}.md")))
    }

    /// Returns the canonical relative `.md` path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A complete source document and its observed disk revision.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VaultDocument {
    /// Note metadata and stable identity.
    pub note: Note,
    /// Markdown body without the YAML frontmatter delimiters.
    pub body: String,
    /// Revision observed when the document was read.
    pub revision: RevisionToken,
    /// True when the source had no workspace `id` field and needs migration.
    pub identity_generated: bool,
}

/// One document without an explicit identity that can receive one through migration.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MigrationCandidate {
    /// Canonical source path.
    pub path: VaultPath,
    /// Deterministic identity derived from the source path.
    pub proposed_id: NoteId,
}

/// Non-destructive migration plan for documents with generated identities.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Documents missing a workspace ID.
    pub candidates: Vec<MigrationCandidate>,
    /// IDs that already occur more than once and require manual resolution.
    pub duplicate_ids: Vec<NoteId>,
}

/// Errors raised by vault path, format, and filesystem operations.
#[derive(Debug, Error)]
pub enum VaultError {
    /// A path escaped the configured vault root or was not a Markdown path.
    #[error("invalid vault path: {0}")]
    InvalidPath(String),
    /// The source document had malformed or missing required frontmatter.
    #[error("invalid Markdown frontmatter: {0}")]
    InvalidFrontmatter(String),
    /// A domain invariant rejected the document.
    #[error("workspace domain error: {0}")]
    Domain(#[from] WorkspaceError),
    /// A migration cannot proceed while explicit duplicate IDs exist.
    #[error("migration blocked by duplicate note IDs: {0:?}")]
    MigrationConflict(Vec<NoteId>),
    /// A filesystem operation failed.
    #[error("vault filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization of workspace frontmatter failed.
    #[error("frontmatter serialization failed: {0}")]
    Serialization(#[from] serde_yaml::Error),
}

/// A Markdown vault rooted at one filesystem directory.
#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Opens a vault rooted at `root`; the directory is created on first write.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the configured filesystem root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads one document without modifying a source file.
    pub fn read(&self, path: &str) -> Result<VaultDocument, VaultError> {
        let path = VaultPath::new(path)?;
        let file_path = self.file_path(&path);
        let raw = fs::read_to_string(&file_path)?;
        let metadata = fs::metadata(&file_path)?;
        parse_document(&path, &raw, modified_seconds(&metadata)?)
    }

    /// Writes one document using a flushed sibling temporary file and rename.
    pub fn write(&self, document: &VaultDocument) -> Result<RevisionToken, VaultError> {
        let path = VaultPath::new(&document.note.source_path)?;
        let raw = serialize_document(&document.note, &document.body)?;
        let file_path = self.file_path(&path);
        atomic_write(&file_path, raw.as_bytes())?;
        let metadata = fs::metadata(file_path)?;
        Ok(RevisionToken::new(
            content_hash(&raw),
            modified_seconds(&metadata)?,
        )?)
    }

    /// Creates a new document with a generated UUIDv4 identity.
    pub fn create(
        &self,
        path: &str,
        title: impl Into<String>,
        body: impl Into<String>,
        properties: BTreeMap<String, Value>,
    ) -> Result<VaultDocument, VaultError> {
        let path = VaultPath::new(path)?;
        let file_path = self.file_path(&path);
        if file_path.exists() {
            return Err(VaultError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                path.as_str().to_string(),
            )));
        }
        let note = Note::new(
            NoteId::new(Uuid::new_v4().to_string())?,
            path.as_str(),
            title,
            Vec::new(),
            None,
            properties,
        )?;
        let document = VaultDocument {
            note,
            body: body.into(),
            revision: RevisionToken::new("pending", 0)?,
            identity_generated: false,
        };
        let revision = self.write(&document)?;
        Ok(VaultDocument {
            revision,
            ..document
        })
    }

    /// Renames a source file without changing its note identity.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), VaultError> {
        let from = VaultPath::new(from)?;
        let to = VaultPath::new(to)?;
        let source = self.file_path(&from);
        let destination = self.file_path(&to);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source, destination)?;
        Ok(())
    }

    /// Scans Markdown documents recursively, excluding disposable `.miku` data.
    pub fn scan(&self) -> Result<Vec<VaultDocument>, VaultError> {
        let mut paths = Vec::new();
        collect_markdown_paths(&self.root, &self.root, &mut paths)?;
        paths.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        paths
            .into_iter()
            .map(|path| self.read(path.as_str()))
            .collect()
    }

    /// Builds a migration plan without changing any source file.
    pub fn plan_migration(&self) -> Result<MigrationPlan, VaultError> {
        let documents = self.scan()?;
        let mut candidates = Vec::new();
        let mut ids = HashMap::<NoteId, usize>::new();
        for document in documents {
            if document.identity_generated {
                candidates.push(MigrationCandidate {
                    path: VaultPath::new(&document.note.source_path)?,
                    proposed_id: path_identity(&document.note.source_path),
                });
            }
            *ids.entry(document.note.id).or_default() += 1;
        }
        let mut duplicate_ids = ids
            .into_iter()
            .filter_map(|(id, count)| (count > 1).then_some(id))
            .collect::<Vec<_>>();
        duplicate_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok(MigrationPlan {
            candidates,
            duplicate_ids,
        })
    }

    /// Applies an explicit migration plan; duplicate identities block all writes.
    pub fn apply_migration(&self, plan: &MigrationPlan) -> Result<usize, VaultError> {
        if !plan.duplicate_ids.is_empty() {
            return Err(VaultError::MigrationConflict(plan.duplicate_ids.clone()));
        }
        for candidate in &plan.candidates {
            let document = self.read(candidate.path.as_str())?;
            let note = Note {
                id: candidate.proposed_id.clone(),
                ..document.note
            };
            self.write(&VaultDocument { note, ..document })?;
        }
        Ok(plan.candidates.len())
    }

    fn file_path(&self, path: &VaultPath) -> PathBuf {
        self.root.join(path.as_str())
    }
}

fn parse_document(path: &VaultPath, raw: &str, mtime: i64) -> Result<VaultDocument, VaultError> {
    let (frontmatter, body) = parse_frontmatter(raw);
    let title = extract_title(path.as_str(), frontmatter.as_ref(), body);
    let (workspace, identity_generated, properties) = match frontmatter {
        Some(value) => {
            let workspace: WorkspaceFrontmatter = serde_json::from_value(value.clone())
                .map_err(|error| VaultError::InvalidFrontmatter(error.to_string()))?;
            let mut properties = workspace.properties.clone();
            if let Some(value) = value.get("title") {
                properties.insert("title".to_string(), value.clone());
            }
            let identity_generated = workspace.id.is_none();
            (workspace, identity_generated, properties)
        }
        None => (
            WorkspaceFrontmatter {
                id: None,
                parents: Vec::new(),
                order: None,
                properties: BTreeMap::new(),
            },
            true,
            BTreeMap::new(),
        ),
    };
    let id = workspace.id.unwrap_or_else(|| path_identity(path.as_str()));
    let note = Note::new(
        id,
        path.as_str(),
        title,
        workspace.parents,
        workspace.order,
        properties,
    )?;
    Ok(VaultDocument {
        note,
        body: body.to_string(),
        revision: RevisionToken::new(content_hash(raw), mtime)?,
        identity_generated,
    })
}

fn serialize_document(note: &Note, body: &str) -> Result<String, VaultError> {
    let mut values = Map::new();
    for (key, value) in &note.properties {
        values.insert(key.clone(), value.clone());
    }
    values.insert(
        "id".to_string(),
        Value::String(note.id.as_str().to_string()),
    );
    values.insert(
        "parents".to_string(),
        Value::Array(
            note.parents
                .iter()
                .map(|parent| Value::String(parent.as_str().to_string()))
                .collect(),
        ),
    );
    if let Some(order) = note.order {
        values.insert("order".to_string(), Value::Number(order.into()));
    }
    let yaml = serde_yaml::to_string(&Value::Object(values))?;
    Ok(format!("---\n{yaml}---\n{body}"))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_path = path.with_file_name(format!(
        ".{}.miku-tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("note.md"),
        std::process::id(),
        counter
    ));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error.into());
    }
    sync_parent(path.parent())?;
    Ok(())
}

fn sync_parent(parent: Option<&Path>) -> Result<(), VaultError> {
    if let Some(parent) = parent {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn collect_markdown_paths(
    root: &Path,
    current: &Path,
    paths: &mut Vec<VaultPath>,
) -> Result<(), VaultError> {
    if !current.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if relative
            .components()
            .any(|component| component.as_os_str() == ".miku")
        {
            continue;
        }
        if path.is_dir() {
            collect_markdown_paths(root, &path, paths)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            let relative = relative.to_string_lossy().replace('\\', "/");
            paths.push(VaultPath::new(&relative)?);
        }
    }
    Ok(())
}

fn modified_seconds(metadata: &fs::Metadata) -> Result<i64, VaultError> {
    Ok(metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64)
}

fn content_hash(contents: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn path_identity(path: &str) -> NoteId {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    NoteId::new(format!("path-{:x}", hasher.finalize())).expect("path digest is non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties() -> BTreeMap<String, Value> {
        BTreeMap::from([("title".to_string(), Value::String("Today".to_string()))])
    }

    #[test]
    fn path_normalization_rejects_escape_and_accepts_markdown_aliases() {
        assert_eq!(
            VaultPath::new("Notes/Today").unwrap().as_str(),
            "Notes/Today.md"
        );
        assert_eq!(
            VaultPath::new("Notes/Today.md").unwrap().as_str(),
            "Notes/Today.md"
        );
        assert!(matches!(
            VaultPath::new("../Today"),
            Err(VaultError::InvalidPath(_))
        ));
        assert!(matches!(
            VaultPath::new("/Today"),
            Err(VaultError::InvalidPath(_))
        ));
    }

    #[test]
    fn create_and_read_round_trip_workspace_frontmatter_atomically() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::new(root.path());
        let document = vault
            .create("Notes/Today", "Today", "# Body\n", properties())
            .unwrap();
        let loaded = vault.read("Notes/Today.md").unwrap();

        assert_eq!(loaded.note.id, document.note.id);
        assert_eq!(loaded.note.source_path, "Notes/Today.md");
        assert_eq!(loaded.body, "# Body\n");
        assert!(!loaded.identity_generated);
        assert!(loaded.revision.content_hash != "pending");
        assert!(root.path().join("Notes/Today.md").exists());
    }

    #[test]
    fn rename_preserves_note_identity() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::new(root.path());
        let created = vault.create("Old", "Old", "body", BTreeMap::new()).unwrap();
        vault.rename("Old", "New").unwrap();
        let renamed = vault.read("New").unwrap();

        assert_eq!(renamed.note.id, created.note.id);
        assert_eq!(renamed.note.source_path, "New.md");
        assert!(!root.path().join("Old.md").exists());
    }

    #[test]
    fn generated_identity_scan_and_explicit_migration_do_not_rewrite_during_planning() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Legacy.md");
        fs::write(&path, "---\ntitle: Legacy\n---\nbody\n").unwrap();
        let vault = Vault::new(root.path());

        let before = fs::read_to_string(&path).unwrap();
        let plan = vault.plan_migration().unwrap();
        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), before);

        assert_eq!(vault.apply_migration(&plan).unwrap(), 1);
        let migrated = vault.read("Legacy").unwrap();
        assert!(!migrated.identity_generated);
        assert_eq!(migrated.note.id, plan.candidates[0].proposed_id);
    }

    #[test]
    fn duplicate_ids_block_migration() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("A.md"), "---\nid: same\n---\na\n").unwrap();
        fs::write(root.path().join("B.md"), "---\nid: same\n---\nb\n").unwrap();
        let vault = Vault::new(root.path());
        let plan = vault.plan_migration().unwrap();

        assert_eq!(plan.duplicate_ids, vec![NoteId::new("same").unwrap()]);
        assert!(matches!(
            vault.apply_migration(&plan),
            Err(VaultError::MigrationConflict(_))
        ));
    }
}
