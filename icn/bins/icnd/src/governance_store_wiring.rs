//! The daemon's single open of `<data_dir>/gateway_store` (M-C1 S1).
//!
//! Built here, at the composition root: one `SharedGatewayStore` (database and
//! receipt store, whose constructor runs the legacy index migration), seen as
//! the governance evidence store by the supervisor and as the concrete shared
//! store by the gateway.

use std::path::Path;
use std::sync::Arc;

use icn_core::supervisor::init_governance_evidence::{
    GovernanceEvidenceStore, GovernanceStoreInjection,
};

/// Open the store once and build both views over the same receipt store.
///
/// An open failure (for example, the store is locked by another holder) is
/// returned to the caller, which passes it to the supervisor rather than
/// stopping the daemon: governance is then held as `Failed`.
pub(crate) fn open_governance_store(data_dir: &Path) -> Result<GovernanceStoreInjection, String> {
    let shared = icn_gateway::shared_store::SharedGatewayStore::open(data_dir)?;
    let receipts = shared.receipt_store().clone();
    let backfill_receipts = receipts.clone();
    Ok(GovernanceStoreInjection {
        evidence: GovernanceEvidenceStore {
            receipt_backend: receipts as Arc<dyn icn_governance_actor::GovernanceReceiptBackend>,
            backfill_by_grantee_index: Arc::new(move || {
                backfill_receipts.backfill_grant_by_grantee_index()
            }),
        },
        gateway: shared,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Both views are the same receipt store over `<data_dir>/gateway_store`.
    #[test]
    fn ok_injection_shares_one_receipt_store_between_governance_and_gateway() {
        let dir = tempfile::tempdir().unwrap();
        let injection = open_governance_store(dir.path()).expect("open");
        let governance = Arc::as_ptr(&injection.evidence.receipt_backend) as *const u8;
        let gateway = Arc::as_ptr(injection.gateway.receipt_store()) as *const u8;
        assert_eq!(
            governance, gateway,
            "the supervisor and the gateway must see the same receipt store"
        );
        assert_eq!((injection.evidence.backfill_by_grantee_index)(), Ok(0));
        assert!(dir.path().join("gateway_store").is_dir());
    }

    /// A store held by another holder is an error value, not a daemon failure.
    #[test]
    fn locked_store_is_returned_as_an_open_error() {
        let dir = tempfile::tempdir().unwrap();
        let _held = open_governance_store(dir.path()).expect("first open");
        let err = match open_governance_store(dir.path()) {
            Ok(_) => panic!("a second open of a held store must fail"),
            Err(e) => e,
        };
        assert!(err.contains("gateway_store"), "{err}");
    }

    // -----------------------------------------------------------------------
    // The real daemon opener feeding the real supervisor split and evidence
    // init. Lives here because `icnd` is a bin crate: icn-core cannot depend
    // on it, so this is the only place both are reachable.
    // -----------------------------------------------------------------------

    use icn_core::supervisor::init_governance_evidence::{
        establish_governance_evidence, split_governance_store, GovernanceEvidenceInit,
        GovernanceEvidenceInputs,
    };
    use icn_gossip::{AccessControl, GossipActor, Topic};
    use icn_governance::{
        sdis::SdisProposal, GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig,
        ProposalPayload, ProposalScope, ProposalState, StaticMembershipResolver, VoteChoice,
    };
    use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
    use icn_governance_actor::{
        EvidenceReadiness, GovernanceActor, GovernanceCommand, GovernanceHandle,
    };
    use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

    const INTENT_PREFIX: &[u8] = b"gov:close-intent:";

    /// Sled store with one injectable fault: once armed, the first close-intent
    /// write makes every later close-intent read fail. A close then journals
    /// its intent and fails in its execution closure (decision commitment
    /// unreadable) BEFORE any mandate or grant is written — as a crash right
    /// after the intent write would leave it.
    struct PrefixFailStore {
        inner: SledStore,
        arm_after_intent_write: std::sync::atomic::AtomicBool,
        intent_reads_fail: std::sync::atomic::AtomicBool,
    }

    impl PrefixFailStore {
        fn after_write(&self, key: &[u8]) {
            use std::sync::atomic::Ordering::SeqCst;
            if key.starts_with(INTENT_PREFIX) && self.arm_after_intent_write.load(SeqCst) {
                self.intent_reads_fail.store(true, SeqCst);
            }
        }
    }

    impl Store for PrefixFailStore {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            if key.starts_with(INTENT_PREFIX)
                && self
                    .intent_reads_fail
                    .load(std::sync::atomic::Ordering::SeqCst)
            {
                anyhow::bail!("injected close-intent read failure");
            }
            self.inner.get(key)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            self.inner.put(key, value)?;
            self.after_write(key);
            Ok(())
        }
        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            self.inner.delete(key)
        }
        fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
            self.inner.delete_atomic(keys)
        }
        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(prefix)
        }
        fn compare_and_put(
            &self,
            key: &[u8],
            expected: Option<&[u8]>,
            new: &[u8],
        ) -> anyhow::Result<icn_store::CompareAndPut> {
            let r = self.inner.compare_and_put(key, expected, new)?;
            self.after_write(key);
            Ok(r)
        }
        fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
            self.inner.get_replica_metadata(h)
        }
        fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(m)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
            self.inner.list_replica_hashes()
        }
        fn flush(&self) -> anyhow::Result<()> {
            self.inner.flush().map(|_| ())
        }
    }

    fn did() -> icn_identity::Did {
        icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone()
    }

    async fn spawn_actor(state: &Path) -> (GovernanceHandle, Arc<PrefixFailStore>) {
        let store = Arc::new(PrefixFailStore {
            inner: SledStore::open(state).unwrap(),
            arm_after_intent_write: std::sync::atomic::AtomicBool::new(false),
            intent_reads_fail: std::sync::atomic::AtomicBool::new(false),
        });
        let node = did();
        let gossip = GossipActor::spawn(node.clone(), None);
        gossip.write().await.create_topic(Topic::new(
            "governance:proposal".into(),
            AccessControl::Public,
        ));
        let handle = GovernanceActor::spawn(
            node,
            store.clone() as Arc<dyn Store>,
            gossip,
            Arc::new(StaticMembershipResolver::new()),
            None,
            None,
        )
        .await
        .unwrap();
        (handle, store)
    }

    /// `open_governance_store` -> `split_governance_store` ->
    /// `establish_governance_evidence`: a journaled appointment whose first
    /// attempt wrote no mandate is replayed, and the mandate recovery writes
    /// through the evidence backend is readable through the gateway view — so
    /// both views are the same store.
    #[tokio::test]
    async fn real_opener_feeds_split_and_evidence_init_over_one_store() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state");
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let (member, candidate) = (did(), did());

        // First boot: leave an accepted appointment journaled, not committed,
        // with no mandate written.
        let id = {
            let injection = open_governance_store(&data).unwrap();
            let (handle, store) = spawn_actor(&state).await;
            handle
                .install_receipt_store(injection.evidence.receipt_backend.clone())
                .await;
            handle.recover_incomplete_closes().await;
            let ops: &dyn GovernanceOps = &handle;
            ops.create_domain(
                GovernanceDomainId("wiring-dom".into()),
                "Wiring".into(),
                "cooperative_default".into(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![member.clone()]),
            )
            .await
            .unwrap();
            let id = ops
                .create_proposal(
                    GovernanceDomainId("wiring-dom".into()),
                    "appoint".into(),
                    "d".into(),
                    ProposalPayload::Sdis {
                        proposal: SdisProposal::AppointSteward {
                            candidate: candidate.clone(),
                            sponsors: vec![member.clone()],
                            region: "r".into(),
                            bond_amount: 1_000,
                            term_length: 3600 * 24 * 30,
                        },
                    },
                    ProposalScope::Local,
                )
                .await
                .unwrap();
            ops.open_proposal(id.clone(), 3600).await.unwrap();
            ops.cast_vote(id.clone(), member.clone(), VoteChoice::For, None)
                .await
                .unwrap();
            store
                .arm_after_intent_write
                .store(true, std::sync::atomic::Ordering::SeqCst);
            handle
                .submit(GovernanceCommand::CloseProposal {
                    proposal_id: id.clone(),
                    eligible_voters: None,
                    excluded_delegators: None,
                    capability_scope: None,
                })
                .await
                .expect_err("the execution closure fails before any mandate is written");
            store
                .intent_reads_fail
                .store(false, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(
                SledGovernanceStateStore::new(store.clone())
                    .list_close_intents()
                    .unwrap()
                    .len(),
                1
            );
            // Non-vacuity: nothing authority-bearing reached the store on the
            // first boot, read through the same API the assertion below uses.
            assert!(
                injection
                    .gateway
                    .receipt_store()
                    .get_mandate_by_proposal(&id.0)
                    .unwrap()
                    .is_none(),
                "boot 1 must leave no mandate; only recovery may write it"
            );
            injection.gateway.db().flush().unwrap();
            store.flush().unwrap();
            handle.shutdown().await;
            id
        };

        // Second boot through the daemon's opener and the supervisor's split.
        let injected = open_governance_store(&data);
        let (evidence, gateway) = split_governance_store(Some(injected));
        let gateway = gateway.expect("the gateway view is delivered");
        let (handle, store) = spawn_actor(&state).await;
        let (init, settled) = establish_governance_evidence(GovernanceEvidenceInputs {
            governance: handle.clone(),
            store: evidence,
            dispatch_evidence_sink: None,
            execution_store: None,
            post_backfill_cleanup: None,
        })
        .await;
        match &init {
            GovernanceEvidenceInit::Established(e) => {
                assert_eq!(e.close_recovery.completed, 1, "{e:?}")
            }
            GovernanceEvidenceInit::Failed(cause) => panic!("not established: {cause:?}"),
        }
        assert_eq!(settled.readiness(), EvidenceReadiness::Ready);
        let st = SledGovernanceStateStore::new(store.clone());
        assert!(matches!(
            st.get_proposal(&id).unwrap().unwrap().state,
            ProposalState::Accepted { .. }
        ));
        assert!(st.list_close_intents().unwrap().is_empty());
        assert!(
            gateway
                .receipt_store()
                .get_mandate_by_proposal(&id.0)
                .unwrap()
                .is_some(),
            "the replayed close's mandate is in the gateway view's store"
        );
        handle.shutdown().await;
    }
}
