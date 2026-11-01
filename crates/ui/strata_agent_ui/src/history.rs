//! Time-travel revision history and state branching for AI agent generative UI.

use crate::sandbox::AgentUiSandbox;

/// A revision snapshot in the agent generation history.
#[derive(Debug, Clone)]
pub struct RevisionSnapshot {
    pub revision_id: usize,
    pub description: String,
    pub sandbox: AgentUiSandbox,
}

/// Manages a branchable stack of agent UI revisions.
#[derive(Debug, Default)]
pub struct TimeTravelHistory {
    revisions: Vec<RevisionSnapshot>,
    current_index: usize,
}

impl TimeTravelHistory {
    pub fn new(initial_sandbox: AgentUiSandbox) -> Self {
        let initial = RevisionSnapshot {
            revision_id: 0,
            description: "Initial state".to_string(),
            sandbox: initial_sandbox,
        };
        Self {
            revisions: vec![initial],
            current_index: 0,
        }
    }

    /// Pushes a new revision snapshot onto the stack, truncating any redo history.
    pub fn push_revision(&mut self, description: impl Into<String>, sandbox: AgentUiSandbox) -> usize {
        self.revisions.truncate(self.current_index + 1);
        let revision_id = self.revisions.len();
        self.revisions.push(RevisionSnapshot {
            revision_id,
            description: description.into(),
            sandbox,
        });
        self.current_index = revision_id;
        revision_id
    }

    /// Rolls back to the previous revision.
    pub fn undo(&mut self) -> Option<&AgentUiSandbox> {
        if self.current_index > 0 {
            self.current_index -= 1;
            Some(&self.revisions[self.current_index].sandbox)
        } else {
            None
        }
    }

    /// Advances to the next revision.
    pub fn redo(&mut self) -> Option<&AgentUiSandbox> {
        if self.current_index + 1 < self.revisions.len() {
            self.current_index += 1;
            Some(&self.revisions[self.current_index].sandbox)
        } else {
            None
        }
    }

    /// Jumps directly to a specific revision index.
    pub fn jump_to(&mut self, revision_id: usize) -> Option<&AgentUiSandbox> {
        if revision_id < self.revisions.len() {
            self.current_index = revision_id;
            Some(&self.revisions[self.current_index].sandbox)
        } else {
            None
        }
    }

    pub fn current(&self) -> &AgentUiSandbox {
        &self.revisions[self.current_index].sandbox
    }

    pub fn total_revisions(&self) -> usize {
        self.revisions.len()
    }

    pub fn current_revision_id(&self) -> usize {
        self.current_index
    }
}
