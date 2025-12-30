//! Discussion and comment infrastructure for proposal deliberation
//!
//! This module provides the foundation for structured discussions during
//! the deliberation phase of governance proposals. Members can comment,
//! reply in threads, and react to comments.

use crate::proposal::ProposalId;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Unique identifier for a comment
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommentId(pub String);

impl CommentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CommentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A comment on a proposal during deliberation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// Unique identifier for this comment
    pub id: CommentId,
    /// The proposal this comment belongs to
    pub proposal_id: ProposalId,
    /// Author's DID
    pub author: Did,
    /// Comment content (markdown supported)
    pub content: String,
    /// Parent comment ID for threaded replies
    pub parent_id: Option<CommentId>,
    /// When the comment was created
    pub created_at: u64,
    /// When the comment was last updated
    pub updated_at: u64,
    /// Reactions (emoji -> list of reactors)
    pub reactions: HashMap<String, Vec<Did>>,
    /// Whether the comment has been edited
    pub is_edited: bool,
    /// Whether the comment has been soft-deleted
    pub is_deleted: bool,
}

impl Comment {
    /// Create a new top-level comment
    pub fn new(proposal_id: ProposalId, author: Did, content: String) -> Self {
        let now = current_timestamp();
        Self {
            id: CommentId::new(),
            proposal_id,
            author,
            content,
            parent_id: None,
            created_at: now,
            updated_at: now,
            reactions: HashMap::new(),
            is_edited: false,
            is_deleted: false,
        }
    }

    /// Create a reply to an existing comment
    pub fn reply(parent: &Comment, author: Did, content: String) -> Self {
        let mut comment = Self::new(parent.proposal_id.clone(), author, content);
        comment.parent_id = Some(parent.id.clone());
        comment
    }

    /// Check if this comment is a reply to another comment
    pub fn is_reply(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Get the total reaction count
    pub fn reaction_count(&self) -> usize {
        self.reactions.values().map(|v| v.len()).sum()
    }

    /// Check if a user has reacted with a specific emoji
    pub fn has_reacted(&self, user: &Did, emoji: &str) -> bool {
        self.reactions
            .get(emoji)
            .map(|reactors| reactors.contains(user))
            .unwrap_or(false)
    }
}

/// Discussion thread on a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discussion {
    /// The proposal being discussed
    pub proposal_id: ProposalId,
    /// All comments in chronological order
    pub comments: Vec<Comment>,
    /// Number of unique participants
    pub participant_count: usize,
    /// Timestamp of last activity
    pub last_activity_at: u64,
}

impl Discussion {
    /// Create a new empty discussion
    pub fn new(proposal_id: ProposalId) -> Self {
        Self {
            proposal_id,
            comments: Vec::new(),
            participant_count: 0,
            last_activity_at: current_timestamp(),
        }
    }

    /// Get top-level comments (not replies)
    pub fn top_level_comments(&self) -> Vec<&Comment> {
        self.comments
            .iter()
            .filter(|c| c.parent_id.is_none() && !c.is_deleted)
            .collect()
    }

    /// Get replies to a specific comment
    pub fn replies_to(&self, comment_id: &CommentId) -> Vec<&Comment> {
        self.comments
            .iter()
            .filter(|c| c.parent_id.as_ref() == Some(comment_id) && !c.is_deleted)
            .collect()
    }
}

/// Error type for discussion operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum DiscussionError {
    #[error("Comment not found: {0}")]
    CommentNotFound(String),

    #[error("Discussion not found for proposal: {0}")]
    DiscussionNotFound(String),

    #[error("Not authorized: {0}")]
    NotAuthorized(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Trait for discussion storage backends
pub trait DiscussionStore: Send + Sync {
    /// Add a new comment
    fn add_comment(&mut self, comment: Comment) -> Result<CommentId, DiscussionError>;

    /// Get a comment by ID
    fn get_comment(&self, id: &CommentId) -> Option<&Comment>;

    /// Get a mutable reference to a comment
    fn get_comment_mut(&mut self, id: &CommentId) -> Option<&mut Comment>;

    /// Edit a comment's content
    fn edit_comment(
        &mut self,
        id: &CommentId,
        new_content: String,
        editor: &Did,
    ) -> Result<(), DiscussionError>;

    /// Soft-delete a comment
    fn delete_comment(&mut self, id: &CommentId, deleter: &Did) -> Result<(), DiscussionError>;

    /// Add a reaction to a comment
    fn add_reaction(
        &mut self,
        comment_id: &CommentId,
        reactor: &Did,
        emoji: &str,
    ) -> Result<(), DiscussionError>;

    /// Remove a reaction from a comment
    fn remove_reaction(
        &mut self,
        comment_id: &CommentId,
        reactor: &Did,
        emoji: &str,
    ) -> Result<(), DiscussionError>;

    /// Get the full discussion for a proposal
    fn get_discussion(&self, proposal_id: &ProposalId) -> Option<Discussion>;

    /// List comments for a proposal with pagination
    fn list_comments(&self, proposal_id: &ProposalId, limit: usize, offset: usize)
        -> Vec<&Comment>;

    /// Count comments for a proposal
    fn count_comments(&self, proposal_id: &ProposalId) -> usize;

    /// Get unique participant DIDs for a proposal discussion
    fn get_participants(&self, proposal_id: &ProposalId) -> Vec<Did>;
}

/// In-memory implementation of DiscussionStore for testing
#[derive(Debug, Default)]
pub struct InMemoryDiscussionStore {
    comments: HashMap<CommentId, Comment>,
    by_proposal: HashMap<ProposalId, Vec<CommentId>>,
}

impl InMemoryDiscussionStore {
    pub fn new() -> Self {
        Self {
            comments: HashMap::new(),
            by_proposal: HashMap::new(),
        }
    }
}

impl DiscussionStore for InMemoryDiscussionStore {
    fn add_comment(&mut self, comment: Comment) -> Result<CommentId, DiscussionError> {
        let id = comment.id.clone();
        let proposal_id = comment.proposal_id.clone();

        // Validate parent exists if this is a reply
        if let Some(ref parent_id) = comment.parent_id {
            if !self.comments.contains_key(parent_id) {
                return Err(DiscussionError::CommentNotFound(format!(
                    "Parent comment {parent_id} not found"
                )));
            }
        }

        self.comments.insert(id.clone(), comment);
        self.by_proposal
            .entry(proposal_id)
            .or_default()
            .push(id.clone());

        Ok(id)
    }

    fn get_comment(&self, id: &CommentId) -> Option<&Comment> {
        self.comments.get(id)
    }

    fn get_comment_mut(&mut self, id: &CommentId) -> Option<&mut Comment> {
        self.comments.get_mut(id)
    }

    fn edit_comment(
        &mut self,
        id: &CommentId,
        new_content: String,
        editor: &Did,
    ) -> Result<(), DiscussionError> {
        let comment = self
            .comments
            .get_mut(id)
            .ok_or_else(|| DiscussionError::CommentNotFound(id.0.clone()))?;

        if &comment.author != editor {
            return Err(DiscussionError::NotAuthorized(
                "Only the author can edit a comment".to_string(),
            ));
        }

        if comment.is_deleted {
            return Err(DiscussionError::InvalidOperation(
                "Cannot edit a deleted comment".to_string(),
            ));
        }

        comment.content = new_content;
        comment.updated_at = current_timestamp();
        comment.is_edited = true;

        Ok(())
    }

    fn delete_comment(&mut self, id: &CommentId, deleter: &Did) -> Result<(), DiscussionError> {
        let comment = self
            .comments
            .get_mut(id)
            .ok_or_else(|| DiscussionError::CommentNotFound(id.0.clone()))?;

        if &comment.author != deleter {
            return Err(DiscussionError::NotAuthorized(
                "Only the author can delete a comment".to_string(),
            ));
        }

        comment.is_deleted = true;
        comment.content = "[deleted]".to_string();
        comment.updated_at = current_timestamp();

        Ok(())
    }

    fn add_reaction(
        &mut self,
        comment_id: &CommentId,
        reactor: &Did,
        emoji: &str,
    ) -> Result<(), DiscussionError> {
        let comment = self
            .comments
            .get_mut(comment_id)
            .ok_or_else(|| DiscussionError::CommentNotFound(comment_id.0.clone()))?;

        if comment.is_deleted {
            return Err(DiscussionError::InvalidOperation(
                "Cannot react to a deleted comment".to_string(),
            ));
        }

        let reactors = comment.reactions.entry(emoji.to_string()).or_default();

        // Don't add duplicate reactions
        if !reactors.contains(reactor) {
            reactors.push(reactor.clone());
        }

        Ok(())
    }

    fn remove_reaction(
        &mut self,
        comment_id: &CommentId,
        reactor: &Did,
        emoji: &str,
    ) -> Result<(), DiscussionError> {
        let comment = self
            .comments
            .get_mut(comment_id)
            .ok_or_else(|| DiscussionError::CommentNotFound(comment_id.0.clone()))?;

        if let Some(reactors) = comment.reactions.get_mut(emoji) {
            reactors.retain(|r| r != reactor);
            if reactors.is_empty() {
                comment.reactions.remove(emoji);
            }
        }

        Ok(())
    }

    fn get_discussion(&self, proposal_id: &ProposalId) -> Option<Discussion> {
        let comment_ids = self.by_proposal.get(proposal_id)?;

        let comments: Vec<Comment> = comment_ids
            .iter()
            .filter_map(|id| self.comments.get(id).cloned())
            .collect();

        if comments.is_empty() {
            return None;
        }

        // Calculate participants and last_activity before moving comments
        let participants: HashSet<Did> = comments
            .iter()
            .filter(|c| !c.is_deleted)
            .map(|c| c.author.clone())
            .collect();

        let last_activity = comments
            .iter()
            .filter(|c| !c.is_deleted)
            .map(|c| c.updated_at)
            .max()
            .unwrap_or(0);

        let participant_count = participants.len();

        Some(Discussion {
            proposal_id: proposal_id.clone(),
            comments,
            participant_count,
            last_activity_at: last_activity,
        })
    }

    fn list_comments(
        &self,
        proposal_id: &ProposalId,
        limit: usize,
        offset: usize,
    ) -> Vec<&Comment> {
        self.by_proposal
            .get(proposal_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.comments.get(id))
                    .filter(|c| !c.is_deleted)
                    .skip(offset)
                    .take(limit)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn count_comments(&self, proposal_id: &ProposalId) -> usize {
        self.by_proposal
            .get(proposal_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.comments.get(id))
                    .filter(|c| !c.is_deleted)
                    .count()
            })
            .unwrap_or(0)
    }

    fn get_participants(&self, proposal_id: &ProposalId) -> Vec<Did> {
        self.by_proposal
            .get(proposal_id)
            .map(|ids| {
                let participants: HashSet<_> = ids
                    .iter()
                    .filter_map(|id| self.comments.get(id))
                    .filter(|c| !c.is_deleted)
                    .map(|c| c.author.clone())
                    .collect();
                participants.into_iter().collect()
            })
            .unwrap_or_default()
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did(name: &str) -> Did {
        // Create deterministic DIDs for testing based on name
        let mut bytes = [0u8; 32];
        let name_bytes = name.as_bytes();
        for (i, &b) in name_bytes.iter().enumerate() {
            if i < 32 {
                bytes[i] = b;
            }
        }
        Did::from_anchor_id(&bytes)
    }

    fn test_proposal_id() -> ProposalId {
        ProposalId("test-proposal-1".to_string())
    }

    #[test]
    fn test_create_comment() {
        let proposal_id = test_proposal_id();
        let author = test_did("alice");
        let comment = Comment::new(
            proposal_id.clone(),
            author.clone(),
            "Great idea!".to_string(),
        );

        assert_eq!(comment.proposal_id, proposal_id);
        assert_eq!(comment.author, author);
        assert_eq!(comment.content, "Great idea!");
        assert!(comment.parent_id.is_none());
        assert!(!comment.is_edited);
        assert!(!comment.is_deleted);
    }

    #[test]
    fn test_create_reply() {
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");

        let parent = Comment::new(proposal_id.clone(), alice, "What do you think?".to_string());
        let reply = Comment::reply(&parent, bob, "I agree!".to_string());

        assert_eq!(reply.parent_id, Some(parent.id.clone()));
        assert!(reply.is_reply());
    }

    #[test]
    fn test_add_and_retrieve_comment() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let author = test_did("alice");

        let comment = Comment::new(
            proposal_id.clone(),
            author.clone(),
            "Test comment".to_string(),
        );
        let id = store.add_comment(comment).unwrap();

        let retrieved = store.get_comment(&id).unwrap();
        assert_eq!(retrieved.content, "Test comment");
        assert_eq!(retrieved.author, author);
    }

    #[test]
    fn test_threaded_replies() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");

        let parent = Comment::new(proposal_id.clone(), alice, "Parent comment".to_string());
        let parent_id = store.add_comment(parent.clone()).unwrap();

        let reply = Comment::reply(
            store.get_comment(&parent_id).unwrap(),
            bob,
            "Reply comment".to_string(),
        );
        let reply_id = store.add_comment(reply).unwrap();

        let retrieved_reply = store.get_comment(&reply_id).unwrap();
        assert_eq!(retrieved_reply.parent_id, Some(parent_id));
    }

    #[test]
    fn test_edit_comment() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");

        let comment = Comment::new(proposal_id.clone(), alice.clone(), "Original".to_string());
        let id = store.add_comment(comment).unwrap();

        store
            .edit_comment(&id, "Edited".to_string(), &alice)
            .unwrap();

        let retrieved = store.get_comment(&id).unwrap();
        assert_eq!(retrieved.content, "Edited");
        assert!(retrieved.is_edited);
    }

    #[test]
    fn test_edit_comment_not_author() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");

        let comment = Comment::new(proposal_id.clone(), alice, "Original".to_string());
        let id = store.add_comment(comment).unwrap();

        let result = store.edit_comment(&id, "Hacked!".to_string(), &bob);
        assert!(matches!(result, Err(DiscussionError::NotAuthorized(_))));
    }

    #[test]
    fn test_delete_comment() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");

        let comment = Comment::new(proposal_id.clone(), alice.clone(), "To delete".to_string());
        let id = store.add_comment(comment).unwrap();

        store.delete_comment(&id, &alice).unwrap();

        let retrieved = store.get_comment(&id).unwrap();
        assert!(retrieved.is_deleted);
        assert_eq!(retrieved.content, "[deleted]");
    }

    #[test]
    fn test_reactions() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");

        let comment = Comment::new(
            proposal_id.clone(),
            alice.clone(),
            "React to me!".to_string(),
        );
        let id = store.add_comment(comment).unwrap();

        store.add_reaction(&id, &bob, "thumbsup").unwrap();
        store.add_reaction(&id, &alice, "thumbsup").unwrap();
        store.add_reaction(&id, &bob, "heart").unwrap();

        let retrieved = store.get_comment(&id).unwrap();
        assert_eq!(retrieved.reactions.get("thumbsup").unwrap().len(), 2);
        assert_eq!(retrieved.reactions.get("heart").unwrap().len(), 1);
        assert!(retrieved.has_reacted(&bob, "thumbsup"));
        assert!(!retrieved.has_reacted(&bob, "smile"));
    }

    #[test]
    fn test_remove_reaction() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");

        let comment = Comment::new(
            proposal_id.clone(),
            alice.clone(),
            "React to me!".to_string(),
        );
        let id = store.add_comment(comment).unwrap();

        store.add_reaction(&id, &bob, "thumbsup").unwrap();
        store.remove_reaction(&id, &bob, "thumbsup").unwrap();

        let retrieved = store.get_comment(&id).unwrap();
        assert!(!retrieved.reactions.contains_key("thumbsup"));
    }

    #[test]
    fn test_get_discussion() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");

        let comment1 = Comment::new(proposal_id.clone(), alice.clone(), "First".to_string());
        let comment2 = Comment::new(proposal_id.clone(), bob.clone(), "Second".to_string());
        let comment3 = Comment::new(proposal_id.clone(), alice.clone(), "Third".to_string());

        store.add_comment(comment1).unwrap();
        store.add_comment(comment2).unwrap();
        store.add_comment(comment3).unwrap();

        let discussion = store.get_discussion(&proposal_id).unwrap();
        assert_eq!(discussion.comments.len(), 3);
        assert_eq!(discussion.participant_count, 2); // alice and bob
    }

    #[test]
    fn test_list_comments_pagination() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");

        for i in 0..10 {
            let comment = Comment::new(proposal_id.clone(), alice.clone(), format!("Comment {i}"));
            store.add_comment(comment).unwrap();
        }

        let page1 = store.list_comments(&proposal_id, 3, 0);
        assert_eq!(page1.len(), 3);

        let page2 = store.list_comments(&proposal_id, 3, 3);
        assert_eq!(page2.len(), 3);

        let page_last = store.list_comments(&proposal_id, 3, 9);
        assert_eq!(page_last.len(), 1);
    }

    #[test]
    fn test_count_comments_excludes_deleted() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");

        let comment1 = Comment::new(proposal_id.clone(), alice.clone(), "Keep".to_string());
        let comment2 = Comment::new(proposal_id.clone(), alice.clone(), "Delete".to_string());

        store.add_comment(comment1).unwrap();
        let id2 = store.add_comment(comment2).unwrap();

        store.delete_comment(&id2, &alice).unwrap();

        assert_eq!(store.count_comments(&proposal_id), 1);
    }

    #[test]
    fn test_get_participants() {
        let mut store = InMemoryDiscussionStore::new();
        let proposal_id = test_proposal_id();
        let alice = test_did("alice");
        let bob = test_did("bob");
        let charlie = test_did("charlie");

        store
            .add_comment(Comment::new(
                proposal_id.clone(),
                alice.clone(),
                "Alice 1".to_string(),
            ))
            .unwrap();
        store
            .add_comment(Comment::new(
                proposal_id.clone(),
                bob.clone(),
                "Bob 1".to_string(),
            ))
            .unwrap();
        store
            .add_comment(Comment::new(
                proposal_id.clone(),
                alice.clone(),
                "Alice 2".to_string(),
            ))
            .unwrap();
        store
            .add_comment(Comment::new(
                proposal_id.clone(),
                charlie.clone(),
                "Charlie 1".to_string(),
            ))
            .unwrap();

        let participants = store.get_participants(&proposal_id);
        assert_eq!(participants.len(), 3);
    }
}
