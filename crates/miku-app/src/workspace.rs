//! Application service boundary for the file-backed workspace.

use async_trait::async_trait;
use miku_domain::workspace::RevisionToken;
use miku_vault::{Vault, VaultDocument, VaultError};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceServiceError {
    #[error("workspace is readonly")]
    Readonly,
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("note changed on disk")]
    Conflict,
    #[error("vault operation failed: {0}")]
    Vault(#[from] VaultError),
}

#[async_trait]
pub trait WorkspaceService: Send + Sync {
    async fn workspace(&self) -> Result<Vec<VaultDocument>, WorkspaceServiceError>;
    async fn save_note(
        &self,
        note_id: &str,
        title: String,
        body: String,
        expected_revision: RevisionToken,
    ) -> Result<VaultDocument, WorkspaceServiceError>;
    fn readonly(&self) -> bool;
    fn root(&self) -> String;
}

#[derive(Debug, Clone)]
pub struct FileWorkspaceService {
    vault: Arc<Vault>,
    readonly: bool,
}

impl FileWorkspaceService {
    pub fn new(vault: Arc<Vault>, readonly: bool) -> Self {
        Self { vault, readonly }
    }
}

#[async_trait]
impl WorkspaceService for FileWorkspaceService {
    async fn workspace(&self) -> Result<Vec<VaultDocument>, WorkspaceServiceError> {
        Ok(self.vault.scan()?)
    }

    async fn save_note(
        &self,
        note_id: &str,
        title: String,
        body: String,
        expected_revision: RevisionToken,
    ) -> Result<VaultDocument, WorkspaceServiceError> {
        if self.readonly {
            return Err(WorkspaceServiceError::Readonly);
        }
        let mut document = self
            .vault
            .scan()?
            .into_iter()
            .find(|document| document.note.id.as_str() == note_id)
            .ok_or_else(|| WorkspaceServiceError::NotFound(note_id.to_string()))?;
        if document.revision != expected_revision {
            return Err(WorkspaceServiceError::Conflict);
        }
        document.note.title = title.trim().to_string();
        document.body = body;
        document.revision = self.vault.write(&document)?;
        Ok(document)
    }

    fn readonly(&self) -> bool {
        self.readonly
    }

    fn root(&self) -> String {
        self.vault.root().display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::workspace::RevisionToken;
    use tempfile::tempdir;

    #[tokio::test]
    async fn service_rejects_stale_writes() {
        let root = tempdir().unwrap();
        let vault = Arc::new(Vault::new(root.path()));
        let document = vault
            .create("Today", "Today", "body", Default::default())
            .unwrap();
        let service = FileWorkspaceService::new(vault, false);
        let stale = RevisionToken::new("stale", document.revision.mtime).unwrap();
        assert!(matches!(
            service
                .save_note("wrong", "Today".into(), "body".into(), stale)
                .await,
            Err(WorkspaceServiceError::NotFound(_))
        ));
    }
}
