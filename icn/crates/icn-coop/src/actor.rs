use crate::{
    CoopStore, CoopType, Cooperative, LifecycleManager, Member, MemberRole, MembershipManager,
    Result,
};
use icn_gossip::GossipActor;
use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

const _COOP_TOPIC: &str = "coop:updates";

pub struct CoopActor {
    rx: mpsc::Receiver<CoopMessage>,
    store: CoopStore,
    lifecycle: LifecycleManager,
    membership: MembershipManager,
    _gossip: Option<Arc<GossipActor>>,
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
}

impl CoopActor {
    pub fn spawn(store: CoopStore, gossip: Option<Arc<GossipActor>>) -> mpsc::Sender<CoopMessage> {
        let (tx, rx) = mpsc::channel(100);

        let lifecycle = LifecycleManager::new();
        let membership = MembershipManager::new();

        let mut actor = Self {
            rx,
            store,
            lifecycle,
            membership,
            _gossip: gossip,
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

    async fn announce_coop_update(&self, _coop: &Cooperative) {
        // Gossip announcement will be implemented when integrated
        // For now, this is a no-op
    }
}
