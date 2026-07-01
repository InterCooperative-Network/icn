use crate::{
    AssetDistributionPlan, CoopStore, CoopType, Cooperative, FormationRequest, LifecycleEvent,
    LifecycleManager, Member, MemberRole, MembershipManager, Result,
};
use icn_entity::{
    project_coop_id, CoopEntityBindingProvenance, CoopEntityMap, CoopEntityMapError, EntityId,
};
use icn_gossip::GossipActor;
use icn_governance::charter::FounderSignature;
use icn_identity::Did;
use icn_ledger::TreasuryManager;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

/// Gossip topic for cooperative state updates
pub const COOP_TOPIC: &str = "coop:updates";

/// Handle type for gossip actor
pub type GossipHandle = Arc<RwLock<GossipActor>>;

/// Handle type for treasury manager (shared with governance handlers)
pub type TreasuryManagerHandle = Arc<RwLock<TreasuryManager>>;

/// Handle to the canonical `coop_id ↔ EntityId` name-binding store.
///
/// A binding written through this handle is a **name binding only**: it grants
/// no standing, role, capability, mandate, or permission. Authority in ICN still
/// flows from memberships, charters, roles, capabilities, governance decisions,
/// receipts, and executed effects — never from the existence of a mapping entry.
/// See [`icn_entity::coop_entity_map`].
pub type CoopEntityMapHandle = Arc<dyn CoopEntityMap + Send + Sync>;

/// Outcome of attempting to bind a `coop_id` into the canonical
/// [`CoopEntityMap`] during cooperative activation.
///
/// This is an **observability signal only**. A mapping bind never gates
/// activation success, and a binding confers no authority. Shared by the local
/// activation handler and the gossip coop-update sync path so both perform the
/// same deterministic bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityMapBindOutcome {
    /// `coop_id` was bound (or was already bound) to this cooperative `EntityId`.
    Mapped(EntityId),
    /// `coop_id` is not a valid cooperative `EntityId` slug, so it was left
    /// unbound. Default ids such as `coop:<uuid>` fall here — this is the
    /// expected common case, not an error.
    NotMappable(String),
    /// The binding conflicts with an existing one; nothing was written.
    Conflict(String),
    /// A storage (or other) error occurred while binding; nothing was written.
    StorageError(String),
}

/// Map a `coop_id → EntityId` bind result to the non-fatal [`EntityMapBindOutcome`].
///
/// Shared by both bind entry points: a mapping bind never propagates an error
/// (it is an observability side effect and must not fail activation), so every
/// `Result` is folded into an outcome variant.
fn bind_outcome(result: std::result::Result<EntityId, CoopEntityMapError>) -> EntityMapBindOutcome {
    match result {
        Ok(entity_id) => EntityMapBindOutcome::Mapped(entity_id),
        Err(CoopEntityMapError::NotMappable(reason)) => EntityMapBindOutcome::NotMappable(reason),
        Err(CoopEntityMapError::Conflict(reason)) => EntityMapBindOutcome::Conflict(reason),
        Err(e) => EntityMapBindOutcome::StorageError(e.to_string()),
    }
}

/// Record a non-authoritative `coop_id → EntityId` name binding with **no trusted
/// provenance** (it reads back as the fail-closed
/// [`CoopEntityBindingProvenance::UnknownLegacy`] sentinel).
///
/// This is the bind for **untrusted** sources — specifically the gossip
/// coop-update mirror, which replicates an unauthenticated `coop:updates` payload
/// (its `status` field is not an activation proof and the entry author is not
/// verified). Recording trusted `Activation` provenance here would let any topic
/// publisher poison the durable map with a row a future store-backed resolver
/// would treat as trusted, so this path deliberately leaves the row untrusted.
/// Use [`bind_coop_entity_map_activation`] only from a path that authoritatively
/// performed the activation.
///
/// Reject-not-normalize: a non-mappable `coop_id` (e.g. the default `coop:<uuid>`
/// shape) yields [`EntityMapBindOutcome::NotMappable`] and writes nothing. Idempotent
/// for an identical pair. A binding grants no authority.
pub fn bind_coop_entity_map(
    map: &(dyn CoopEntityMap + Send + Sync),
    coop_id: &str,
) -> EntityMapBindOutcome {
    // `bind_projected` records no provenance, so the row reads back as
    // `UnknownLegacy` (untrusted) — correct for an unverified gossip-sourced bind.
    bind_outcome(map.bind_projected(coop_id))
}

/// Record a `coop_id → EntityId` name binding with trusted
/// [`CoopEntityBindingProvenance::Activation`] provenance.
///
/// Call this **only** from a path that authoritatively performed (or witnessed)
/// the cooperative's activation — i.e. the local activation handler. The trusted
/// `Activation` provenance is what lets the merged `StoreBackedCoopEntityResolver`
/// (#2192) resolve the row; it must never be written for a binding derived from an
/// unauthenticated source (see [`bind_coop_entity_map`] for the gossip mirror).
///
/// Returns the [`EntityMapBindOutcome`] without ever propagating an error — a
/// mapping bind must not fail activation. Reject-not-normalize: a non-mappable
/// `coop_id` yields [`EntityMapBindOutcome::NotMappable`] and writes nothing.
/// Re-binding the identical pair with `Activation` is idempotent; a pre-provenance
/// (`UnknownLegacy`) row from an older binary or the gossip mirror is upgraded in
/// place. A binding still grants no authority.
pub fn bind_coop_entity_map_activation(
    map: &(dyn CoopEntityMap + Send + Sync),
    coop_id: &str,
) -> EntityMapBindOutcome {
    // Reject-not-normalize projection (the same gate `bind_projected` applies),
    // then a provenance-aware write recording trusted `Activation`.
    let result = project_coop_id(coop_id).and_then(|entity_id| {
        map.bind_resolved_with_provenance(
            coop_id,
            &entity_id,
            CoopEntityBindingProvenance::Activation,
        )
        .map(|()| entity_id)
    });
    bind_outcome(result)
}

pub struct CoopActor {
    rx: mpsc::Receiver<CoopMessage>,
    store: CoopStore,
    lifecycle: LifecycleManager,
    membership: MembershipManager,
    gossip: Option<GossipHandle>,
    /// Optional treasury manager for registering treasury accounts in the ledger
    treasury_manager: Option<TreasuryManagerHandle>,
    /// Optional canonical `coop_id ↔ EntityId` name-binding store. When present,
    /// it is populated during activation as a non-authoritative side effect.
    coop_entity_map: Option<CoopEntityMapHandle>,
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
    /// Create a treasury for a cooperative
    CreateTreasury {
        coop_id: String,
        /// Reply channel for the created treasury ID
        reply: oneshot::Sender<Result<String>>,
    },
}

impl CoopActor {
    pub fn spawn(store: CoopStore, gossip: Option<GossipHandle>) -> mpsc::Sender<CoopMessage> {
        Self::spawn_with_treasury(store, gossip, None)
    }

    /// Spawn actor with optional treasury manager for ledger integration
    pub fn spawn_with_treasury(
        store: CoopStore,
        gossip: Option<GossipHandle>,
        treasury_manager: Option<TreasuryManagerHandle>,
    ) -> mpsc::Sender<CoopMessage> {
        Self::spawn_with_treasury_and_map(store, gossip, treasury_manager, None)
    }

    /// Spawn actor with optional treasury manager and optional canonical
    /// `coop_id ↔ EntityId` name-binding store.
    ///
    /// When `coop_entity_map` is `Some`, cooperative activation records a
    /// non-authoritative `coop_id ↔ EntityId` binding as a side effect (a
    /// binding grants no permission, and a bind failure never fails activation).
    /// When `None`, behavior is identical to [`Self::spawn_with_treasury`].
    pub fn spawn_with_treasury_and_map(
        store: CoopStore,
        gossip: Option<GossipHandle>,
        treasury_manager: Option<TreasuryManagerHandle>,
        coop_entity_map: Option<CoopEntityMapHandle>,
    ) -> mpsc::Sender<CoopMessage> {
        let (tx, rx) = mpsc::channel(100);

        let lifecycle = LifecycleManager::new();
        let membership = MembershipManager::new();

        let mut actor = Self {
            rx,
            store,
            lifecycle,
            membership,
            gossip,
            treasury_manager,
            coop_entity_map,
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
                CoopMessage::CreateTreasury { coop_id, reply } => {
                    let result = self.handle_create_treasury(&coop_id).await;
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
        let (mut coop, treasury_id) = self.lifecycle.activate(coop, charter_hash).await?;

        // Assign treasury DID onto cooperative if not already set (idempotency guard).
        // This must happen before persisting so the saved record is complete.
        if coop.treasury_did.is_none() {
            let treasury_did_str = crate::lifecycle::derive_treasury_did(&coop_id);

            coop.assign_treasury(treasury_did_str.clone())
                .map_err(crate::CoopError::Governance)?;

            // Register treasury account in ledger if treasury manager is available
            if let Some(ref treasury_mgr) = self.treasury_manager {
                let anchor = crate::lifecycle::derive_treasury_anchor(&coop_id);
                let mut anchor_32 = [0u8; 32];
                anchor_32[..16].copy_from_slice(&anchor);
                let treasury_did = Did::from_anchor_id(&anchor_32);

                // Idempotency guard: if treasury is already registered (e.g. a prior activation
                // attempt crashed after register_treasury but before save_cooperative), skip
                // registration so that retries do not fail forever.
                let already_registered = {
                    let guard = treasury_mgr.read().await;
                    guard.get_treasury_by_coop(&coop_id).is_some()
                };

                if already_registered {
                    tracing::info!(
                        coop_id = %coop_id,
                        treasury_did = %treasury_did,
                        "Treasury already registered in ledger; skipping duplicate registration"
                    );
                } else {
                    let created_by = self
                        .store
                        .list_members(&coop_id)
                        .ok()
                        .and_then(|members| members.first().map(|m| m.did.clone()))
                        .unwrap_or_else(|| treasury_did.clone());

                    let description = Some(format!("Treasury for cooperative {}", coop_id));
                    let mut treasury_guard = treasury_mgr.write().await;

                    // #2082: a treasury is born with its canonical cooperative
                    // EntityId when the coop_id directly projects to a cooperative
                    // slug — the same trusted, reject-not-normalize projection the
                    // Activation-provenance map binding (below) records, so the two
                    // agree. A non-projectable coop_id keeps entity_id: None (no
                    // guessing); it is surrogate-bound and backfilled later. This
                    // sets an identity target only: it grants no authority and does
                    // not change enforcement mode/defaults.
                    match project_coop_id(&coop_id) {
                        Ok(entity_id) => {
                            treasury_guard
                                .register_treasury_with_entity(
                                    treasury_did.clone(),
                                    entity_id,
                                    "HOURS".to_string(),
                                    created_by,
                                    description,
                                )
                                .map_err(|e| crate::CoopError::Ledger(e.to_string()))?;
                        }
                        Err(_) => {
                            treasury_guard
                                .register_treasury(
                                    treasury_did.clone(),
                                    coop_id.clone(),
                                    "HOURS".to_string(),
                                    created_by,
                                    description,
                                )
                                .map_err(|e| crate::CoopError::Ledger(e.to_string()))?;
                        }
                    }

                    tracing::info!(
                        coop_id = %coop_id,
                        treasury_id = %treasury_id,
                        treasury_did = %treasury_did,
                        "Registered treasury in ledger during activation"
                    );
                }
            }
        }

        self.store.save_cooperative(&coop)?;

        // Non-authoritative name binding (#2082 PR2): record the canonical
        // `coop_id ↔ EntityId` mapping when a map is wired. A binding grants NO
        // authority, and a bind failure — including the common NotMappable case
        // for default `coop:<uuid>` ids — is reported but never fails activation.
        if let Some(ref map) = self.coop_entity_map {
            match bind_coop_entity_map_activation(map.as_ref(), &coop_id) {
                EntityMapBindOutcome::Mapped(entity_id) => tracing::info!(
                    target: "coop_entity_bind",
                    coop_id = %coop_id,
                    entity_id = %entity_id,
                    outcome = "mapped",
                    "bound coop_id to cooperative EntityId during activation (name binding only; grants no authority)"
                ),
                EntityMapBindOutcome::NotMappable(reason) => tracing::info!(
                    target: "coop_entity_bind",
                    coop_id = %coop_id,
                    outcome = "not_mappable",
                    reason = %reason,
                    "coop_id is not a mappable cooperative EntityId slug; left unbound; activation unaffected"
                ),
                EntityMapBindOutcome::Conflict(reason) => tracing::error!(
                    target: "coop_entity_bind",
                    coop_id = %coop_id,
                    outcome = "conflict",
                    reason = %reason,
                    "coop_id/entity mapping conflict during activation; binding skipped; activation NOT failed"
                ),
                EntityMapBindOutcome::StorageError(reason) => tracing::error!(
                    target: "coop_entity_bind",
                    coop_id = %coop_id,
                    outcome = "storage_error",
                    reason = %reason,
                    "coop_entity_map bind failed during activation; binding skipped; activation NOT failed"
                ),
            }
        }

        tracing::info!(
            coop_id = %coop.id,
            treasury_id = %treasury_id,
            "Cooperative activated with treasury"
        );

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

    async fn handle_create_treasury(&mut self, coop_id: &str) -> Result<String> {
        // Verify coop exists
        let mut coop = self.store.get_cooperative(coop_id)?;

        // Check if treasury already exists
        if coop.treasury_did.is_some() {
            return Err(crate::CoopError::Governance(format!(
                "Cooperative {} already has a treasury",
                coop_id
            )));
        }

        // Create treasury with derived DID
        let treasury_did_str = crate::lifecycle::derive_treasury_did(coop_id);
        let treasury_id = format!("treasury:{}", coop_id);

        // Assign treasury to cooperative
        coop.assign_treasury(treasury_did_str.clone())
            .map_err(crate::CoopError::Governance)?;

        // Register treasury account in ledger if treasury manager is available
        if let Some(ref treasury_mgr) = self.treasury_manager {
            // Create a DID from the treasury anchor for ledger registration
            let anchor = crate::lifecycle::derive_treasury_anchor(coop_id);
            let mut anchor_32 = [0u8; 32];
            anchor_32[..16].copy_from_slice(&anchor);
            let treasury_did = Did::from_anchor_id(&anchor_32);

            // Get the founder DID as the creator (first member of the coop)
            let created_by = self
                .store
                .list_members(coop_id)
                .ok()
                .and_then(|members| members.first().map(|m| m.did.clone()))
                .unwrap_or_else(|| treasury_did.clone());

            let mut treasury_guard = treasury_mgr.write().await;
            treasury_guard
                .register_treasury(
                    treasury_did.clone(),
                    coop_id.to_string(),
                    "HOURS".to_string(), // Default cooperative currency
                    created_by,
                    Some(format!("Treasury for cooperative {}", coop_id)),
                )
                .map_err(|e| crate::CoopError::Ledger(e.to_string()))?;

            tracing::info!(
                coop_id,
                treasury_id = %treasury_id,
                treasury_did = %treasury_did,
                "Created treasury account in ledger for cooperative"
            );
        }

        // Save updated cooperative
        self.store.save_cooperative(&coop)?;

        tracing::info!(
            coop_id,
            treasury_id = %treasury_id,
            treasury_did = %treasury_did_str,
            "Created treasury for cooperative"
        );

        // Announce to network
        self.announce_coop_update(&coop).await;

        Ok(treasury_id)
    }

    async fn announce_coop_update(&self, coop: &Cooperative) {
        if let Some(gossip) = &self.gossip {
            // Serialize the cooperative for gossip
            match icn_encoding::encode(coop) {
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
    use icn_entity::InMemoryCoopEntityMap;
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

    // === Single-writer atomicity validation (PR-P1) ===

    /// Validates that the CoopActor single-writer pattern serializes
    /// concurrent mutations correctly: interleaved create + add_member
    /// operations across multiple cooperatives must each see consistent state.
    #[tokio::test]
    async fn test_single_writer_serialization_across_coops() {
        let handle = spawn_test_actor();

        // Create 5 coops concurrently — each gets a unique ID
        let mut create_tasks = Vec::new();
        for i in 0..5 {
            let h = handle.clone();
            let founder = create_test_did();
            create_tasks.push(tokio::spawn(async move {
                h.create_cooperative(None, format!("Coop-{}", i), CoopType::Worker, founder)
                    .await
            }));
        }

        let mut coop_ids = Vec::new();
        for t in create_tasks {
            let coop = t.await.unwrap().unwrap();
            coop_ids.push(coop.id);
        }

        // All 5 coops should have unique IDs and be retrievable
        assert_eq!(coop_ids.len(), 5);
        for cid in &coop_ids {
            let result = handle.get_cooperative(cid.clone()).await;
            assert!(result.is_ok());
        }
    }

    /// Validates that concurrent add_member + get_cooperative
    /// operations don't produce stale or inconsistent reads.
    #[tokio::test]
    async fn test_single_writer_read_after_write_consistency() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Read-Write Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        // Interleave writes and reads concurrently
        let mut tasks = Vec::new();
        for i in 0..20 {
            let h = handle.clone();
            let cid = coop.id.clone();
            if i % 2 == 0 {
                // Write: add a member
                let member_did = create_test_did();
                tasks.push(tokio::spawn(async move {
                    h.add_member(cid, member_did, MemberRole::Worker)
                        .await
                        .map(|_| ())
                }));
            } else {
                // Read: list members
                tasks.push(tokio::spawn(async move {
                    h.list_members(cid).await.map(|_| ())
                }));
            }
        }

        // All operations should succeed — no panics, no data corruption
        for t in tasks {
            let result = t.await.unwrap();
            assert!(result.is_ok());
        }

        // Final state should show all 10 added members + 1 founder
        let members = handle.list_members(coop.id).await.unwrap();
        assert_eq!(members.len(), 11); // 1 founder + 10 workers
    }

    /// Validates that duplicate member additions are serialized through the actor.
    /// Same DID added concurrently results in exactly one entry (upsert semantics).
    #[tokio::test]
    async fn test_single_writer_upsert_semantics() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Upsert Coop".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        let member_did = create_test_did();

        // Try adding the same member 10 times concurrently
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let h = handle.clone();
            let cid = coop.id.clone();
            let did = member_did.clone();
            tasks.push(tokio::spawn(async move {
                h.add_member(cid, did, MemberRole::Worker).await
            }));
        }

        // All operations go through the single-writer actor sequentially
        for t in tasks {
            // All should succeed (upsert semantics)
            let _ = t.await.unwrap();
        }

        // Final state: founder + 1 unique member (not 10 duplicates)
        let members = handle.list_members(coop.id).await.unwrap();
        assert_eq!(
            members.len(),
            2,
            "Upsert should produce exactly 1 member entry"
        );
    }

    // === Treasury creation tests ===

    #[tokio::test]
    async fn test_create_treasury_success() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        // Create a cooperative first
        let coop = handle
            .create_cooperative(
                None,
                "Treasury Test Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        // Create treasury for the cooperative
        let treasury_id = handle.create_treasury(coop.id.clone()).await.unwrap();

        // Verify treasury ID format
        assert_eq!(treasury_id, format!("treasury:{}", coop.id));

        // Verify the cooperative now has a treasury DID
        let updated_coop = handle.get_cooperative(coop.id).await.unwrap();
        assert!(updated_coop.treasury_did.is_some());
        assert!(updated_coop
            .treasury_did
            .unwrap()
            .starts_with("did:icn:treasury:"));
    }

    #[tokio::test]
    async fn test_create_treasury_rejects_duplicate() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        // Create a cooperative
        let coop = handle
            .create_cooperative(
                None,
                "Dup Treasury Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        // Create treasury - first time should succeed
        let result = handle.create_treasury(coop.id.clone()).await;
        assert!(result.is_ok());

        // Create treasury again - should fail
        let result = handle.create_treasury(coop.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_treasury_nonexistent_coop() {
        let handle = spawn_test_actor();

        // Try to create treasury for a non-existent cooperative
        let result = handle
            .create_treasury("nonexistent-coop-id".to_string())
            .await;
        assert!(result.is_err());
    }

    // === Treasury-Ledger integration tests (B1.3) ===

    fn spawn_test_actor_with_treasury() -> (CoopHandle, TreasuryManagerHandle) {
        let store = create_test_store();
        let treasury_mgr = Arc::new(RwLock::new(icn_ledger::TreasuryManager::new()));
        let tx = CoopActor::spawn_with_treasury(store, None, Some(treasury_mgr.clone()));
        (CoopHandle::new(tx), treasury_mgr)
    }

    #[tokio::test]
    async fn test_create_treasury_registers_in_ledger() {
        let (handle, treasury_mgr) = spawn_test_actor_with_treasury();
        let founder = create_test_did();

        // Create a cooperative
        let coop = handle
            .create_cooperative(
                Some("ledger-test-coop".to_string()),
                "Ledger Treasury Test Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        // Create treasury for the cooperative
        let treasury_id = handle.create_treasury(coop.id.clone()).await.unwrap();
        assert_eq!(treasury_id, format!("treasury:{}", coop.id));

        // Verify treasury was registered in the TreasuryManager
        let guard = treasury_mgr.read().await;
        let treasury = guard.get_treasury_by_coop(&coop.id);
        assert!(
            treasury.is_some(),
            "Treasury should be registered in ledger"
        );

        let treasury = treasury.unwrap();
        assert_eq!(treasury.coop_id, coop.id);
        assert_eq!(treasury.currency, "HOURS");
        assert!(treasury.is_active);
    }

    #[tokio::test]
    async fn test_treasury_account_shows_zero_balance() {
        let (handle, treasury_mgr) = spawn_test_actor_with_treasury();
        let founder = create_test_did();

        // Create cooperative and treasury
        let coop = handle
            .create_cooperative(
                Some("zero-balance-coop".to_string()),
                "Zero Balance Coop".to_string(),
                CoopType::Consumer,
                founder,
            )
            .await
            .unwrap();
        handle.create_treasury(coop.id.clone()).await.unwrap();

        // Verify treasury exists and has zero initial state (no budgets spent)
        let guard = treasury_mgr.read().await;
        let treasury = guard.get_treasury_by_coop(&coop.id).unwrap();

        // Treasury accounts start with no budgets
        let budgets = guard.list_budgets(&treasury.treasury_did);
        assert!(budgets.is_empty(), "New treasury should have no budgets");
    }

    #[tokio::test]
    async fn test_treasury_metadata_contains_coop_id() {
        let (handle, treasury_mgr) = spawn_test_actor_with_treasury();
        let founder = create_test_did();

        // Create cooperative with specific ID
        let coop = handle
            .create_cooperative(
                Some("metadata-test-coop".to_string()),
                "Metadata Test Coop".to_string(),
                CoopType::Platform,
                founder,
            )
            .await
            .unwrap();
        handle.create_treasury(coop.id.clone()).await.unwrap();

        // Verify treasury metadata
        let guard = treasury_mgr.read().await;
        let treasury = guard.get_treasury_by_coop(&coop.id).unwrap();

        assert_eq!(treasury.coop_id, "metadata-test-coop");
        assert!(
            treasury.description.is_some(),
            "Treasury should have description"
        );
        assert!(
            treasury.description.as_ref().unwrap().contains(&coop.id),
            "Description should contain coop ID"
        );
    }

    #[tokio::test]
    async fn test_treasury_works_without_ledger_manager() {
        // Test that treasury creation still works when no TreasuryManager is provided
        let handle = spawn_test_actor(); // Uses spawn() without treasury manager
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                Some("no-ledger-coop".to_string()),
                "No Ledger Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        // Treasury creation should succeed (just no ledger registration)
        let treasury_id = handle.create_treasury(coop.id.clone()).await.unwrap();
        assert_eq!(treasury_id, format!("treasury:{}", coop.id));

        // Verify the coop has a treasury DID assigned
        let updated_coop = handle.get_cooperative(coop.id).await.unwrap();
        assert!(updated_coop.treasury_did.is_some());
    }

    // === Treasury assignment during activation (t10 — single-writer atomicity) ===

    #[tokio::test]
    async fn test_activate_cooperative_assigns_treasury_did() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(None, "Activate Test".to_string(), CoopType::Worker, founder)
            .await
            .unwrap();

        assert!(
            coop.treasury_did.is_none(),
            "treasury_did must be None before activation"
        );

        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter-hash".to_string())
            .await
            .unwrap();

        assert!(
            activated.treasury_did.is_some(),
            "treasury_did must be Some after activation"
        );
        let expected = crate::lifecycle::derive_treasury_did(&coop.id);
        assert_eq!(activated.treasury_did.unwrap().as_str(), expected);
    }

    #[tokio::test]
    async fn test_activate_cooperative_registers_treasury_in_ledger() {
        let (handle, treasury_mgr) = spawn_test_actor_with_treasury();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                Some("activate-ledger-coop".to_string()),
                "Ledger Registration Test".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter-ledger".to_string())
            .await
            .unwrap();

        assert!(activated.treasury_did.is_some());

        let guard = treasury_mgr.read().await;
        let treasury = guard.get_treasury_by_coop(&coop.id);
        assert!(
            treasury.is_some(),
            "Treasury must be registered in ledger on activation"
        );
        let treasury = treasury.unwrap();
        assert_eq!(treasury.coop_id, coop.id);
        assert_eq!(treasury.currency, "HOURS");
        assert!(treasury.is_active);
    }

    #[tokio::test]
    async fn test_activate_then_create_treasury_rejects_duplicate() {
        let handle = spawn_test_actor();
        let founder = create_test_did();

        let coop = handle
            .create_cooperative(
                None,
                "Dup Guard Test".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();

        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter-dup".to_string())
            .await
            .unwrap();
        assert!(activated.treasury_did.is_some());

        // CreateTreasury after activation must be rejected
        let result = handle.create_treasury(coop.id).await;
        assert!(
            result.is_err(),
            "CreateTreasury after activation must be rejected — treasury already exists"
        );
    }

    // === Entity-map binding during activation (#2082 PR2) ===
    //
    // A binding is a non-authoritative name binding only. These tests prove that
    // activation populates the canonical CoopEntityMap when the id is mappable,
    // reports (and never fails on) non-mappable ids, and grants no authority.

    fn map_handle(map: &Arc<InMemoryCoopEntityMap>) -> CoopEntityMapHandle {
        map.clone()
    }

    fn spawn_test_actor_with_map() -> (CoopHandle, Arc<InMemoryCoopEntityMap>) {
        let store = create_test_store();
        let map = Arc::new(InMemoryCoopEntityMap::new());
        let tx = CoopActor::spawn_with_treasury_and_map(store, None, None, Some(map_handle(&map)));
        (CoopHandle::new(tx), map)
    }

    fn spawn_test_actor_with_treasury_and_map() -> (
        CoopHandle,
        TreasuryManagerHandle,
        Arc<InMemoryCoopEntityMap>,
    ) {
        let store = create_test_store();
        let treasury_mgr = Arc::new(RwLock::new(icn_ledger::TreasuryManager::new()));
        let map = Arc::new(InMemoryCoopEntityMap::new());
        let tx = CoopActor::spawn_with_treasury_and_map(
            store,
            None,
            Some(treasury_mgr.clone()),
            Some(map_handle(&map)),
        );
        (CoopHandle::new(tx), treasury_mgr, map)
    }

    // T1: a mappable coop_id is bound (forward + reverse) on activation.
    #[tokio::test]
    async fn test_activation_binds_mappable_coop_id() {
        let (handle, map) = spawn_test_actor_with_map();
        let founder = create_test_did();
        let coop = handle
            .create_cooperative(
                Some("good-coop".to_string()),
                "Good Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();
        handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();

        let entity = EntityId::cooperative("good-coop").unwrap();
        assert_eq!(
            map.entity_for_coop("good-coop").unwrap(),
            Some(entity.clone())
        );
        assert_eq!(
            map.coop_for_entity(&entity).unwrap(),
            Some("good-coop".to_string())
        );
    }

    // T2: the default `coop:<uuid>` id is non-mappable; activation must NOT fail
    // and must write nothing.
    #[tokio::test]
    async fn test_activation_non_mappable_default_id_does_not_fail() {
        let (handle, map) = spawn_test_actor_with_map();
        let founder = create_test_did();
        // Default-generated id is `coop:<uuid>` (the colon makes it a non-slug).
        let coop = handle
            .create_cooperative(
                None,
                "Default Id Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();
        assert!(
            coop.id.starts_with("coop:"),
            "default cooperative id should be coop:<uuid>, got {}",
            coop.id
        );

        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();
        assert_eq!(activated.id, coop.id, "activation must succeed unchanged");
        assert_eq!(
            map.entity_for_coop(&coop.id).unwrap(),
            None,
            "a non-mappable id must not be bound"
        );
    }

    // T6: existing treasury registration behavior is preserved, and activation now
    // populates treasury entity_id for a projectable coop_id (#2082).
    #[tokio::test]
    async fn test_activation_with_map_preserves_treasury_behavior() {
        let (handle, treasury_mgr, map) = spawn_test_actor_with_treasury_and_map();
        let founder = create_test_did();
        let coop = handle
            .create_cooperative(
                Some("treasury-coop".to_string()),
                "Treasury Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();
        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();
        assert!(activated.treasury_did.is_some());

        let guard = treasury_mgr.read().await;
        let treasury = guard
            .get_treasury_by_coop(&coop.id)
            .expect("treasury must still register during activation");
        assert_eq!(treasury.coop_id, coop.id);
        assert!(treasury.is_active);

        // Activation now populates treasury entity_id from the trusted, projectable
        // binding (#2082): the treasury is born with the same cooperative EntityId
        // that the Activation-provenance map binding records.
        let entity = EntityId::cooperative("treasury-coop").unwrap();
        assert_eq!(treasury.entity_id(), Some(&entity));

        // ...and the mappable coop_id is still bound as a pure name binding.
        assert_eq!(map.entity_for_coop("treasury-coop").unwrap(), Some(entity));
    }

    // T7: an actor without a map handle behaves exactly as before.
    #[tokio::test]
    async fn test_activation_without_map_handle_unchanged() {
        let handle = spawn_test_actor(); // spawn() -> no treasury, no map
        let founder = create_test_did();
        let coop = handle
            .create_cooperative(
                Some("no-map-coop".to_string()),
                "No Map Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();
        let activated = handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();
        assert_eq!(activated.id, coop.id);
        assert!(activated.treasury_did.is_some());
    }

    // === Activation-time treasury entity_id population (#2082) ===
    //
    // A treasury created during activation is born with the SAME cooperative
    // EntityId the Activation-provenance map binding records — but ONLY when the
    // coop_id directly projects to a cooperative slug (reject-not-normalize). A
    // non-projectable coop_id keeps entity_id: None (no guessing); it is bound via
    // a surrogate and backfilled later. Population sets an identity target only and
    // grants no authority; enforcement mode/defaults are untouched.

    // A1: a projectable coop_id => the activation treasury is born with entity_id,
    // its legacy coop_id is preserved byte-for-byte, and it agrees with the map.
    #[tokio::test]
    async fn test_activation_populates_treasury_entity_id_for_projectable_coop_id() {
        let (handle, treasury_mgr, map) = spawn_test_actor_with_treasury_and_map();
        let founder = create_test_did();
        let coop = handle
            .create_cooperative(
                Some("activation-entity-coop".to_string()),
                "Activation Entity Coop".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();
        handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();

        let expected = EntityId::cooperative("activation-entity-coop").unwrap();

        let guard = treasury_mgr.read().await;
        let treasury = guard
            .get_treasury_by_coop("activation-entity-coop")
            .expect("treasury registered at activation");
        // Born with the canonical cooperative EntityId...
        assert_eq!(treasury.entity_id(), Some(&expected));
        // ...while the legacy coop_id is preserved byte-for-byte (no normalization).
        assert_eq!(treasury.coop_id(), "activation-entity-coop");
        // ...resolvable via the entity index.
        assert_eq!(
            guard
                .get_treasury_by_entity(&expected)
                .map(|t| t.coop_id().to_string()),
            Some("activation-entity-coop".to_string())
        );
        // ...and consistent with the durable Activation-provenance map binding.
        assert_eq!(
            map.entity_for_coop("activation-entity-coop").unwrap(),
            Some(expected)
        );
    }

    // A2: a non-projectable coop_id must NOT guess an entity_id — the treasury
    // stays legacy (entity_id: None), exactly as before, and the map stays empty.
    #[tokio::test]
    async fn test_activation_does_not_populate_entity_id_for_non_projectable_coop_id() {
        let (handle, treasury_mgr, map) = spawn_test_actor_with_treasury_and_map();
        let founder = create_test_did();
        // Default-generated id is `coop:<uuid>` — the colon makes it a non-slug.
        let coop = handle
            .create_cooperative(
                None,
                "Non Projectable".to_string(),
                CoopType::Worker,
                founder,
            )
            .await
            .unwrap();
        assert!(
            coop.id.starts_with("coop:"),
            "expected default coop:<uuid> id, got {}",
            coop.id
        );
        handle
            .activate_cooperative(coop.id.clone(), "charter".to_string())
            .await
            .unwrap();

        let guard = treasury_mgr.read().await;
        let treasury = guard
            .get_treasury_by_coop(&coop.id)
            .expect("treasury registered at activation");
        assert!(
            treasury.entity_id().is_none(),
            "a non-projectable coop_id must not receive a guessed entity_id"
        );
        assert_eq!(treasury.coop_id(), coop.id);
        assert_eq!(
            map.entity_for_coop(&coop.id).unwrap(),
            None,
            "a non-mappable id must not be bound"
        );
    }

    // === bind_coop_entity_map outcome helper (pure, no actor) ===

    // T3a: a mappable id yields Mapped and writes the binding.
    #[test]
    fn test_bind_outcome_mapped_writes_binding() {
        let map = InMemoryCoopEntityMap::new();
        let outcome = bind_coop_entity_map(&map, "good-coop");
        assert_eq!(
            outcome,
            EntityMapBindOutcome::Mapped(EntityId::cooperative("good-coop").unwrap())
        );
        assert_eq!(
            map.entity_for_coop("good-coop").unwrap(),
            Some(EntityId::cooperative("good-coop").unwrap())
        );
    }

    // T3b: non-mappable ids (default colon shape, uppercase/underscore) yield
    // NotMappable and write nothing — this is the reported, expected path.
    #[test]
    fn test_bind_outcome_not_mappable_writes_nothing() {
        let map = InMemoryCoopEntityMap::new();
        for id in ["coop:11111111-2222-3333-4444-555555555555", "coop_A"] {
            let outcome = bind_coop_entity_map(&map, id);
            assert!(
                matches!(outcome, EntityMapBindOutcome::NotMappable(_)),
                "{id} should be NotMappable, got {outcome:?}"
            );
            assert_eq!(map.entity_for_coop(id).unwrap(), None);
        }
    }

    // T4: repeated bind of an identical pair is idempotent.
    #[test]
    fn test_bind_outcome_idempotent_for_same_pair() {
        let map = InMemoryCoopEntityMap::new();
        let first = bind_coop_entity_map(&map, "good-coop");
        let second = bind_coop_entity_map(&map, "good-coop");
        assert_eq!(first, second);
        assert_eq!(
            second,
            EntityMapBindOutcome::Mapped(EntityId::cooperative("good-coop").unwrap())
        );
    }

    // T5: a conflict is reported and grants no authority — the pre-existing
    // binding is untouched and no one-sided write is made.
    #[test]
    fn test_bind_outcome_conflict_is_reported_and_grants_no_authority() {
        let map = InMemoryCoopEntityMap::new();
        // Pre-seed a conflicting forward binding: good-coop -> other-coop entity.
        map.bind_exact("good-coop", &EntityId::cooperative("other-coop").unwrap())
            .unwrap();

        // An activation-style bind projects good-coop -> good-coop, which conflicts.
        let outcome = bind_coop_entity_map(&map, "good-coop");
        assert!(
            matches!(outcome, EntityMapBindOutcome::Conflict(_)),
            "expected Conflict, got {outcome:?}"
        );
        // The pre-existing binding is unchanged.
        assert_eq!(
            map.entity_for_coop("good-coop").unwrap(),
            Some(EntityId::cooperative("other-coop").unwrap())
        );
        // No one-sided reverse write was made for good-coop's own projected entity.
        assert_eq!(
            map.coop_for_entity(&EntityId::cooperative("good-coop").unwrap())
                .unwrap(),
            None
        );
    }

    // === A2c: activation bindings record real `Activation` provenance ===

    /// A `CoopEntityMap` whose writes always fail, to prove a storage error during
    /// the provenance-aware activation bind is reported (`StorageError`) and never
    /// propagated — a mapping bind must not fail activation.
    struct FailingCoopEntityMap;

    impl CoopEntityMap for FailingCoopEntityMap {
        fn bind_resolved(
            &self,
            _coop_id: &str,
            _entity_id: &EntityId,
        ) -> std::result::Result<(), CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "simulated backend failure".into(),
            ))
        }
        fn entity_for_coop(
            &self,
            _coop_id: &str,
        ) -> std::result::Result<Option<EntityId>, CoopEntityMapError> {
            Ok(None)
        }
        fn coop_for_entity(
            &self,
            _entity_id: &EntityId,
        ) -> std::result::Result<Option<String>, CoopEntityMapError> {
            Ok(None)
        }
    }

    // T6a: a mappable activation bind records `Activation` provenance (not the
    // fail-closed `UnknownLegacy` that the old plain `bind_projected` left behind).
    #[test]
    fn test_bind_outcome_records_activation_provenance() {
        let map = InMemoryCoopEntityMap::new();
        let outcome = bind_coop_entity_map_activation(&map, "good-coop");
        assert_eq!(
            outcome,
            EntityMapBindOutcome::Mapped(EntityId::cooperative("good-coop").unwrap())
        );
        let binding = map
            .binding_for_coop("good-coop")
            .unwrap()
            .expect("binding present after activation bind");
        assert_eq!(binding.provenance, CoopEntityBindingProvenance::Activation);
    }

    // T6b: repeating the identical activation bind is idempotent and keeps the
    // `Activation` provenance (re-recording the same provenance is not a Conflict).
    #[test]
    fn test_bind_outcome_idempotent_keeps_activation_provenance() {
        let map = InMemoryCoopEntityMap::new();
        let first = bind_coop_entity_map_activation(&map, "good-coop");
        let second = bind_coop_entity_map_activation(&map, "good-coop");
        assert_eq!(first, second);
        assert_eq!(
            second,
            EntityMapBindOutcome::Mapped(EntityId::cooperative("good-coop").unwrap())
        );
        assert_eq!(
            map.binding_for_coop("good-coop")
                .unwrap()
                .unwrap()
                .provenance,
            CoopEntityBindingProvenance::Activation
        );
    }

    // T6c: a storage error during the bind is reported as `StorageError` and never
    // propagated — the activation contract (bind is non-fatal) is preserved.
    #[test]
    fn test_bind_outcome_storage_error_is_reported_not_propagated() {
        let outcome = bind_coop_entity_map_activation(&FailingCoopEntityMap, "good-coop");
        assert!(
            matches!(outcome, EntityMapBindOutcome::StorageError(_)),
            "expected StorageError, got {outcome:?}"
        );
    }

    // T6d: the activation-written binding satisfies every trust gate the merged
    // `StoreBackedCoopEntityResolver` (#2192) requires to resolve — trusted
    // `Activation` provenance, a cooperative target, and a consistent reverse index.
    // (The resolver actually resolving such a binding is covered by the icn-gateway
    // test `store_backed_resolves_binding_with_trusted_provenance`.)
    #[test]
    fn test_activation_binding_satisfies_resolver_trust_gates() {
        let map = InMemoryCoopEntityMap::new();
        assert!(matches!(
            bind_coop_entity_map_activation(&map, "good-coop"),
            EntityMapBindOutcome::Mapped(_)
        ));
        let entity = EntityId::cooperative("good-coop").unwrap();
        let binding = map.binding_for_coop("good-coop").unwrap().unwrap();
        // (1) trusted provenance
        assert_eq!(binding.provenance, CoopEntityBindingProvenance::Activation);
        // (2) cooperative target
        assert!(binding.entity_id.is_cooperative());
        // (3) reverse index consistent: entity -> good-coop
        assert_eq!(
            map.coop_for_entity(&entity).unwrap(),
            Some("good-coop".to_string())
        );
    }

    // T6e: the gossip/untrusted bind (`bind_coop_entity_map`) records NO trusted
    // provenance — it reads back as `UnknownLegacy`, never `Activation`. This guards
    // the trust boundary: an unauthenticated `coop:updates` payload must not be able
    // to write a row the store-backed resolver would treat as trusted. Only
    // `bind_coop_entity_map_activation` (the local authoritative path) records trust.
    #[test]
    fn test_gossip_bind_records_untrusted_unknown_legacy_provenance() {
        let map = InMemoryCoopEntityMap::new();
        let outcome = bind_coop_entity_map(&map, "good-coop");
        assert_eq!(
            outcome,
            EntityMapBindOutcome::Mapped(EntityId::cooperative("good-coop").unwrap())
        );
        let binding = map
            .binding_for_coop("good-coop")
            .unwrap()
            .expect("name binding present after gossip-mirror bind");
        assert_eq!(
            binding.provenance,
            CoopEntityBindingProvenance::UnknownLegacy,
            "gossip-sourced bind must NOT record trusted Activation provenance"
        );
    }
}
