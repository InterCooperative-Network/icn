//! Commons Manager for Gateway API
//!
//! Provides Commons Evolution operations for the gateway API:
//! - PersonhoodAnchor management (Layer 0)
//! - CommonsHolderRecord management (Layer 1)
//! - Charter management (Layer 2)
//! - Affiliation/membership management

use anyhow::{bail, Result};
use icn_governance::{Charter, CharterStatus, OrgType, StewardRecord};
use icn_identity::{
    Affiliation, Anchor, AnchorStatus, CommonsHolderRecord, CommonsRevocationReason, Did,
    EnrollmentPathway, HolderStatus, JurisdictionId, MembershipCapability, MembershipStatus,
    POPAttestation, POPLevel, POPMethod, PersonhoodAnchor, RevocationRecord, RevocationRegistry,
    RevocationScope, RevocationType,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::RwLock;

/// Commons Manager for gateway API
pub struct CommonsManager {
    // Layer 0: PersonhoodAnchors (by anchor_id hex)
    anchors: RwLock<HashMap<String, PersonhoodAnchor>>,
    // Index: DID -> anchor_id
    anchors_by_did: RwLock<HashMap<String, String>>,

    // Layer 1: CommonsHolderRecords (by holder_id hex)
    holders: RwLock<HashMap<String, CommonsHolderRecord>>,
    // Index: DID -> holder_id
    holders_by_did: RwLock<HashMap<String, String>>,
    // Index: anchor_id -> holder_id
    holders_by_anchor: RwLock<HashMap<String, String>>,

    // Layer 2: Charters (by charter_id hex)
    charters: RwLock<HashMap<String, Charter>>,
    // Index: domain_id -> charter_id
    charters_by_domain: RwLock<HashMap<String, String>>,

    // Steward records (by steward_id hex)
    stewards: RwLock<HashMap<String, StewardRecord>>,
    // Index: DID -> steward_id
    stewards_by_did: RwLock<HashMap<String, String>>,

    // Revocation registry
    revocations: RevocationRegistry,
}

impl CommonsManager {
    /// Create a new commons manager with in-memory storage
    pub fn new() -> Self {
        CommonsManager {
            anchors: RwLock::new(HashMap::new()),
            anchors_by_did: RwLock::new(HashMap::new()),
            holders: RwLock::new(HashMap::new()),
            holders_by_did: RwLock::new(HashMap::new()),
            holders_by_anchor: RwLock::new(HashMap::new()),
            charters: RwLock::new(HashMap::new()),
            charters_by_domain: RwLock::new(HashMap::new()),
            stewards: RwLock::new(HashMap::new()),
            stewards_by_did: RwLock::new(HashMap::new()),
            revocations: RevocationRegistry::new(),
        }
    }

    // ========== PersonhoodAnchor Operations (Layer 0) ==========

    /// Create a new PersonhoodAnchor from enrollment completion
    ///
    /// This creates a full PersonhoodAnchor with an underlying SDIS Anchor
    /// and an initial POP attestation.
    pub async fn create_anchor_from_enrollment(
        &self,
        did: &Did,
        steward_did: Option<&Did>,
    ) -> Result<PersonhoodAnchor> {
        // Generate pseudo-VUI from DID (in real SDIS, this comes from ceremony)
        let mut hasher = Sha256::new();
        hasher.update(b"gateway-enrollment-vui:");
        hasher.update(did.as_str().as_bytes());
        let vui_result = hasher.finalize();
        let mut vui = [0u8; 32];
        vui.copy_from_slice(&vui_result);

        // Generate genesis random
        let mut genesis = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut genesis);

        // Create the underlying SDIS Anchor
        let pathway = if steward_did.is_some() {
            EnrollmentPathway::WebOfTrust {
                vouchers: vec![steward_did.unwrap().clone()],
                vouched_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            }
        } else {
            EnrollmentPathway::Genesis {
                reason: "Gateway enrollment".to_string(),
            }
        };

        let sdis_anchor = Anchor::from_vui(&vui, pathway, &genesis);
        let anchor_id_bytes = sdis_anchor.id;

        // Create POP attestation
        let (pop_method, pop_level, issuer) = if let Some(steward) = steward_did {
            (
                POPMethod::Sponsored {
                    sponsor_did: steward.clone(),
                },
                POPLevel::Strong,
                steward.clone(),
            )
        } else {
            (
                POPMethod::Genesis {
                    reason: "Gateway enrollment".to_string(),
                },
                POPLevel::Weak,
                did.clone(),
            )
        };

        let attestation = POPAttestation::new(
            &anchor_id_bytes,
            issuer,
            pop_method,
            pop_level,
            vec![], // Signature placeholder
            None,   // No expiry
        );

        // Derive a current key from the DID (hash of DID as placeholder)
        let mut key_hasher = Sha256::new();
        key_hasher.update(b"gateway-current-key:");
        key_hasher.update(did.as_str().as_bytes());
        let key_result = key_hasher.finalize();
        let mut current_key = [0u8; 32];
        current_key.copy_from_slice(&key_result);

        // Create PersonhoodAnchor
        let anchor = PersonhoodAnchor::new(sdis_anchor, attestation, current_key);

        // Store anchor
        let anchor_id = hex::encode(anchor.id());
        let did_str = did.to_string();

        {
            let mut anchors = self
                .anchors
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

            if anchors.contains_key(&anchor_id) {
                bail!("Anchor already exists: {anchor_id}");
            }

            anchors.insert(anchor_id.clone(), anchor.clone());
        }

        {
            let mut by_did = self
                .anchors_by_did
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_did.insert(did_str, anchor_id);
        }

        Ok(anchor)
    }

    /// Get a PersonhoodAnchor by ID
    pub async fn get_anchor(&self, anchor_id: &str) -> Result<Option<PersonhoodAnchor>> {
        let anchors = self
            .anchors
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(anchors.get(anchor_id).cloned())
    }

    /// Get a PersonhoodAnchor by DID
    pub async fn get_anchor_by_did(&self, did: &Did) -> Result<Option<PersonhoodAnchor>> {
        let did_str = did.to_string();

        let anchor_id = {
            let by_did = self
                .anchors_by_did
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_did.get(&did_str).cloned()
        };

        if let Some(id) = anchor_id {
            self.get_anchor(&id).await
        } else {
            Ok(None)
        }
    }

    /// Update anchor status
    pub async fn update_anchor_status(&self, anchor_id: &str, status: AnchorStatus) -> Result<()> {
        let mut anchors = self
            .anchors
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(anchor) = anchors.get_mut(anchor_id) {
            anchor.status = status;
            anchor.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(())
        } else {
            bail!("Anchor not found: {anchor_id}")
        }
    }

    /// Add an attestation to an anchor
    pub async fn add_attestation(
        &self,
        anchor_id: &str,
        attestation: POPAttestation,
    ) -> Result<()> {
        let mut anchors = self
            .anchors
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(anchor) = anchors.get_mut(anchor_id) {
            anchor.add_attestation(attestation);
            Ok(())
        } else {
            bail!("Anchor not found: {anchor_id}")
        }
    }

    // ========== CommonsHolderRecord Operations (Layer 1) ==========

    /// Create a CommonsHolderRecord from an anchor
    pub async fn create_holder_from_anchor(
        &self,
        anchor_id: &str,
        did: &Did,
    ) -> Result<CommonsHolderRecord> {
        // Verify anchor exists and get its POP level
        let anchor = self
            .get_anchor(anchor_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Anchor not found: {anchor_id}"))?;

        // Convert anchor_id hex string to bytes
        let anchor_bytes: [u8; 32] = hex::decode(anchor_id)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid anchor_id length"))?;

        // Get the POP level from the anchor (default to Weak if no attestations)
        let pop_level = anchor.pop_level().unwrap_or(POPLevel::Weak);

        // Create holder with baseline Commons Rights
        let holder = CommonsHolderRecord::new(anchor_bytes, did.clone(), pop_level);

        let holder_id = hex::encode(holder.id());
        let did_str = did.to_string();

        // Store holder
        {
            let mut holders = self
                .holders
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

            if holders.contains_key(&holder_id) {
                bail!("Holder already exists: {holder_id}");
            }

            holders.insert(holder_id.clone(), holder.clone());
        }

        // Update indexes
        {
            let mut by_did = self
                .holders_by_did
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_did.insert(did_str, holder_id.clone());
        }

        {
            let mut by_anchor = self
                .holders_by_anchor
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_anchor.insert(anchor_id.to_string(), holder_id);
        }

        Ok(holder)
    }

    /// Get a CommonsHolderRecord by ID
    pub async fn get_holder(&self, holder_id: &str) -> Result<Option<CommonsHolderRecord>> {
        let holders = self
            .holders
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(holders.get(holder_id).cloned())
    }

    /// Get a CommonsHolderRecord by DID
    pub async fn get_holder_by_did(&self, did: &Did) -> Result<Option<CommonsHolderRecord>> {
        let did_str = did.to_string();

        let holder_id = {
            let by_did = self
                .holders_by_did
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_did.get(&did_str).cloned()
        };

        if let Some(id) = holder_id {
            self.get_holder(&id).await
        } else {
            Ok(None)
        }
    }

    /// Update holder status
    pub async fn update_holder_status(&self, holder_id: &str, status: HolderStatus) -> Result<()> {
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(holder) = holders.get_mut(holder_id) {
            holder.status = status;
            holder.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(())
        } else {
            bail!("Holder not found: {holder_id}")
        }
    }

    // ========== Affiliation Operations ==========

    /// Add an affiliation (join jurisdiction)
    pub async fn join_jurisdiction(
        &self,
        holder_id: &str,
        jurisdiction: JurisdictionId,
        initial_capabilities: Vec<MembershipCapability>,
    ) -> Result<Affiliation> {
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let holder = holders
            .get_mut(holder_id)
            .ok_or_else(|| anyhow::anyhow!("Holder not found: {holder_id}"))?;

        // Check if already affiliated
        if holder
            .affiliations
            .iter()
            .any(|a| a.jurisdiction_id == jurisdiction)
        {
            bail!(
                "Already affiliated with jurisdiction: {}",
                jurisdiction.as_str()
            );
        }

        // Create affiliation (starts as Candidate by default)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let affiliation = Affiliation {
            jurisdiction_id: jurisdiction.clone(),
            membership_status: MembershipStatus::Candidate,
            capabilities: initial_capabilities,
            roles: Vec::new(),
            joined_at: now,
            expires_at: None,
        };

        holder.affiliations.push(affiliation.clone());
        holder.updated_at = now;

        Ok(affiliation)
    }

    /// Leave a jurisdiction
    pub async fn leave_jurisdiction(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
    ) -> Result<()> {
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let holder = holders
            .get_mut(holder_id)
            .ok_or_else(|| anyhow::anyhow!("Holder not found: {holder_id}"))?;

        // Find and update affiliation status to Exited
        if let Some(affiliation) = holder
            .affiliations
            .iter_mut()
            .find(|a| &a.jurisdiction_id == jurisdiction)
        {
            affiliation.membership_status = MembershipStatus::Exited;
            holder.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(())
        } else {
            bail!(
                "Not affiliated with jurisdiction: {}",
                jurisdiction.as_str()
            )
        }
    }

    /// Update affiliation status (e.g., Candidate -> Member)
    pub async fn update_affiliation_status(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        status: MembershipStatus,
    ) -> Result<()> {
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let holder = holders
            .get_mut(holder_id)
            .ok_or_else(|| anyhow::anyhow!("Holder not found: {holder_id}"))?;

        if let Some(affiliation) = holder
            .affiliations
            .iter_mut()
            .find(|a| &a.jurisdiction_id == jurisdiction)
        {
            affiliation.membership_status = status;
            holder.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(())
        } else {
            bail!(
                "Not affiliated with jurisdiction: {}",
                jurisdiction.as_str()
            )
        }
    }

    /// List affiliations for a holder
    pub async fn list_affiliations(&self, holder_id: &str) -> Result<Vec<Affiliation>> {
        let holders = self
            .holders
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let holder = holders
            .get(holder_id)
            .ok_or_else(|| anyhow::anyhow!("Holder not found: {holder_id}"))?;

        Ok(holder.affiliations.clone())
    }

    // ========== Charter Operations (Layer 2) ==========

    /// Store a charter
    pub async fn store_charter(&self, charter: Charter) -> Result<()> {
        let charter_id = charter.charter_id.to_hex();
        let domain_id = charter.domain_id.clone();

        {
            let mut charters = self
                .charters
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

            if charters.contains_key(&charter_id) {
                bail!("Charter already exists: {charter_id}");
            }

            charters.insert(charter_id.clone(), charter);
        }

        {
            let mut by_domain = self
                .charters_by_domain
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_domain.insert(domain_id, charter_id);
        }

        Ok(())
    }

    /// Get a charter by ID
    pub async fn get_charter(&self, charter_id: &str) -> Result<Option<Charter>> {
        let charters = self
            .charters
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(charters.get(charter_id).cloned())
    }

    /// Get a charter by domain ID
    pub async fn get_charter_by_domain(&self, domain_id: &str) -> Result<Option<Charter>> {
        let charter_id = {
            let by_domain = self
                .charters_by_domain
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_domain.get(domain_id).cloned()
        };

        if let Some(id) = charter_id {
            self.get_charter(&id).await
        } else {
            Ok(None)
        }
    }

    /// List all charters with optional filters
    pub async fn list_charters(
        &self,
        org_type: Option<OrgType>,
        status: Option<CharterStatus>,
    ) -> Result<Vec<Charter>> {
        let charters = self
            .charters
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let mut results: Vec<Charter> = charters
            .values()
            .filter(|c| {
                let type_match = org_type.as_ref().is_none_or(|t| c.org_type == *t);
                let status_match = status.as_ref().is_none_or(|s| c.status == *s);
                type_match && status_match
            })
            .cloned()
            .collect();

        // Sort by creation time, newest first
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(results)
    }

    /// Update charter status
    pub async fn update_charter_status(
        &self,
        charter_id: &str,
        status: CharterStatus,
    ) -> Result<()> {
        let mut charters = self
            .charters
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(charter) = charters.get_mut(charter_id) {
            charter.status = status;
            // Note: Charter doesn't have an updated_at field
            // Status changes are tracked via amendments in production
            Ok(())
        } else {
            bail!("Charter not found: {charter_id}")
        }
    }

    // ========== Steward Operations ==========

    /// Register a new steward from a Commons Holder
    ///
    /// Requirements:
    /// - Must be an active Commons Holder
    /// - Must have at least Strong POP level
    /// - Must have governance approval (proposal ID)
    pub async fn register_steward(
        &self,
        holder_did: &Did,
        steward_did: &Did,
        term_duration_days: u64,
        bond_amount: u64,
        governance_approval: String,
        jurisdiction: Option<String>,
        specializations: Vec<String>,
    ) -> Result<StewardRecord> {
        // Verify holder exists and is active
        let holder = self
            .get_holder_by_did(holder_did)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found for DID: {holder_did}"))?;

        if !holder.is_active() {
            bail!("Holder is not active");
        }

        // Require at least Strong POP level to become a steward
        if holder.personhood_level == POPLevel::Weak {
            bail!("Steward requires at least Strong POP level");
        }

        // Check not already a steward
        if self.get_steward_by_did(steward_did).await?.is_some() {
            bail!("Already registered as steward");
        }

        // Calculate term
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let term_end = now + (term_duration_days * 24 * 60 * 60);

        // Create steward record
        let mut steward = StewardRecord::new(
            steward_did.clone(),
            holder_did.clone(),
            now,
            term_end,
            bond_amount,
            governance_approval,
        );

        // Set optional fields
        if let Some(j) = jurisdiction {
            steward.jurisdiction = Some(j);
        }
        for spec in specializations {
            steward.add_specialization(spec);
        }

        // Store
        self.store_steward(steward.clone()).await?;

        Ok(steward)
    }

    /// Store a steward record
    pub async fn store_steward(&self, steward: StewardRecord) -> Result<()> {
        let steward_id = steward.steward_id.to_hex();
        let did_str = steward.steward_did.to_string();

        {
            let mut stewards = self
                .stewards
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

            if stewards.contains_key(&steward_id) {
                bail!("Steward already exists: {steward_id}");
            }

            stewards.insert(steward_id.clone(), steward);
        }

        {
            let mut by_did = self
                .stewards_by_did
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_did.insert(did_str, steward_id);
        }

        Ok(())
    }

    /// Get a steward by ID
    pub async fn get_steward(&self, steward_id: &str) -> Result<Option<StewardRecord>> {
        let stewards = self
            .stewards
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(stewards.get(steward_id).cloned())
    }

    /// Get a steward by DID
    pub async fn get_steward_by_did(&self, did: &Did) -> Result<Option<StewardRecord>> {
        let did_str = did.to_string();

        let steward_id = {
            let by_did = self
                .stewards_by_did
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            by_did.get(&did_str).cloned()
        };

        if let Some(id) = steward_id {
            self.get_steward(&id).await
        } else {
            Ok(None)
        }
    }

    /// List all stewards with optional filters
    pub async fn list_stewards(
        &self,
        active_only: bool,
        jurisdiction: Option<&str>,
    ) -> Result<Vec<StewardRecord>> {
        let stewards = self
            .stewards
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let mut results: Vec<StewardRecord> = stewards
            .values()
            .filter(|s| {
                let active_match = !active_only || s.is_active();
                let jurisdiction_match = jurisdiction.is_none()
                    || s.jurisdiction.as_deref() == jurisdiction;
                active_match && jurisdiction_match
            })
            .cloned()
            .collect();

        // Sort by reputation score, highest first
        results.sort_by(|a, b| {
            b.reputation_score
                .partial_cmp(&a.reputation_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// List stewards who can issue attestations
    pub async fn list_attesters(&self) -> Result<Vec<StewardRecord>> {
        let stewards = self
            .stewards
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let attesters: Vec<StewardRecord> = stewards
            .values()
            .filter(|s| s.can_attest())
            .cloned()
            .collect();

        Ok(attesters)
    }

    /// Check if a DID belongs to an active steward
    pub async fn is_active_steward(&self, did: &Did) -> Result<bool> {
        if let Some(steward) = self.get_steward_by_did(did).await? {
            Ok(steward.is_active() && steward.can_attest())
        } else {
            Ok(false)
        }
    }

    /// Suspend a steward
    pub async fn suspend_steward(&self, steward_id: &str, reason: String) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.suspend(reason);
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Reinstate a suspended steward
    pub async fn reinstate_steward(&self, steward_id: &str) -> Result<bool> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            Ok(steward.reinstate())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Retire a steward
    pub async fn retire_steward(&self, steward_id: &str) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.retire();
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Revoke a steward for cause
    pub async fn revoke_steward(
        &self,
        steward_id: &str,
        reason: String,
        evidence: Vec<[u8; 32]>,
    ) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.revoke(reason, evidence);
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Record an attestation issued by a steward
    pub async fn record_steward_attestation(&self, steward_id: &str) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.record_attestation();
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Record a dispute against a steward's attestation
    pub async fn record_steward_dispute(&self, steward_id: &str) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.record_dispute();
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Record a dispute won by a steward
    pub async fn record_steward_dispute_won(&self, steward_id: &str) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.record_dispute_won();
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Extend a steward's term
    pub async fn extend_steward_term(&self, steward_id: &str, new_term_end: u64) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.extend_term(new_term_end);
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Add to a steward's bond
    pub async fn add_steward_bond(&self, steward_id: &str, amount: u64) -> Result<()> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            steward.add_bond(amount);
            Ok(())
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    /// Slash a steward's bond
    pub async fn slash_steward_bond(&self, steward_id: &str, amount: u64) -> Result<u64> {
        let mut stewards = self
            .stewards
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(steward) = stewards.get_mut(steward_id) {
            Ok(steward.slash_bond(amount))
        } else {
            bail!("Steward not found: {steward_id}")
        }
    }

    // ========== Revocation Operations ==========

    /// Add a revocation record
    pub async fn add_revocation(&self, record: RevocationRecord) -> Result<()> {
        self.revocations
            .add(record)
            .map_err(|e| anyhow::anyhow!("Failed to add revocation: {e}"))
    }

    /// Check if a target is revoked
    pub async fn check_revocation(&self, target_id: &str) -> icn_identity::RevocationCheck {
        self.revocations.check(target_id)
    }

    /// Check if a target is revoked at a specific scope
    pub async fn check_revocation_at_scope(
        &self,
        target_id: &str,
        scope: &RevocationScope,
    ) -> icn_identity::RevocationCheck {
        self.revocations.check_at_scope(target_id, scope)
    }

    /// Get a revocation record by ID
    pub async fn get_revocation(&self, revocation_id: &str) -> Option<RevocationRecord> {
        self.revocations.get(revocation_id)
    }

    /// List all revocations for a target
    pub async fn list_revocations_for_target(&self, target_id: &str) -> Vec<RevocationRecord> {
        self.revocations.list_for_target(target_id)
    }

    /// List all revocations by type
    pub async fn list_revocations_by_type(
        &self,
        target_type: RevocationType,
    ) -> Vec<RevocationRecord> {
        self.revocations.list_by_type(target_type)
    }

    /// List all pending appeals
    pub async fn list_pending_appeals(&self) -> Vec<RevocationRecord> {
        self.revocations.list_pending_appeals()
    }

    /// File an appeal for a revocation
    pub async fn file_appeal(&self, revocation_id: &str, reason: String) -> Result<()> {
        let mut record = self
            .revocations
            .get(revocation_id)
            .ok_or_else(|| anyhow::anyhow!("Revocation not found"))?;

        if !record.file_appeal(reason) {
            bail!("Cannot file appeal - appeal window closed or already appealed");
        }

        self.revocations
            .update(record)
            .map_err(|e| anyhow::anyhow!("Failed to update revocation: {e}"))
    }

    /// Resolve an appeal
    pub async fn resolve_appeal(
        &self,
        revocation_id: &str,
        upheld: bool,
        resolution_notes: String,
    ) -> Result<()> {
        let mut record = self
            .revocations
            .get(revocation_id)
            .ok_or_else(|| anyhow::anyhow!("Revocation not found"))?;

        record.resolve_appeal(upheld, resolution_notes);

        self.revocations
            .update(record)
            .map_err(|e| anyhow::anyhow!("Failed to update revocation: {e}"))
    }

    /// Revoke a membership with proper due process
    pub async fn revoke_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        authority: Did,
        reason: CommonsRevocationReason,
        appeal_window_days: u64,
    ) -> Result<RevocationRecord> {
        // Verify holder exists
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        // Verify has affiliation with jurisdiction
        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        // Create revocation record
        let target_id = format!("{}:{}", holder_id, jurisdiction_id);
        let record = RevocationRecord::new(
            RevocationType::Membership,
            target_id,
            RevocationScope::Jurisdiction {
                jurisdiction_id: jurisdiction_id.clone(),
            },
            authority,
            reason,
            appeal_window_days,
        );

        // Add to registry
        self.add_revocation(record.clone()).await?;

        // Update affiliation status (but allow appeal)
        affiliation.suspend();

        // Update holder
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(record)
    }

    /// Ban a member immediately (severe violations)
    pub async fn ban_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        authority: Did,
        reason: CommonsRevocationReason,
    ) -> Result<RevocationRecord> {
        // Verify holder exists
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        // Verify has affiliation
        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        // Create immediate revocation
        let target_id = format!("{}:{}", holder_id, jurisdiction_id);
        let record = RevocationRecord::immediate(
            RevocationType::Membership,
            target_id,
            RevocationScope::Jurisdiction {
                jurisdiction_id: jurisdiction_id.clone(),
            },
            authority,
            reason,
        );

        // Add to registry
        self.add_revocation(record.clone()).await?;

        // Ban the member
        affiliation.ban();

        // Update holder
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(record)
    }

    /// Check if a member is revoked in a jurisdiction
    pub async fn is_member_revoked(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> bool {
        let target_id = format!("{}:{}", holder_id, jurisdiction_id);
        self.revocations
            .is_revoked_in_jurisdiction(&target_id, jurisdiction_id)
    }

    // ========== Membership Management Operations ==========

    /// Apply for membership in a jurisdiction
    pub async fn apply_for_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: JurisdictionId,
        capabilities_requested: Vec<MembershipCapability>,
    ) -> Result<Affiliation> {
        // Verify holder exists and is active
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        if !holder.is_active() {
            bail!("Holder is not active");
        }

        // Check not already affiliated
        if holder.is_affiliated(&jurisdiction_id) {
            bail!("Already affiliated with jurisdiction: {jurisdiction_id}");
        }

        // Check not revoked from this jurisdiction
        if self.is_member_revoked(holder_id, &jurisdiction_id).await {
            bail!("Previously banned from jurisdiction: {jurisdiction_id}");
        }

        // Create affiliation as candidate
        let mut affiliation = Affiliation::new(jurisdiction_id.clone());
        // Store requested capabilities for review
        for cap in capabilities_requested {
            affiliation.add_capability(cap);
        }

        // Add to holder
        holder.add_affiliation(affiliation.clone());

        // Update storage
        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(affiliation)
    }

    /// Approve a membership application (moves from Candidate to Provisional)
    pub async fn approve_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        if affiliation.membership_status != MembershipStatus::Candidate {
            bail!("Can only approve candidates, current status: {}", affiliation.membership_status);
        }

        affiliation.approve();

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Promote a member from Provisional to full Member
    pub async fn promote_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        if affiliation.membership_status != MembershipStatus::Provisional {
            bail!(
                "Can only promote provisional members, current status: {}",
                affiliation.membership_status
            );
        }

        affiliation.promote();

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Suspend a member
    pub async fn suspend_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        affiliation.suspend();

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Reinstate a suspended member
    pub async fn reinstate_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        if affiliation.membership_status != MembershipStatus::Suspended {
            bail!("Member is not suspended");
        }

        // Reinstate to Member status
        affiliation.membership_status = MembershipStatus::Member;

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Grant a capability to a member
    pub async fn grant_capability(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        if !affiliation.is_active() {
            bail!("Member is not active");
        }

        affiliation.add_capability(capability);

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Revoke a capability from a member
    pub async fn revoke_capability(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        affiliation.remove_capability(capability);

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Add a role to a member
    pub async fn add_member_role(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        role: String,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        if !affiliation.roles.contains(&role) {
            affiliation.roles.push(role);
        }

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Remove a role from a member
    pub async fn remove_member_role(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        role: &str,
    ) -> Result<()> {
        let mut holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation_mut(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        affiliation.roles.retain(|r| r != role);

        let mut holders = self
            .holders
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        holders.insert(holder_id.to_string(), holder);

        Ok(())
    }

    /// Check if a member has a specific capability
    pub async fn member_has_capability(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<bool> {
        let holder = self
            .get_holder(holder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Holder not found"))?;

        let affiliation = holder.get_affiliation(jurisdiction_id).ok_or_else(|| {
            anyhow::anyhow!("Holder not affiliated with jurisdiction: {jurisdiction_id}")
        })?;

        Ok(affiliation.is_active() && affiliation.has_capability(capability))
    }

    /// List members of a jurisdiction by status
    pub async fn list_members_by_status(
        &self,
        jurisdiction_id: &JurisdictionId,
        status: Option<MembershipStatus>,
    ) -> Vec<(String, Affiliation)> {
        let holders = match self.holders.read() {
            Ok(h) => h,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        for (holder_id, holder) in holders.iter() {
            if let Some(affiliation) = holder.get_affiliation(jurisdiction_id) {
                if status.is_none() || Some(affiliation.membership_status) == status {
                    results.push((holder_id.clone(), affiliation.clone()));
                }
            }
        }

        results
    }
}

impl Default for CommonsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        Did::from_anchor_id(&[42u8; 32])
    }

    fn steward_did() -> Did {
        Did::from_anchor_id(&[99u8; 32])
    }

    #[tokio::test]
    async fn test_create_anchor_with_steward() {
        let mgr = CommonsManager::new();
        let did = test_did();
        let steward = steward_did();

        let anchor = mgr
            .create_anchor_from_enrollment(&did, Some(&steward))
            .await
            .unwrap();

        assert!(anchor.is_active());
        assert_eq!(anchor.pop_attestations.len(), 1);
        assert_eq!(anchor.pop_level(), Some(POPLevel::Strong));
    }

    #[tokio::test]
    async fn test_create_holder_from_anchor() {
        let mgr = CommonsManager::new();
        let did = test_did();

        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        let anchor_id = hex::encode(anchor.id());

        let holder = mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();

        assert!(holder.is_active());
        assert!(holder.affiliations.is_empty());
    }

    #[tokio::test]
    async fn test_join_and_leave_jurisdiction() {
        let mgr = CommonsManager::new();
        let did = test_did();

        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        let anchor_id = hex::encode(anchor.id());

        let holder = mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();
        let holder_id = hex::encode(holder.id());

        let jurisdiction = JurisdictionId::new("coop:test-coop");

        // Join
        let affiliation = mgr
            .join_jurisdiction(&holder_id, jurisdiction.clone(), vec![])
            .await
            .unwrap();
        assert_eq!(affiliation.membership_status, MembershipStatus::Candidate);

        // Verify affiliation exists
        let affiliations = mgr.list_affiliations(&holder_id).await.unwrap();
        assert_eq!(affiliations.len(), 1);

        // Leave
        mgr.leave_jurisdiction(&holder_id, &jurisdiction)
            .await
            .unwrap();

        // Verify status is Exited
        let affiliations = mgr.list_affiliations(&holder_id).await.unwrap();
        assert_eq!(affiliations[0].membership_status, MembershipStatus::Exited);
    }

    #[tokio::test]
    async fn test_get_by_did() {
        let mgr = CommonsManager::new();
        let did = test_did();

        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        let anchor_id = hex::encode(anchor.id());

        mgr.create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();

        // Lookup by DID
        let found_anchor = mgr.get_anchor_by_did(&did).await.unwrap();
        assert!(found_anchor.is_some());

        let found_holder = mgr.get_holder_by_did(&did).await.unwrap();
        assert!(found_holder.is_some());
    }
}
