//! Domain invariants for the file-backed Markdown workspace.

use std::collections::{BTreeMap, HashSet};

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

/// Stable identity of one note placement in the workspace tree.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlacementId(String);

impl PlacementId {
    /// Creates an opaque, non-empty placement identity.
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkspaceError::EmptyIdentifier {
                kind: "placement_id",
            });
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
    /// Ordered parent note identities for tree placement.
    #[serde(default)]
    pub parents: Vec<NoteId>,
    /// Optional sibling ordering value.
    pub order: Option<i64>,
    /// Frontmatter fields not owned by the workspace model.
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
    /// Ordered parent references from workspace frontmatter.
    #[serde(default)]
    pub parents: Vec<NoteId>,
    /// Optional sibling ordering value from workspace frontmatter.
    pub order: Option<i64>,
    /// User-defined frontmatter retained by the domain.
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

impl Note {
    /// Creates note metadata after validating its source path and parents.
    pub fn new(
        id: NoteId,
        source_path: impl Into<String>,
        title: impl Into<String>,
        parents: Vec<NoteId>,
        order: Option<i64>,
        properties: BTreeMap<String, Value>,
    ) -> Result<Self, WorkspaceError> {
        let source_path = source_path.into();
        if source_path.trim().is_empty() {
            return Err(WorkspaceError::EmptySourcePath);
        }
        validate_parent_list(&id, &parents)?;
        Ok(Self {
            id,
            source_path,
            title: title.into(),
            parents,
            order,
            properties,
        })
    }
}

/// One tree placement of a note. Multiple placements may reference one note.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// Stable placement identity.
    pub id: PlacementId,
    /// Referenced note content identity.
    pub note_id: NoteId,
    /// Parent note identity, or `None` for a workspace root placement.
    pub parent_id: Option<NoteId>,
    /// Stable sibling ordering value.
    pub order: i64,
}

/// Rebuildable in-memory view of notes and their placements.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceProjection {
    /// All discovered note records.
    pub notes: Vec<Note>,
    /// All tree placements derived from note metadata.
    pub placements: Vec<Placement>,
}

impl WorkspaceProjection {
    /// Creates a projection only when all references and identities are valid.
    pub fn new(notes: Vec<Note>, placements: Vec<Placement>) -> Result<Self, WorkspaceError> {
        validate_projection(&notes, &placements)?;
        Ok(Self { notes, placements })
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
    /// Move an existing placement.
    MovePlacement,
    /// Add another placement for existing content.
    ClonePlacement,
    /// Hide a note without deleting its source.
    ArchiveNote,
    /// Delete a note or placement.
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
    /// A note was listed more than once during rebuild.
    #[error("duplicate note_id: {0:?}")]
    DuplicateNoteId(NoteId),
    /// A placement was listed more than once during rebuild.
    #[error("duplicate placement_id: {0:?}")]
    DuplicatePlacementId(PlacementId),
    /// A placement referenced a note that was not rebuilt.
    #[error("placement references unknown note_id: {0:?}")]
    UnknownNoteId(NoteId),
    /// A placement referenced an unknown parent.
    #[error("placement references unknown parent note_id: {0:?}")]
    UnknownParentId(NoteId),
    /// A note cannot be its own ancestor.
    #[error("note cannot parent itself: {0:?}")]
    SelfParent(NoteId),
    /// A parent was repeated in ordered frontmatter.
    #[error("duplicate parent note_id: {0:?}")]
    DuplicateParentId(NoteId),
    /// Removing a parent that is absent is an invalid domain command.
    #[error("parent is not present: {0:?}")]
    ParentNotPresent(NoteId),
    /// Readonly mode rejects every mutation action.
    #[error("readonly workspace rejects mutation: {0:?}")]
    ReadonlyMutation(MutationAction),
}

/// Validates ordered parent references for one note.
pub fn validate_parent_list(note_id: &NoteId, parents: &[NoteId]) -> Result<(), WorkspaceError> {
    let mut seen = HashSet::with_capacity(parents.len());
    for parent_id in parents {
        if parent_id == note_id {
            return Err(WorkspaceError::SelfParent(note_id.clone()));
        }
        if !seen.insert(parent_id) {
            return Err(WorkspaceError::DuplicateParentId(parent_id.clone()));
        }
    }
    Ok(())
}

/// Returns a new ordered parent list with one parent appended.
pub fn add_parent(note: &Note, parent_id: NoteId) -> Result<Vec<NoteId>, WorkspaceError> {
    validate_parent_list(&note.id, &note.parents)?;
    if parent_id == note.id {
        return Err(WorkspaceError::SelfParent(note.id.clone()));
    }
    if note.parents.contains(&parent_id) {
        return Err(WorkspaceError::DuplicateParentId(parent_id));
    }
    let mut parents = note.parents.clone();
    parents.push(parent_id);
    Ok(parents)
}

/// Returns a new ordered parent list with one parent removed.
pub fn remove_parent(note: &Note, parent_id: &NoteId) -> Result<Vec<NoteId>, WorkspaceError> {
    let mut parents = note.parents.clone();
    let Some(index) = parents.iter().position(|candidate| candidate == parent_id) else {
        return Err(WorkspaceError::ParentNotPresent(parent_id.clone()));
    };
    parents.remove(index);
    Ok(parents)
}

/// Validates all identity and reference invariants needed after a full rebuild.
pub fn validate_projection(notes: &[Note], placements: &[Placement]) -> Result<(), WorkspaceError> {
    let mut note_ids = HashSet::with_capacity(notes.len());
    for note in notes {
        if !note_ids.insert(note.id.clone()) {
            return Err(WorkspaceError::DuplicateNoteId(note.id.clone()));
        }
        validate_parent_list(&note.id, &note.parents)?;
    }

    let mut placement_ids = HashSet::with_capacity(placements.len());
    for placement in placements {
        if !placement_ids.insert(placement.id.clone()) {
            return Err(WorkspaceError::DuplicatePlacementId(placement.id.clone()));
        }
        if !note_ids.contains(&placement.note_id) {
            return Err(WorkspaceError::UnknownNoteId(placement.note_id.clone()));
        }
        if let Some(parent_id) = &placement.parent_id {
            if parent_id == &placement.note_id {
                return Err(WorkspaceError::SelfParent(parent_id.clone()));
            }
            if !note_ids.contains(parent_id) {
                return Err(WorkspaceError::UnknownParentId(parent_id.clone()));
            }
        }
    }
    Ok(())
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

    fn placement_id(value: &str) -> PlacementId {
        PlacementId::new(value).expect("test placement id")
    }

    fn note(id: &str) -> Note {
        Note::new(
            note_id(id),
            format!("{id}.md"),
            id,
            Vec::new(),
            None,
            BTreeMap::new(),
        )
        .expect("test note")
    }

    #[test]
    fn note_and_placement_ids_are_distinct_typed_values() {
        let content = note("same-value");
        let projection = WorkspaceProjection::new(
            vec![content.clone()],
            vec![
                Placement {
                    id: placement_id("left"),
                    note_id: content.id.clone(),
                    parent_id: None,
                    order: 0,
                },
                Placement {
                    id: placement_id("right"),
                    note_id: content.id,
                    parent_id: None,
                    order: 1,
                },
            ],
        );

        assert!(projection.is_ok());
    }

    #[test]
    fn identifiers_and_revision_require_values() {
        assert!(matches!(
            NoteId::new("  "),
            Err(WorkspaceError::EmptyIdentifier { .. })
        ));
        assert!(matches!(
            PlacementId::new(""),
            Err(WorkspaceError::EmptyIdentifier { .. })
        ));
        assert!(matches!(
            RevisionToken::new("", 1),
            Err(WorkspaceError::EmptyRevisionHash)
        ));
    }

    #[test]
    fn parent_updates_are_ordered_and_pure() {
        let original = note("child");
        let parent = note_id("parent");
        let with_parent = add_parent(&original, parent.clone()).expect("add parent");

        assert_eq!(original.parents, Vec::<NoteId>::new());
        assert_eq!(with_parent, vec![parent.clone()]);
        assert!(matches!(
            add_parent(
                &Note {
                    parents: with_parent,
                    ..original.clone()
                },
                parent.clone()
            ),
            Err(WorkspaceError::DuplicateParentId(_))
        ));
        assert_eq!(
            remove_parent(
                &Note {
                    parents: vec![parent.clone()],
                    ..original
                },
                &parent
            ),
            Ok(Vec::new())
        );
    }

    #[test]
    fn duplicate_ids_and_dangling_references_fail_rebuild() {
        let duplicate = note("duplicate");
        assert_eq!(
            validate_projection(&[duplicate.clone(), duplicate], &[]),
            Err(WorkspaceError::DuplicateNoteId(note_id("duplicate")))
        );

        let dangling = Placement {
            id: placement_id("placement"),
            note_id: note_id("missing"),
            parent_id: None,
            order: 0,
        };
        assert!(matches!(
            validate_projection(&[note("known")], &[dangling]),
            Err(WorkspaceError::UnknownNoteId(_))
        ));
    }

    #[test]
    fn frontmatter_preserves_workspace_and_user_fields() {
        let json = serde_json::json!({
            "id": "note-1",
            "parents": ["root"],
            "order": 2,
            "custom": "kept"
        });
        let frontmatter: WorkspaceFrontmatter = serde_json::from_value(json).expect("frontmatter");

        assert_eq!(frontmatter.id, Some(note_id("note-1")));
        assert_eq!(frontmatter.parents, vec![note_id("root")]);
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
            MutationAction::MovePlacement,
            MutationAction::ClonePlacement,
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
