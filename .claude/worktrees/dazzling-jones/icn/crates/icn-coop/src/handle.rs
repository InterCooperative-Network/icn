use crate::{
    AssetDistributionPlan, CoopMessage, CoopType, Cooperative, FormationRequest, LifecycleEvent,
    Member, MemberRole, Result,
};
use icn_governance::charter::FounderSignature;
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
            .send(CoopMessage::RemoveMember {
                coop_id,
                did,
                reply,
            })
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

    // === Issue #290: Charter signing and dissolution methods ===

    /// Create a cooperative from a formation request
    ///
    /// This creates a cooperative in Forming state with the specified
    /// founding members. The cooperative must be activated after all
    /// required founders have signed the charter.
    pub async fn create_from_request(
        &self,
        request: FormationRequest,
        id: String,
        first_founder: Did,
    ) -> Result<(Cooperative, LifecycleEvent)> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::CreateFromRequest {
                request,
                id,
                first_founder,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    /// Sign the cooperative's charter as a founder
    ///
    /// Each founder must sign the charter before the cooperative can be
    /// activated. Returns lifecycle events including CharterSigned and
    /// potentially CharterRatified if this was the final required signature.
    pub async fn sign_charter(
        &self,
        coop_id: String,
        signature: FounderSignature,
    ) -> Result<(Cooperative, Vec<LifecycleEvent>)> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::SignCharter {
                coop_id,
                signature,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    /// Start dissolution process with an asset distribution plan
    ///
    /// This transitions the cooperative to Dissolving state. The plan
    /// specifies how balances, debts, and capital should be handled.
    /// Optionally includes a governance proposal ID that approved this.
    pub async fn start_dissolution(
        &self,
        coop_id: String,
        initiator: Did,
        plan: AssetDistributionPlan,
        proposal_id: Option<String>,
    ) -> Result<(Cooperative, LifecycleEvent)> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::StartDissolution {
                coop_id,
                initiator,
                plan,
                proposal_id,
                reply,
            })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    /// Complete the dissolution process
    ///
    /// This should be called after assets have been distributed according
    /// to the plan. Transitions the cooperative to Dissolved state.
    pub async fn complete_dissolution(
        &self,
        coop_id: String,
    ) -> Result<(Cooperative, Vec<LifecycleEvent>)> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::CompleteDissolution { coop_id, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }

    /// Create a treasury for a cooperative
    ///
    /// Creates and assigns a treasury DID to the specified cooperative.
    /// Returns the treasury ID on success.
    pub async fn create_treasury(&self, coop_id: String) -> Result<String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CoopMessage::CreateTreasury { coop_id, reply })
            .await
            .map_err(|_| crate::CoopError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| crate::CoopError::Governance("Reply failed".into()))?
    }
}
