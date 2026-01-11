use crate::{
    AssetDistributionPlan, CoopStore, CoopType, Cooperative, FormationRequest, LifecycleEvent,
    LifecycleManager, Member, MemberRole, MembershipManager, Result,
};
use icn_gossip::GossipActor;
use icn_governance::charter::FounderSignature;
use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

/// Gossip topic for cooperative state updates
pub const COOP_TOPIC: &str = "coop:updates";

/// Handle type for gossip actor
pub type GossipHandle = Arc<RwLock<GossipActor>>;

pub struct CoopActor {
    rx: mpsc::Receiver<CoopMessage>,
    store: CoopStore,
    lifecycle: LifecycleManager,
    membership: MembershipManager,
    gossip: Option<GossipHandle>,
}

pub enum CoopMessage {
    CreateCooperative {
        id: Option<String>, // Optional explicit ID from gateway
        name: String,
        coop_type: CoopType,
        founder: Did,
        reply: oneshot::Sender<Result<Cooperative>>,
    },
    /// Create cooperative from a formation request (Issue #290)
    CreateFromRequest {
        request: FormationRequest,
        id: String,
        first_founder: Did,
        reply: oneshot::Sender<Result<(Cooperative, LifecycleEvent)>>,
    },
    GetCooperative {
        coop_id: String,
        reply: oneshot::Sender<Result<Cooperative>>,
    },
    ListCooperatives {
        reply: oneshot::Sender<Result<Vec<Cooperative>>>,
    },
    DeleteCooperative {
        coop_id: String,
        reply: oneshot::Sender<Result<()>>,
    },
    ActivateCooperative {
        coop_id: String,
        charter_hash: String,
        reply: oneshot::Sender<Result<Cooperative>>,
    },
    /// Sign charter as a founder (Issue #290)
    SignCharter {
        coop_id: String,
        signature: FounderSignature,
        reply: oneshot::Sender<Result<(Cooperative, Vec<LifecycleEvent>)>>,
    },
    AddMember {
        coop_id: String,
        did: Did,
        role: MemberRole,
        reply: oneshot::Sender<Result<Member>>,
    },
    RemoveMember {
        coop_id: String,
        did: Did,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateMemberRole {
        coop_id: String,
        did: Did,
        new_role: MemberRole,
        reply: oneshot::Sender<Result<Member>>,
    },
    ApproveMember {
        coop_id: String,
        did: Did,
        reply: oneshot::Sender<Result<Member>>,
    },
    ListMembers {
        coop_id: String,
        reply: oneshot::Sender<Result<Vec<Member>>>,
    },
    GetMemberCoops {
        did: Did,
        reply: oneshot::Sender<Result<Vec<String>>>,
    },
    UpdateCooperative {
        coop_id: String,
        name: Option<String>,
        metadata: Option<std::collections::HashMap<String, String>>,
        reply: oneshot::Sender<Result<Cooperative>>,
    },
    /// Start dissolution process with asset distribution plan (Issue #290)
    StartDissolution {
        coop_id: String,
        initiator: Did,
        plan: AssetDistributionPlan,
        proposal_id: Option<String>,
        reply: oneshot::Sender<Result<(Cooperative, LifecycleEvent)>>,
    },
    /// Complete dissolution after assets distributed (Issue #290)
    CompleteDissolution {
        coop_id: String,
        reply: oneshot::Sender<Result<(Cooperative, Vec<LifecycleEvent>)>>,
    },
}

impl CoopActor {
    pub fn spawn(store: CoopStore, gossip: Option<GossipHandle>) -> mpsc::Sender<CoopMessage> {
        let (tx, rx) = mpsc::channel(100);

        let lifecycle = LifecycleManager::new();
        let membership = MembershipManager::new();

        let mut actor = Self {
            rx,
            store,
            lifecycle,
            membership,
            gossip,
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        tx
    }

    async fn run(&mut self) {
        info!("CoopActor started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                CoopMessage::CreateCooperative {
                    id,
                    name,
                    coop_type,
                    founder,
                    reply,
                } => {
                    let result = self
                        .handle_create_cooperative(id, name, coop_type, founder)
                        .await;
                    let _ = reply.send(result);
                }
                CoopMessage::CreateFromRequest {
                    request,
                    id,
                    first_founder,
                    reply,
                } => {
                    let result = self
                        .handle_create_from_request(request, id, first_founder)
                        .await;
                    let _ = reply.send(result);
                }
                CoopMessage::SignCharter {
                    coop_id,
                    signature,
                    reply,
                } => {
                    let result = self.handle_sign_charter(coop_id, signature).await;
                    let _ = reply.send(result);
                }
                CoopMessage::StartDissolution {
                    coop_id,
                    initiator,
                    plan,
                    proposal_id,
                    reply,
                } => {
                    let result = self
                        .handle_start_dissolution(coop_id, initiator, plan, proposal_id)
                        .await;
                    let _ = reply.send(result);
                }
                CoopMessage::CompleteDissolution { coop_id, reply } => {
                    let result = self.handle_complete_dissolution(coop_id).await;
                    let _ = reply.send(result);
                }
                CoopMessage::GetCooperative { coop_id, reply } => {
                    let result = self.store.get_cooperative(&coop_id);
                    let _ = reply.send(result);
                }
                CoopMessage::ListCooperatives { reply } => {
                    let result = self.store.list_cooperatives();
                    let _ = reply.send(result);
                }
                CoopMessage::DeleteCooperative { coop_id, reply } => {
                    let result = self.handle_delete_cooperative(coop_id).await;
                    let _ = reply.send(result);
                }
                CoopMessage::ActivateCooperative {
                    coop_id,
                    charter_hash,
                    reply,
                } => {
                    let result = self
                        .handle_activate_cooperative(coop_id, charter_hash)
                        .await;
                    let _ = reply.send(result);
                }
                CoopMessage::AddMember {
                    coop_id,
                    did,
                    role,
                    reply,
                } => {
                    let result = self.handle_add_member(coop_id, did, role).await;
                    let _ = reply.send(result);
                }
                CoopMessage::RemoveMember {
                    coop_id,
                    did,
                    reply,
                } => {
                    let result = self.handle_remove_member(coop_id, did).await;
                    let _ = reply.send(result);
                }
                CoopMessage::UpdateMemberRole {
                    coop_id,
                    did,
                    new_role,
                    reply,
                } => {
                    let result = self.handle_update_member_role(coop_id, did, new_role).await;
                    let _ = reply.send(result);
                }
                CoopMessage::ApproveMember {
                    coop_id,
                    did,
                    reply,
                } => {
                    let result = self.handle_approve_member(coop_id, did).await;
                    let _ = reply.send(result);
                }
                CoopMessage::ListMembers { coop_id, reply } => {
                    let result = self.store.list_members(&coop_id);
                    let _ = reply.send(result);
                }
                CoopMessage::GetMemberCoops { did, reply } => {
                    let result = self.store.get_member_coops(&did);
                    let _ = reply.send(result);
                }
                CoopMessage::UpdateCooperative {
                    coop_id,
                    name,
                    metadata,
                    reply,
                } => {
                    let result = self
                        .handle_update_cooperative(coop_id, name, metadata)
                        .await;
                    let _ = reply.send(result);
                }
            }
        }

        info!("CoopActor stopped");
    }

    async fn handle_create_cooperative(
        &mut self,
        id: Option<String>,
        name: String,
        coop_type: CoopType,
        founder: Did,
    ) -> Result<Cooperative> {
        let coop = if let Some(id) = id {
            Cooperative::new_with_id(id, name, coop_type)
        } else {
            Cooperative::new(name, coop_type)
        };
        let coop = self
            .lifecycle
            .create_cooperative(coop, founder.clone())
            .await?;
        self.store.save_cooperative(&coop)?;

        // Add founder as first member
        let member = Member::new(founder, coop.id.clone(), MemberRole::Founder);
        let member = self.membership.add_member(member, 0.0).await?;
        let member = self.membership.approve_member(member).await?;
        self.store.save_member(&member)?;

        // Announce to network
        self.announce_coop_update(&coop).await;

        Ok(coop)
    }

    async fn handle_activate_cooperative(
        &mut self,
        coop_id: String,
        charter_hash: String,
    ) -> Result<Cooperative> {
        let coop = self.store.get_cooperative(&coop_id)?;
        let coop = self.lifecycle.activate(coop, charter_hash).await?;
        self.store.save_cooperative(&coop)?;

        self.announce_coop_update(&coop).await;

        Ok(coop)
    }

    async fn handle_add_member(
        &mut self,
        coop_id: String,
        did: Did,
        role: MemberRole,
    ) -> Result<Member> {
        // Verify coop exists
        let _ = self.store.get_cooperative(&coop_id)?;

        let member = Member::new(did, coop_id, role);
        let member = self.membership.add_member(member, 0.3).await?;
        self.store.save_member(&member)?;

        Ok(member)
    }

    async fn handle_approve_member(&mut self, coop_id: String, did: Did) -> Result<Member> {
        let member = self.store.get_member(&coop_id, &did)?;
        let member = self.membership.approve_member(member).await?;
        self.store.save_member(&member)?;

        Ok(member)
    }

    async fn handle_delete_cooperative(&mut self, coop_id: String) -> Result<()> {
        // Verify coop exists
        let _ = self.store.get_cooperative(&coop_id)?;

        // Delete all members first
        let members = self.store.list_members(&coop_id)?;
        for member in members {
            self.store.delete_member(&coop_id, &member.did)?;
        }

        // Delete the cooperative
        self.store.delete_cooperative(&coop_id)?;
        Ok(())
    }

    async fn handle_remove_member(&mut self, coop_id: String, did: Did) -> Result<()> {
        // Verify coop and member exist
        let _ = self.store.get_cooperative(&coop_id)?;
        let _ = self.store.get_member(&coop_id, &did)?;

        // Remove member
        self.store.delete_member(&coop_id, &did)?;
        Ok(())
    }

    async fn handle_update_member_role(
        &mut self,
        coop_id: String,
        did: Did,
        new_role: MemberRole,
    ) -> Result<Member> {
        // Verify coop exists
        let _ = self.store.get_cooperative(&coop_id)?;

        // Get existing member and update role
        let mut member = self.store.get_member(&coop_id, &did)?;
        member.role = new_role;
        self.store.save_member(&member)?;

        Ok(member)
    }

    async fn handle_update_cooperative(
        &mut self,
        coop_id: String,
        name: Option<String>,
        metadata: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Cooperative> {
        // Get existing cooperative
        let mut coop = self.store.get_cooperative(&coop_id)?;

        // Update name if provided
        if let Some(new_name) = name {
            coop.name = new_name;
        }

        // Update/merge metadata if provided
        if let Some(new_metadata) = metadata {
            for (key, value) in new_metadata {
                coop.metadata.insert(key, value);
            }
        }

        // Update timestamp
        coop.updated_at = chrono::Utc::now();

        // Save and announce
        self.store.save_cooperative(&coop)?;
        self.announce_coop_update(&coop).await;

        Ok(coop)
    }

    async fn announce_coop_update(&self, coop: &Cooperative) {
        if let Some(gossip) = &self.gossip {
            // Serialize the cooperative for gossip
            match icn_encoding::encode_bincode_legacy(coop) {
                Ok(data) => {
                    // Publish to gossip topic
                    let mut gossip_actor = gossip.write().await;
                    match gossip_actor.publish(COOP_TOPIC, data).await {
                        Ok(hash) => {
                            debug!(
                                coop_id = %coop.id,
                                entry_hash = ?hash,
                                "Published coop update to gossip"
                            );
                        }
                        Err(e) => {
                            warn!(
                                coop_id = %coop.id,
                                error = %e,
                                "Failed to publish coop update to gossip"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        coop_id = %coop.id,
                        error = %e,
                        "Failed to serialize coop for gossip"
                    );
                }
            }
        }
    }

    // === Issue #290: Charter signing and dissolution handlers ===

    async fn handle_create_from_request(
        &mut self,
        request: FormationRequest,
        id: String,
        first_founder: Did,
    ) -> Result<(Cooperative, LifecycleEvent)> {
        let (coop, event) = self
            .lifecycle
            .create_from_request(request, id, first_founder.clone())
            .await?;

        self.store.save_cooperative(&coop)?;

        // Add first founder as member
        let member = Member::new(first_founder, coop.id.clone(), MemberRole::Founder);
        let member = self.membership.add_member(member, 0.0).await?;
        let member = self.membership.approve_member(member).await?;
        self.store.save_member(&member)?;

        self.announce_coop_update(&coop).await;

        Ok((coop, event))
    }

    async fn handle_sign_charter(
        &mut self,
        coop_id: String,
        signature: FounderSignature,
    ) -> Result<(Cooperative, Vec<LifecycleEvent>)> {
        let coop = self.store.get_cooperative(&coop_id)?;
        let (coop, events) = self.lifecycle.sign_charter(coop, signature).await?;

        self.store.save_cooperative(&coop)?;
        self.announce_coop_update(&coop).await;

        Ok((coop, events))
    }

    async fn handle_start_dissolution(
        &mut self,
        coop_id: String,
        initiator: Did,
        plan: AssetDistributionPlan,
        proposal_id: Option<String>,
    ) -> Result<(Cooperative, LifecycleEvent)> {
        let coop = self.store.get_cooperative(&coop_id)?;
        let (coop, event) = self
            .lifecycle
            .start_dissolution_with_plan(coop, initiator, plan, proposal_id)
            .await?;

        self.store.save_cooperative(&coop)?;
        self.announce_coop_update(&coop).await;

        Ok((coop, event))
    }

    async fn handle_complete_dissolution(
        &mut self,
        coop_id: String,
    ) -> Result<(Cooperative, Vec<LifecycleEvent>)> {
        let coop = self.store.get_cooperative(&coop_id)?;
        let (coop, events) = self.lifecycle.complete_dissolution_with_plan(coop).await?;

        self.store.save_cooperative(&coop)?;
        self.announce_coop_update(&coop).await;

        Ok((coop, events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AssetDistributionPlan, BalanceAction, CapitalReturnMethod, CoopHandle, CoopStatus,
        CoopType, DebtAction, FormationRequest, MemberRole, MemberStatus,
    };
    use icn_identity::KeyPair;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_store() -> CoopStore {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        CoopStore::new(Arc::new(db))
    }

    fn create_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn spawn_test_actor() -> CoopHandle {
        let store = create_test_store();
        let tx = CoopActor::spawn(store, None);
        CoopHandle::new(tx)
    }

    // === Basic cooperative operations ===

    #[tokio::test]
    async fn test_create_cooperative() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Test Coop".to_string(),
                CoopType::Worker,
                founder.clone(),
            )
            .await
            .unwrap();

        assert_eq!(coop.name, "Test Coop");
        assert_eq!(coop.coop_type, CoopType::Worker);
        assert_eq!(coop.status, CoopStatus::Forming);
    }

    #[tokio::test]
    async fn test_create_cooperative_with_explicit_id() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                Some("my-custom-id".to_string()),
                "My Coop".to_string(),
                CoopType::Consumer,
                founder,
            )
            .await
            .unwrap();

        assert_eq!(coop.id, "my-custom-id");
        assert_eq!(coop.name, "My Coop");
    }

    #[tokio::test]
    async fn test_create_cooperative_all_types() {
        let handle = spawn_test_actor();

        for coop_type in [
            CoopType::Worker,
            CoopType::Consumer,
            CoopType::Producer,
            CoopType::MultiStakeholder,
            CoopType::Platform,
            CoopType::Housing,
            CoopType::Credit,
        ] {
            let founder = create_test_did();
            let coop = handle
                .create_cooperative(None, format!("Coop {:?}", coop_type), coop_type, founder)
                .await
                .unwrap();

            assert_eq!(coop.coop_type, coop_type);
        }
    }

    #[tokio::test]
    async fn test_get_cooperative() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let created = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        let retrieved = handle.get_cooperative(created.id.clone()).await.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, created.name);
    }

    #[tokio::test]
    async fn test_get_cooperative_not_found() {
        let handle = spawn_test_actor();

        let result = handle.get_cooperative("nonexistent".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_cooperatives_empty() {
        let handle = spawn_test_actor();

        let coops = handle.list_cooperatives().await.unwrap();
        assert!(coops.is_empty());
    }

    #[tokio::test]
    async fn test_list_cooperatives() {
        let handle = spawn_test_actor();

        // Create multiple coops
        for i in 1..=3 {
            let founder = create_test_did();
            handle
                .create_cooperative(None, format!("Coop {i}"), CoopType::Worker, founder)
                .await
                .unwrap();
        }

        let coops = handle.list_cooperatives().await.unwrap();
        assert_eq!(coops.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_cooperative() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "To Delete".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        // Verify it exists
        assert!(handle.get_cooperative(coop.id.clone()).await.is_ok());

        // Delete it
        handle.delete_cooperative(coop.id.clone()).await.unwrap();

        // Verify it's gone
        assert!(handle.get_cooperative(coop.id).await.is_err());
    }

    #[tokio::test]
    async fn test_delete_cooperative_with_members() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "With Members".to_string(),
                CoopType::Worker,
                founder.clone(),
            )
            .await
            .unwrap();

        // Add another member
        let member_did = create_test_did();
        handle
            .add_member(coop.id.clone(), member_did, MemberRole::Worker)
            .await
            .unwrap();

        // Delete the coop (should also delete members)
        handle.delete_cooperative(coop.id.clone()).await.unwrap();

        // Verify coop is gone
        assert!(handle.get_cooperative(coop.id).await.is_err());
    }

    #[tokio::test]
    async fn test_activate_cooperative() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        assert_eq!(coop.status, CoopStatus::Forming);

        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter-hash-123".to_string())
            .await
            .unwrap();

        assert_eq!(activated.status, CoopStatus::Active);
        assert_eq!(activated.charter_hash, Some("charter-hash-123".to_string()));
    }

    #[tokio::test]
    async fn test_update_cooperative() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Original Name".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        // Update name only
        let updated = handle
            .update_cooperative(coop.id.clone(), Some("New Name".to_string()), None)
            .await
            .unwrap();
        assert_eq!(updated.name, "New Name");

        // Update metadata only
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());
        let updated = handle
            .update_cooperative(coop.id.clone(), None, Some(metadata))
            .await
            .unwrap();
        assert_eq!(updated.metadata.get("key"), Some(&"value".to_string()));

        // Update both
        let mut more_metadata = std::collections::HashMap::new();
        more_metadata.insert("another".to_string(), "data".to_string());
        let updated = handle
            .update_cooperative(coop.id, Some("Final Name".to_string()), Some(more_metadata))
            .await
            .unwrap();
        assert_eq!(updated.name, "Final Name");
        assert_eq!(updated.metadata.get("key"), Some(&"value".to_string()));
        assert_eq!(updated.metadata.get("another"), Some(&"data".to_string()));
    }

    // === Member operations ===

    #[tokio::test]
    async fn test_add_member() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        let member_did = create_test_did();
        let member = handle
            .add_member(coop.id.clone(), member_did.clone(), MemberRole::Worker)
            .await
            .unwrap();

        assert_eq!(member.did, member_did);
        assert_eq!(member.role, MemberRole::Worker);
        assert_eq!(member.status, MemberStatus::Pending);
    }

    #[tokio::test]
    async fn test_add_member_all_roles() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        for role in [
            MemberRole::Member,
            MemberRole::Worker,
            MemberRole::Consumer,
            MemberRole::Producer,
            MemberRole::BoardMember,
            MemberRole::Officer,
        ] {
            let member_did = create_test_did();
            let member = handle
                .add_member(coop.id.clone(), member_did, role)
                .await
                .unwrap();
            assert_eq!(member.role, role);
        }
    }

    #[tokio::test]
    async fn test_add_member_to_nonexistent_coop() {
        let handle = spawn_test_actor();
        let member_did = create_test_did();

        let result = handle
            .add_member("nonexistent".to_string(), member_did, MemberRole::Worker)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_approve_member() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        let member_did = create_test_did();
        let member = handle
            .add_member(coop.id.clone(), member_did.clone(), MemberRole::Worker)
            .await
            .unwrap();
        assert_eq!(member.status, MemberStatus::Pending);

        let approved = handle
            .approve_member(coop.id.clone(), member_did)
            .await
            .unwrap();
        assert_eq!(approved.status, MemberStatus::Active);
    }

    #[tokio::test]
    async fn test_remove_member() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        let member_did = create_test_did();
        handle
            .add_member(coop.id.clone(), member_did.clone(), MemberRole::Worker)
            .await
            .unwrap();

        // Remove the member
        handle
            .remove_member(coop.id.clone(), member_did.clone())
            .await
            .unwrap();

        // Verify member list
        let members = handle.list_members(coop.id).await.unwrap();
        // Only founder should remain
        assert_eq!(members.len(), 1);
    }

    #[tokio::test]
    async fn test_update_member_role() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Test Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        let member_did = create_test_did();
        handle
            .add_member(coop.id.clone(), member_did.clone(), MemberRole::Member)
            .await
            .unwrap();

        let updated = handle
            .update_member_role(coop.id, member_did, MemberRole::BoardMember)
            .await
            .unwrap();
        assert_eq!(updated.role, MemberRole::BoardMember);
    }

    #[tokio::test]
    async fn test_list_members() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Test Coop".to_string(),
                CoopType::Worker,
                founder.clone(),
            )
            .await
            .unwrap();

        // Add more members
        for _ in 0..3 {
            let member_did = create_test_did();
            handle
                .add_member(coop.id.clone(), member_did, MemberRole::Worker)
                .await
                .unwrap();
        }

        let members = handle.list_members(coop.id).await.unwrap();
        // 1 founder + 3 workers = 4
        assert_eq!(members.len(), 4);
    }

    #[tokio::test]
    async fn test_get_member_coops() {
        let handle = spawn_test_actor();
        let member_did = create_test_did();

        // Create multiple coops and add the same member to each
        for i in 1..=3 {
            let founder = create_test_did();
            let coop = handle
                .create_cooperative(None, format!("Coop {i}"), CoopType::Worker, founder)
                .await
                .unwrap();

            handle
                .add_member(coop.id, member_did.clone(), MemberRole::Worker)
                .await
                .unwrap();
        }

        let coops = handle.get_member_coops(member_did).await.unwrap();
        assert_eq!(coops.len(), 3);
    }

    // === Formation request workflow ===

    #[tokio::test]
    async fn test_create_from_request() {
        let handle = spawn_test_actor();

        let founder1 = create_test_did();
        let founder2 = create_test_did();
        let founder3 = create_test_did();

        let request = FormationRequest::new(
            "My Coop".to_string(),
            CoopType::Worker,
            vec![founder1.clone(), founder2, founder3],
        )
        .with_description("A test cooperative".to_string())
        .with_currency("hours".to_string());

        let (coop, event) = handle
            .create_from_request(request, "formed-coop".to_string(), founder1.clone())
            .await
            .unwrap();

        assert_eq!(coop.id, "formed-coop");
        assert_eq!(coop.name, "My Coop");
        assert_eq!(coop.status, CoopStatus::Forming);
        assert_eq!(coop.min_founders, 3);
        assert_eq!(coop.description, Some("A test cooperative".to_string()));
        assert_eq!(coop.currency, Some("hours".to_string()));

        if let LifecycleEvent::Created { coop_id, founder } = event {
            assert_eq!(coop_id, "formed-coop");
            assert_eq!(founder, founder1);
        } else {
            panic!("Expected Created event");
        }
    }

    #[tokio::test]
    async fn test_sign_charter_workflow() {
        let handle = spawn_test_actor();

        let founder1 = create_test_did();
        let founder2 = create_test_did();
        let founder3 = create_test_did();

        let request = FormationRequest::new(
            "Charter Coop".to_string(),
            CoopType::Worker,
            vec![founder1.clone(), founder2.clone(), founder3.clone()],
        );

        let (coop, _) = handle
            .create_from_request(request, "charter-coop".to_string(), founder1.clone())
            .await
            .unwrap();

        // Sign with first founder
        let sig1 = icn_governance::charter::FounderSignature {
            did: founder1,
            signature: vec![1, 2, 3],
            timestamp: 1000,
            role: Some("initiator".to_string()),
        };
        let (coop, events) = handle.sign_charter(coop.id.clone(), sig1).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(!coop.charter_ratified);

        // Sign with second founder
        let sig2 = icn_governance::charter::FounderSignature {
            did: founder2,
            signature: vec![4, 5, 6],
            timestamp: 1001,
            role: None,
        };
        let (coop, events) = handle.sign_charter(coop.id.clone(), sig2).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(!coop.charter_ratified);

        // Sign with third founder (should ratify)
        let sig3 = icn_governance::charter::FounderSignature {
            did: founder3,
            signature: vec![7, 8, 9],
            timestamp: 1002,
            role: None,
        };
        let (coop, events) = handle.sign_charter(coop.id, sig3).await.unwrap();
        assert_eq!(events.len(), 2); // CharterSigned + CharterRatified
        assert!(coop.charter_ratified);
    }

    // === Dissolution workflow ===

    #[tokio::test]
    async fn test_dissolution_workflow() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        // Create and activate a coop
        let coop = handle
            .create_cooperative(
                None,
                "To Dissolve".to_string(),
                CoopType::Worker,
                founder.clone(),
            )
            .await
            .unwrap();
        let coop = handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();
        assert_eq!(coop.status, CoopStatus::Active);

        // Start dissolution
        let plan = AssetDistributionPlan {
            positive_balance_action: BalanceAction::ReturnToMember,
            negative_balance_action: DebtAction::WriteOff,
            capital_return: CapitalReturnMethod::ProRata,
            residual_recipient: Some("federation:treasury".to_string()),
        };

        let (coop, event) = handle
            .start_dissolution(
                coop.id.clone(),
                founder.clone(),
                plan,
                Some("prop-123".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(coop.status, CoopStatus::Dissolving);
        assert!(coop.dissolution_plan.is_some());
        assert_eq!(coop.dissolution_proposal_id, Some("prop-123".to_string()));

        if let LifecycleEvent::DissolutionStarted {
            initiator,
            proposal_id,
            ..
        } = event
        {
            assert_eq!(initiator, founder);
            assert_eq!(proposal_id, Some("prop-123".to_string()));
        } else {
            panic!("Expected DissolutionStarted event");
        }

        // Complete dissolution
        let (coop, events) = handle.complete_dissolution(coop.id).await.unwrap();

        assert_eq!(coop.status, CoopStatus::Dissolved);
        assert_eq!(events.len(), 2); // AssetsDistributed + Dissolved
    }

    #[tokio::test]
    async fn test_cannot_dissolve_forming_coop() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Forming".to_string(),
                CoopType::Worker,
                founder.clone(),
            )
            .await
            .unwrap();
        assert_eq!(coop.status, CoopStatus::Forming);

        let plan = AssetDistributionPlan::default();
        let result = handle.start_dissolution(coop.id, founder, plan, None).await;

        assert!(result.is_err());
    }

    // === Founder adds themselves on creation ===

    #[tokio::test]
    async fn test_founder_is_member_on_creation() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Test Coop".to_string(),
                CoopType::Worker,
                founder.clone(),
            )
            .await
            .unwrap();

        let members = handle.list_members(coop.id).await.unwrap();
        assert_eq!(members.len(), 1);

        let founder_member = &members[0];
        assert_eq!(founder_member.did, founder);
        assert_eq!(founder_member.role, MemberRole::Founder);
        assert_eq!(founder_member.status, MemberStatus::Active);
    }

    // === Concurrent operations ===

    #[tokio::test]
    async fn test_concurrent_member_additions() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Concurrent Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        // Add members concurrently
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let handle_clone = handle.clone();
            let coop_id = coop.id.clone();
            let member_did = create_test_did();

            tasks.push(tokio::spawn(async move {
                handle_clone
                    .add_member(coop_id, member_did, MemberRole::Worker)
                    .await
            }));
        }

        // Wait for all to complete
        for task in tasks {
            let result = task.await.unwrap();
            assert!(result.is_ok());
        }

        // Verify all members were added
        let members = handle.list_members(coop.id).await.unwrap();
        assert_eq!(members.len(), 11); // 1 founder + 10 workers
    }
}
