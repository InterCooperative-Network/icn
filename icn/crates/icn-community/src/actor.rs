//! CommunityActor - Async actor for community management
//!
//! This actor handles all community operations in an async runtime, providing:
//! - Community creation, activation, and dissolution
//! - Member join/leave operations
//! - Resource pool management
//! - Gossip network synchronization

use crate::{
    Community, CommunityError, CommunityLifecycle, CommunityStore, CommunityType, MemberType,
    MembershipManager, ResourceManager, Result,
};
use icn_gossip::GossipActor;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info};

/// Gossip topic for community state updates
pub const COMMUNITY_TOPIC: &str = "community:updates";

/// Actor message channel buffer size
const ACTOR_CHANNEL_SIZE: usize = 100;

/// Handle type for gossip actor
pub type GossipHandle = Arc<RwLock<GossipActor>>;

/// Messages handled by CommunityActor
pub enum CommunityMessage {
    Create {
        id: Option<String>,
        name: String,
        community_type: CommunityType,
        founder_id: String,
        founder_type: MemberType,
        charter: String,
        reply: oneshot::Sender<Result<Community>>,
    },
    Get {
        community_id: String,
        reply: oneshot::Sender<Result<Community>>,
    },
    List {
        reply: oneshot::Sender<Result<Vec<Community>>>,
    },
    Activate {
        community_id: String,
        reply: oneshot::Sender<Result<Community>>,
    },
    Dissolve {
        community_id: String,
        reply: oneshot::Sender<Result<Community>>,
    },
    Join {
        community_id: String,
        member_id: String,
        member_type: MemberType,
        reply: oneshot::Sender<Result<Community>>,
    },
    Leave {
        community_id: String,
        member_id: String,
        reply: oneshot::Sender<Result<Community>>,
    },
    ListMembers {
        community_id: String,
        reply: oneshot::Sender<Result<Vec<crate::types::Member>>>,
    },
    AllocateResource {
        community_id: String,
        pool_name: String,
        recipient: String,
        amount: u64,
        reply: oneshot::Sender<Result<Community>>,
    },
    GetResourcePools {
        community_id: String,
        reply: oneshot::Sender<Result<Vec<crate::types::ResourcePool>>>,
    },
    Update {
        community_id: String,
        name: Option<String>,
        metadata: Option<std::collections::HashMap<String, String>>,
        reply: oneshot::Sender<Result<Community>>,
    },
}

/// Async actor for community management
pub struct CommunityActor {
    rx: mpsc::Receiver<CommunityMessage>,
    store: CommunityStore,
    lifecycle: CommunityLifecycle,
    membership: MembershipManager,
    resources: ResourceManager,
    gossip: Option<GossipHandle>,
}

impl CommunityActor {
    /// Spawn the actor and return a sender for messages
    pub fn spawn(
        store: CommunityStore,
        gossip: Option<GossipHandle>,
    ) -> mpsc::Sender<CommunityMessage> {
        let (tx, rx) = mpsc::channel(ACTOR_CHANNEL_SIZE);

        let lifecycle = CommunityLifecycle::new(1); // At least 1 founder
        let membership = MembershipManager::new();
        let resources = ResourceManager::new();

        let mut actor = Self {
            rx,
            store,
            lifecycle,
            membership,
            resources,
            gossip,
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        tx
    }

    async fn run(&mut self) {
        info!("CommunityActor started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                CommunityMessage::Create {
                    id,
                    name,
                    community_type,
                    founder_id,
                    founder_type,
                    charter,
                    reply,
                } => {
                    let result = self
                        .handle_create(id, name, community_type, founder_id, founder_type, charter)
                        .await;
                    let _ = reply.send(result);
                }
                CommunityMessage::Get {
                    community_id,
                    reply,
                } => {
                    let result = self.handle_get(&community_id);
                    let _ = reply.send(result);
                }
                CommunityMessage::List { reply } => {
                    let result = self.store.list();
                    let _ = reply.send(result);
                }
                CommunityMessage::Activate {
                    community_id,
                    reply,
                } => {
                    let result = self.handle_activate(&community_id).await;
                    let _ = reply.send(result);
                }
                CommunityMessage::Dissolve {
                    community_id,
                    reply,
                } => {
                    let result = self.handle_dissolve(&community_id).await;
                    let _ = reply.send(result);
                }
                CommunityMessage::Join {
                    community_id,
                    member_id,
                    member_type,
                    reply,
                } => {
                    let result = self
                        .handle_join(&community_id, member_id, member_type)
                        .await;
                    let _ = reply.send(result);
                }
                CommunityMessage::Leave {
                    community_id,
                    member_id,
                    reply,
                } => {
                    let result = self.handle_leave(&community_id, &member_id).await;
                    let _ = reply.send(result);
                }
                CommunityMessage::ListMembers {
                    community_id,
                    reply,
                } => {
                    let result = self.handle_list_members(&community_id);
                    let _ = reply.send(result);
                }
                CommunityMessage::AllocateResource {
                    community_id,
                    pool_name,
                    recipient,
                    amount,
                    reply,
                } => {
                    let result = self
                        .handle_allocate_resource(&community_id, &pool_name, &recipient, amount)
                        .await;
                    let _ = reply.send(result);
                }
                CommunityMessage::GetResourcePools {
                    community_id,
                    reply,
                } => {
                    let result = self.handle_get_resource_pools(&community_id);
                    let _ = reply.send(result);
                }
                CommunityMessage::Update {
                    community_id,
                    name,
                    metadata,
                    reply,
                } => {
                    let result = self.handle_update(&community_id, name, metadata).await;
                    let _ = reply.send(result);
                }
            }
        }

        info!("CommunityActor stopped");
    }

    fn handle_get(&self, community_id: &str) -> Result<Community> {
        self.store
            .get(&community_id.to_string())?
            .ok_or_else(|| CommunityError::NotFound(community_id.to_string()))
    }

    /// Validate community ID format
    fn validate_community_id(id: &str) -> Result<()> {
        // Max length check
        if id.len() > 128 {
            return Err(CommunityError::Validation(
                "Community ID must be 128 characters or less".into(),
            ));
        }

        // Min length check
        if id.is_empty() {
            return Err(CommunityError::Validation(
                "Community ID cannot be empty".into(),
            ));
        }

        // Character validation: alphanumeric, hyphens, underscores, colons
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':')
        {
            return Err(CommunityError::Validation(
                "Community ID can only contain alphanumeric characters, hyphens, underscores, and colons".into(),
            ));
        }

        Ok(())
    }

    async fn handle_create(
        &mut self,
        id: Option<String>,
        name: String,
        community_type: CommunityType,
        founder_id: String,
        founder_type: MemberType,
        charter: String,
    ) -> Result<Community> {
        // Generate or validate ID
        let community_id = match id {
            Some(provided_id) => {
                // Validate provided ID format
                Self::validate_community_id(&provided_id)?;
                provided_id
            }
            None => {
                // Auto-generate from name
                format!("community:{}", name.replace(' ', "-").to_lowercase())
            }
        };

        // Check for duplicate ID
        if self.store.get(&community_id)?.is_some() {
            return Err(CommunityError::Validation(format!(
                "Community with ID '{}' already exists",
                community_id
            )));
        }

        // Create the community using lifecycle
        let request = crate::FormationRequest {
            name: name.clone(),
            community_type,
            founding_members: vec![founder_id.clone()],
            charter_ccl: charter,
        };

        // Form the community with our validated ID
        // Note: lifecycle.form() generates internal gov_id, but we use our validated community_id
        let mut community = self
            .lifecycle
            .form(request, format!("gov:{community_id}"))?;
        community.id = community_id;

        // Add founder as first member with voting weight 1
        self.membership
            .add_member(&mut community, founder_id, founder_type, 1)?;

        // Save to store
        self.store.put(&community)?;

        // Announce to network
        self.announce_update(&community).await;

        info!(community_id = %community.id, "Created community");
        Ok(community)
    }

    async fn handle_activate(&mut self, community_id: &str) -> Result<Community> {
        let mut community = self.handle_get(community_id)?;

        self.lifecycle.activate(&mut community)?;
        self.store.put(&community)?;

        self.announce_update(&community).await;

        info!(community_id = %community.id, "Activated community");
        Ok(community)
    }

    async fn handle_dissolve(&mut self, community_id: &str) -> Result<Community> {
        let mut community = self.handle_get(community_id)?;

        self.lifecycle.dissolve(&mut community)?;
        self.store.put(&community)?;

        self.announce_update(&community).await;

        info!(community_id = %community.id, "Dissolved community");
        Ok(community)
    }

    async fn handle_join(
        &mut self,
        community_id: &str,
        member_id: String,
        member_type: MemberType,
    ) -> Result<Community> {
        let mut community = self.handle_get(community_id)?;

        // Default voting weight of 1 (1-member-1-vote)
        self.membership
            .add_member(&mut community, member_id.clone(), member_type, 1)?;
        self.store.put(&community)?;

        self.announce_update(&community).await;

        debug!(community_id = %community.id, member_id = %member_id, "Member joined community");
        Ok(community)
    }

    async fn handle_leave(&mut self, community_id: &str, member_id: &str) -> Result<Community> {
        let mut community = self.handle_get(community_id)?;

        self.membership.remove_member(&mut community, member_id)?;
        self.store.put(&community)?;

        self.announce_update(&community).await;

        debug!(community_id = %community.id, member_id = %member_id, "Member left community");
        Ok(community)
    }

    fn handle_list_members(&self, community_id: &str) -> Result<Vec<crate::types::Member>> {
        let community = self.handle_get(community_id)?;
        Ok(community.members.values().cloned().collect())
    }

    async fn handle_allocate_resource(
        &mut self,
        community_id: &str,
        pool_name: &str,
        _recipient: &str, // Reserved for future tracking
        amount: u64,
    ) -> Result<Community> {
        let mut community = self.handle_get(community_id)?;

        self.resources.allocate(&mut community, pool_name, amount)?;
        self.store.put(&community)?;

        self.announce_update(&community).await;

        debug!(
            community_id = %community.id,
            pool = %pool_name,
            amount = %amount,
            "Allocated resource"
        );
        Ok(community)
    }

    fn handle_get_resource_pools(
        &self,
        community_id: &str,
    ) -> Result<Vec<crate::types::ResourcePool>> {
        let community = self.handle_get(community_id)?;
        Ok(community.resource_pools.values().cloned().collect())
    }

    async fn handle_update(
        &mut self,
        community_id: &str,
        name: Option<String>,
        metadata: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Community> {
        let mut community = self.handle_get(community_id)?;

        if let Some(new_name) = name {
            community.name = new_name;
        }

        if let Some(new_metadata) = metadata {
            community.metadata.extend(new_metadata);
        }

        community.updated_at = chrono::Utc::now();
        self.store.put(&community)?;

        self.announce_update(&community).await;

        debug!(community_id = %community.id, "Updated community");
        Ok(community)
    }

    async fn announce_update(&self, community: &Community) {
        if let Some(gossip) = &self.gossip {
            match bincode::serde::encode_to_vec(community, bincode::config::legacy()) {
                Ok(data) => {
                    let mut gossip_actor = gossip.write().await;
                    match gossip_actor.publish(COMMUNITY_TOPIC, data) {
                        Ok(hash) => {
                            debug!(
                                community_id = %community.id,
                                entry_hash = ?hash,
                                "Published community update to gossip"
                            );
                        }
                        Err(e) => {
                            error!(
                                community_id = %community.id,
                                error = %e,
                                "Failed to publish community update to gossip - state may diverge"
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        community_id = %community.id,
                        error = %e,
                        "Failed to serialize community for gossip"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_store::SledStore;

    #[tokio::test]
    async fn test_create_and_get_community() {
        let store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary().unwrap());
        let comm_store = CommunityStore::new(store);
        let tx = CommunityActor::spawn(comm_store, None);

        // Create community
        let (reply, rx) = oneshot::channel();
        tx.send(CommunityMessage::Create {
            id: Some("test-community".to_string()),
            name: "Test Community".to_string(),
            community_type: CommunityType::Geographic,
            founder_id: "did:icn:founder123".to_string(),
            founder_type: MemberType::Individual("did:icn:founder123".to_string()),
            charter: "test charter".to_string(),
            reply,
        })
        .await
        .unwrap();

        let community = rx.await.unwrap().unwrap();
        assert_eq!(community.name, "Test Community");
        assert_eq!(community.active_member_count(), 1);

        // Get community
        let (reply, rx) = oneshot::channel();
        tx.send(CommunityMessage::Get {
            community_id: "test-community".to_string(),
            reply,
        })
        .await
        .unwrap();

        let community = rx.await.unwrap().unwrap();
        assert_eq!(community.name, "Test Community");
    }

    #[tokio::test]
    async fn test_join_and_leave_community() {
        let store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary().unwrap());
        let comm_store = CommunityStore::new(store);
        let tx = CommunityActor::spawn(comm_store, None);

        // Create community
        let (reply, rx) = oneshot::channel();
        tx.send(CommunityMessage::Create {
            id: Some("join-test".to_string()),
            name: "Join Test".to_string(),
            community_type: CommunityType::Interest,
            founder_id: "did:icn:founder".to_string(),
            founder_type: MemberType::Individual("did:icn:founder".to_string()),
            charter: "".to_string(),
            reply,
        })
        .await
        .unwrap();
        rx.await.unwrap().unwrap();

        // Join community
        let (reply, rx) = oneshot::channel();
        tx.send(CommunityMessage::Join {
            community_id: "join-test".to_string(),
            member_id: "did:icn:member1".to_string(),
            member_type: MemberType::Individual("did:icn:member1".to_string()),
            reply,
        })
        .await
        .unwrap();

        let community = rx.await.unwrap().unwrap();
        assert_eq!(community.active_member_count(), 2);

        // Leave community
        let (reply, rx) = oneshot::channel();
        tx.send(CommunityMessage::Leave {
            community_id: "join-test".to_string(),
            member_id: "did:icn:member1".to_string(),
            reply,
        })
        .await
        .unwrap();

        let community = rx.await.unwrap().unwrap();
        assert_eq!(community.active_member_count(), 1); // Member marked inactive
    }
}
