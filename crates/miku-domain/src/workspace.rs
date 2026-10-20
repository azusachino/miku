//! Domain invariants for the file-backed Markdown workspace.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Stable identity of note content, independent of where it is shown.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NoteId(String);

impl NoteId {
    /// Creates an opaque, non-empty note identity.
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkspaceError::EmptyIdentifier { kind: "note_id" });
        }
        Ok(Self(value))
    }

    /// Returns the serialized identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// File revision used for optimistic conflict checks.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RevisionToken {
    /// Content digest calculated from the source file.
    pub content_hash: String,
    /// Source file modification time as Unix seconds.
    pub mtime: i64,
}

impl RevisionToken {
    /// Creates a revision token with a non-empty content digest.
    pub fn new(content_hash: impl Into<String>, mtime: i64) -> Result<Self, WorkspaceError> {
        let content_hash = content_hash.into();
        if content_hash.trim().is_empty() {
            return Err(WorkspaceError::EmptyRevisionHash);
        }
        Ok(Self {
            content_hash,
            mtime,
        })
    }
}

/// Parsed workspace-owned frontmatter fields plus user-defined properties.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceFrontmatter {
    /// Stable note identity, absent only before migration assigns one.
    pub id: Option<NoteId>,
    /// Frontmatter fields not owned by the workspace model, including any
    /// legacy `parents` or `order` keys (ADR-0024), kept verbatim.
    #[serde(flatten)]
    pub properties: BTreeMap<String, Value>,
}

/// File-backed note metadata used by the workspace projection.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Stable content identity.
    pub id: NoteId,
    /// Path relative to the configured Markdown vault.
    pub source_path: String,
    /// Display title.
    pub title: String,
    /// User-defined frontmatter retained by the domain.
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

impl Note {
    /// Creates note metadata after validating its source path.
    pub fn new(
        id: NoteId,
        source_path: impl Into<String>,
        title: impl Into<String>,
        properties: BTreeMap<String, Value>,
    ) -> Result<Self, WorkspaceError> {
        let source_path = source_path.into();
        if source_path.trim().is_empty() {
            return Err(WorkspaceError::EmptySourcePath);
        }
        Ok(Self {
            id,
            source_path,
            title: title.into(),
            properties,
        })
    }
}

/// Mutations subject to the readonly policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum MutationAction {
    /// Create a new Markdown note.
    CreateNote,
    /// Change Markdown or frontmatter content.
    EditNote,
    /// Rename a source file.
    RenameNote,
    /// Hide a note without deleting its source.
    ArchiveNote,
    /// Delete a note.
    DeleteNote,
}

/// Errors raised when workspace invariants are violated.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Error)]
pub enum WorkspaceError {
    /// A typed identity was empty or whitespace-only.
    #[error("{kind} cannot be empty")]
    EmptyIdentifier { kind: &'static str },
    /// A source path was empty or whitespace-only.
    #[error("source_path cannot be empty")]
    EmptySourcePath,
    /// A revision did not contain a digest.
    #[error("revision content_hash cannot be empty")]
    EmptyRevisionHash,
    /// Readonly mode rejects every mutation action.
    #[error("readonly workspace rejects mutation: {0:?}")]
    ReadonlyMutation(MutationAction),
}

/// Authorizes a mutation against the workspace's readonly setting.
pub fn authorize_mutation(readonly: bool, action: MutationAction) -> Result<(), WorkspaceError> {
    if readonly {
        Err(WorkspaceError::ReadonlyMutation(action))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_id(value: &str) -> NoteId {
        NoteId::new(value).expect("test note id")
    }

    #[test]
    fn note_requires_a_source_path() {
        assert!(Note::new(note_id("n"), "note.md", "Note", BTreeMap::new()).is_ok());
        assert_eq!(
            Note::new(note_id("n"), "  ", "Note", BTreeMap::new()),
            Err(WorkspaceError::EmptySourcePath)
        );
    }

    #[test]
    fn identifiers_and_revision_require_values() {
        assert!(matches!(
            NoteId::new("  "),
            Err(WorkspaceError::EmptyIdentifier { .. })
        ));
        assert!(matches!(
            RevisionToken::new("", 1),
            Err(WorkspaceError::EmptyRevisionHash)
        ));
    }

    #[test]
    fn frontmatter_keeps_legacy_parents_and_order_as_plain_properties() {
        let json = serde_json::json!({
            "id": "note-1",
            "parents": ["root"],
            "order": 2,
            "custom": "kept"
        });
        let frontmatter: WorkspaceFrontmatter = serde_json::from_value(json).expect("frontmatter");

        assert_eq!(frontmatter.id, Some(note_id("note-1")));
        assert_eq!(
            frontmatter.properties["parents"],
            serde_json::json!(["root"])
        );
        assert_eq!(frontmatter.properties["order"], serde_json::json!(2));
        assert_eq!(
            frontmatter.properties["custom"],
            Value::String("kept".into())
        );
    }

    #[test]
    fn readonly_rejects_every_mutation_action() {
        let actions = [
            MutationAction::CreateNote,
            MutationAction::EditNote,
            MutationAction::RenameNote,
            MutationAction::ArchiveNote,
            MutationAction::DeleteNote,
        ];

        for action in actions {
            assert_eq!(
                authorize_mutation(true, action),
                Err(WorkspaceError::ReadonlyMutation(action))
            );
            assert_eq!(authorize_mutation(false, action), Ok(()));
        }
    }
}
