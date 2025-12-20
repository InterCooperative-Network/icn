use crate::{CoopMessage, CoopType, Cooperative, Member, MemberRole, Result};
use icn_identity::Did;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct CoopHandle {
    tx: mpsc::Sender<CoopMessage>,
}

impl CoopHandle {
    pub fn new(tx: mpsc::Sender<CoopMessage>) -> Self {
        Self { tx }
    }

    pub async fn create_cooperative(
        &self,
        id: Option<String>,
        name: String,
        coop_type: CoopType,
        founder: Did,
    ) -> Result<Cooperative> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::CreateCooperative {
                id,
                name,
                coop_type,
                founder,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn get_cooperative(&self, coop_id: String) -> Result<Cooperative> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::GetCooperative { coop_id, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn list_cooperatives(&self) -> Result<Vec<Cooperative>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::ListCooperatives { reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn activate_cooperative(
        &self,
        coop_id: String,
        charter_hash: String,
    ) -> Result<Cooperative> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::ActivateCooperative {
                coop_id,
                charter_hash,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn add_member(&self, coop_id: String, did: Did, role: MemberRole) -> Result<Member> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::AddMember {
                coop_id,
                did,
                role,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn approve_member(&self, coop_id: String, did: Did) -> Result<Member> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::ApproveMember {
                coop_id,
                did,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn list_members(&self, coop_id: String) -> Result<Vec<Member>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::ListMembers { coop_id, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn get_member_coops(&self, did: Did) -> Result<Vec<String>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::GetMemberCoops { did, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn delete_cooperative(&self, coop_id: String) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::DeleteCooperative { coop_id, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn remove_member(&self, coop_id: String, did: Did) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::RemoveMember { coop_id, did, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn update_member_role(
        &self,
        coop_id: String,
        did: Did,
        new_role: MemberRole,
    ) -> Result<Member> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::UpdateMemberRole {
                coop_id,
                did,
                new_role,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    pub async fn update_cooperative(
        &self,
        coop_id: String,
        name: Option<String>,
        metadata: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Cooperative> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::UpdateCooperative {
                coop_id,
                name,
                metadata,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }
}
