use crate::{
    CoopStore, CoopType, Cooperative, LifecycleManager, Member, MemberRole, MembershipManager,
    Result,
};
use icn_gossip::GossipActor;
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
            match bincode::serde::encode_to_vec(coop, bincode::config::legacy()) {
                Ok(data) => {
                    // Publish to gossip topic
                    let mut gossip_actor = gossip.write().await;
                    match gossip_actor.publish(COOP_TOPIC, data) {
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
}
