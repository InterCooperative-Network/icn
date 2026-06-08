//! Issue #1987: post-crash backfill of durable dispatch evidence.
//!
//! The decision executor (icn-core) persists each accepted decision's effects
//! AND their per-effect results on a durable `ExecutionRecord`, keyed
//! `exec:<decision_hash>`. The downstream `EffectDispatchEvidence` is written
//! best-effort by a spawned task, so a crash (or a receipt-store write failure)
//! in that async window can leave a terminal execution record whose evidence
//! was never persisted — the `InstitutionalEffectRecord` is stranded as
//! `emitted_only` with no replay handle.
//!
//! This backfill, run once at gateway startup *after* the dispatch-evidence
//! sink backend is installed and *after* the governance close-journal replay,
//! re-drives the sink for every terminal execution record that carries stored
//! results. The sink's `backfill_effects` reads back the evidence already
//! durable for each effect record and writes only what is missing, so this is
//! idempotent: it heals a crash residue exactly once and a clean restart
//! re-scans and writes nothing. It never re-executes effects (it replays the
//! stored results), so it cannot cause a duplicate downstream side effect.
//!
//! The scan is bounded: it pages the `exec:` keyspace in fixed-size batches
//! rather than loading every record into memory at once, so startup memory and
//! readiness stay flat on nodes with a large execution history.

use std::sync::Arc;

use icn_governance_actor::DeferredDispatchEvidenceSink;
use icn_kernel_api::execution::{ExecutionRecord, EXECUTION_RECORD_KEY_PREFIX};
use icn_store::Store;

/// Execution-store key prefix the decision executor writes under
/// (`exec:<decision_hash>`). Sourced from the canonical
/// [`EXECUTION_RECORD_KEY_PREFIX`] in `icn-kernel-api` (the same const
/// `icn-core`'s `SledExecutionStore` keys with), so the gateway reads exactly
/// the records the executor persists without the two sides drifting.
const EXECUTION_PREFIX: &[u8] = EXECUTION_RECORD_KEY_PREFIX.as_bytes();

/// Page size for the startup backfill scan.
///
/// Bounds peak memory to one page of execution records at a time instead of
/// materializing the entire `exec:` keyspace into a `Vec` — on a long-lived node
/// that keyspace can be large enough to spike memory and delay gateway
/// readiness. A once-at-startup heal is not latency-sensitive, so a modest page
/// keeps the resident working set small without meaningfully slowing it.
const BACKFILL_PAGE_SIZE: usize = 256;

/// Summary of a dispatch-evidence backfill scan (for startup logging).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct EvidenceBackfillReport {
    /// Execution records successfully read and deserialized.
    pub scanned: usize,
    /// Terminal records with stored results that were re-driven through the
    /// sink (the sink itself deduplicates, so this counts attempts, not writes).
    pub redriven: usize,
    /// Records skipped because they were non-terminal or had no stored results.
    pub skipped: usize,
    /// Records that could not be deserialized (pre-upgrade / corrupt rows).
    pub unparsable: usize,
}

/// Scan durable execution records and idempotently backfill any missing
/// `EffectDispatchEvidence` via the already-installed sink. See module docs.
///
/// Non-terminal records are intentionally left alone: the executor's own
/// in-flight recovery (`DecisionExecutor::recover_in_flight`) re-runs those
/// through the live, evidence-writing callback. Only terminal records — whose
/// effects already applied and will not run again — need their evidence healed
/// here.
///
/// The scan is bounded: it pages through the keyspace via
/// [`backfill_pending_dispatch_evidence_paged`] at [`BACKFILL_PAGE_SIZE`] rather
/// than materializing every record at once.
pub(crate) fn backfill_pending_dispatch_evidence(
    execution_store: &Arc<dyn Store>,
    sink: &DeferredDispatchEvidenceSink,
) -> anyhow::Result<EvidenceBackfillReport> {
    backfill_pending_dispatch_evidence_paged(execution_store, sink, BACKFILL_PAGE_SIZE)
}

/// Page-bounded core of the backfill (see [`backfill_pending_dispatch_evidence`]).
///
/// Pages the `exec:` keyspace with [`Store::scan_after`] cursor pagination — each
/// page resumes strictly after the previous page's last key — so at most
/// `page_size` records are resident at once *and* the whole keyspace is
/// traversed in `O(n)` (a numeric-offset scan would re-walk from the start every
/// page, `O(n²/page_size)`). The backfill runs at startup before the gateway
/// serves and before retention cleanup prunes, so no concurrent writer mutates
/// the keyspace mid-walk; and the sink dedups on read-back, so a re-observed
/// record cannot double-write.
fn backfill_pending_dispatch_evidence_paged(
    execution_store: &Arc<dyn Store>,
    sink: &DeferredDispatchEvidenceSink,
    page_size: usize,
) -> anyhow::Result<EvidenceBackfillReport> {
    // A zero page would never make progress; clamp defensively.
    let page_size = page_size.max(1);
    let mut report = EvidenceBackfillReport::default();
    let mut cursor: Option<Vec<u8>> = None;

    loop {
        let page = execution_store.scan_after(EXECUTION_PREFIX, cursor.as_deref(), page_size)?;
        if page.is_empty() {
            break;
        }
        let page_len = page.len();
        // Resume the next page strictly after this page's last key.
        cursor = page.last().map(|(k, _)| k.clone());

        for (_key, value) in page {
            let record: ExecutionRecord = match serde_json::from_slice(&value) {
                Ok(record) => record,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "dispatch-evidence backfill: skipping unparsable execution record"
                    );
                    report.unparsable += 1;
                    continue;
                }
            };
            report.scanned += 1;

            // Only terminal records carry final results, and only records that
            // produced results can yield evidence. A non-terminal record (or a
            // pre-upgrade record with no stored results) has nothing to backfill.
            if !record.is_terminal() || record.results.is_empty() {
                report.skipped += 1;
                continue;
            }

            // Reproduce the dispatch's own completion time on replay (mirrors the
            // close journal's stored `decided_at`) rather than using restart time,
            // so the healed evidence is timestamped when the dispatch happened.
            let recorded_at = record.finished_at.unwrap_or(record.started_at);
            sink.backfill_effects(
                &record.decision_receipt_id,
                &record.effects,
                &record.results,
                recorded_at,
            );
            report.redriven += 1;
        }

        // A short page means the keyspace is exhausted; an exact-multiple final
        // page is caught by the empty-page check on the next iteration.
        if page_len < page_size {
            break;
        }
    }

    tracing::debug!(
        scanned = report.scanned,
        redriven = report.redriven,
        skipped = report.skipped,
        unparsable = report.unparsable,
        "dispatch-evidence backfill scan complete"
    );
    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::receipt_store::ReceiptStore;
    use icn_governance_actor::institutional_effect::InstitutionalEffectRecord;
    use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
    use icn_governance_actor::DeferredDispatchEvidenceSink;
    use icn_kernel_api::effects::{EffectOutcome, EffectResult, KernelEffect, SdisEffect};
    use icn_store::SledStore;

    fn approve_effect(proposal_id: &str) -> KernelEffect {
        KernelEffect::Sdis(SdisEffect::ApproveSteward {
            steward_did: "did:icn:zSteward".into(),
            jurisdiction_id: "dom".into(),
            term_length_seconds: 3600,
            bond_amount: 100,
            region: Some("r".into()),
            proposal_id: proposal_id.into(),
            capabilities_hash: String::new(),
        })
    }

    fn applied_result(receipt_ref: &str) -> EffectResult {
        EffectResult {
            effect_id: "eff".into(),
            success: true,
            message: "approved".into(),
            state_change_hash: Some("sch".into()),
            ledger_entry_id: None,
            not_executed: false,
            receipt_ref: Some(receipt_ref.into()),
            outcome: Some(EffectOutcome::Applied),
        }
    }

    fn seed_terminal_record(
        store: &Arc<dyn Store>,
        decision_hash: &str,
        decision_receipt_id: &str,
        proposal_id: &str,
    ) {
        let mut record = ExecutionRecord::new_pending(
            decision_hash,
            proposal_id,
            decision_receipt_id,
            vec![approve_effect(proposal_id)],
        );
        record.set_results(vec![applied_result("steward-hex")]);
        record.mark_confirmed(vec![], vec!["sch".into()]);
        store
            .put(
                format!("{EXECUTION_RECORD_KEY_PREFIX}{decision_hash}").as_bytes(),
                &serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
    }

    fn receipt_store_with_ier(proposal_id: &str) -> (Arc<ReceiptStore>, InstitutionalEffectRecord) {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let ier = InstitutionalEffectRecord::new(
            proposal_id,
            "dom",
            None,
            "appoint_steward",
            Some("did:icn:zSteward".into()),
            Some("r".into()),
            None,
            1,
            serde_json::json!({}),
        );
        receipt_store.put_institutional_effect(&ier).unwrap();
        (receipt_store, ier)
    }

    /// The #1987 acceptance test: a terminal execution record whose evidence
    /// was lost (IER present, no `EffectDispatchEvidence`) is healed by the
    /// backfill, and re-running the backfill writes no duplicate.
    #[test]
    fn backfill_writes_missing_evidence_then_is_idempotent() {
        let (receipt_store, ier) = receipt_store_with_ier("prop-e2e");
        assert!(
            receipt_store
                .list_effect_dispatch_evidence_by_record(&ier.record_id)
                .unwrap()
                .is_empty(),
            "precondition: evidence was lost in the crash window"
        );

        // Mirror gateway startup: install the backend into the deferred sink.
        let sink = DeferredDispatchEvidenceSink::new();
        sink.install_backend(receipt_store.clone() as Arc<dyn GovernanceReceiptBackend>);

        // The durable execution record survived the crash; its evidence did not.
        let exec: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        seed_terminal_record(&exec, "hash-e2e", "gov:dom:prop-e2e:receipt", "prop-e2e");

        let report = backfill_pending_dispatch_evidence(&exec, &sink).unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(
            report.redriven, 1,
            "one terminal record with results re-driven"
        );

        let evidence = receipt_store
            .list_effect_dispatch_evidence_by_record(&ier.record_id)
            .unwrap();
        assert_eq!(evidence.len(), 1, "backfill wrote the missing evidence");
        assert_eq!(evidence[0].proposal_id, "prop-e2e");
        assert_eq!(evidence[0].subsystem, "sdis");
        assert!(evidence[0].success);
        assert_eq!(evidence[0].receipt_ref.as_deref(), Some("steward-hex"));

        // Another restart re-runs the backfill: the scan still re-drives, but
        // the sink's read-back dedup must add no second row.
        let report2 = backfill_pending_dispatch_evidence(&exec, &sink).unwrap();
        assert_eq!(report2.redriven, 1);
        assert_eq!(
            receipt_store
                .list_effect_dispatch_evidence_by_record(&ier.record_id)
                .unwrap()
                .len(),
            1,
            "replaying the backfill must not duplicate evidence"
        );
    }

    /// Backfill heals only terminal-with-results records; a non-terminal
    /// (Pending) record is left to the executor's in-flight recovery.
    #[test]
    fn backfill_skips_nonterminal_records() {
        let (receipt_store, _ier) = receipt_store_with_ier("prop-pending");
        let sink = DeferredDispatchEvidenceSink::new();
        sink.install_backend(receipt_store as Arc<dyn GovernanceReceiptBackend>);

        let exec: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        let pending = ExecutionRecord::new_pending(
            "hash-pending",
            "prop-pending",
            "gov:dom:prop-pending:receipt",
            vec![approve_effect("prop-pending")],
        );
        exec.put(
            format!("{EXECUTION_RECORD_KEY_PREFIX}hash-pending").as_bytes(),
            &serde_json::to_vec(&pending).unwrap(),
        )
        .unwrap();

        let report = backfill_pending_dispatch_evidence(&exec, &sink).unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(
            report.redriven, 0,
            "non-terminal records are left to in-flight execution recovery"
        );
        assert_eq!(report.skipped, 1);
    }

    /// Concern #2 (bounded scan): the backfill pages through the execution-record
    /// keyspace instead of materializing it whole, and must still visit and heal
    /// every terminal record across page boundaries. A page size of 2 over 5
    /// records forces three pages (2 + 2 + 1); a loop that stops after the first
    /// page or spins forever fails this. Re-paging stays idempotent.
    #[test]
    fn backfill_pages_through_records_across_page_boundaries() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let sink = DeferredDispatchEvidenceSink::new();
        sink.install_backend(receipt_store.clone() as Arc<dyn GovernanceReceiptBackend>);

        let exec: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());

        let mut record_ids = Vec::new();
        for i in 0..5 {
            let proposal_id = format!("prop-page-{i}");
            let ier = InstitutionalEffectRecord::new(
                &proposal_id,
                "dom",
                None,
                "appoint_steward",
                Some("did:icn:zSteward".into()),
                Some("r".into()),
                None,
                1,
                serde_json::json!({}),
            );
            receipt_store.put_institutional_effect(&ier).unwrap();
            record_ids.push(ier.record_id.clone());
            seed_terminal_record(
                &exec,
                &format!("hash-page-{i}"),
                &format!("gov:dom:{proposal_id}:receipt"),
                &proposal_id,
            );
        }

        // Page size smaller than the record count: a correct loop visits all
        // three pages; a broken one stops early or never terminates.
        let report = backfill_pending_dispatch_evidence_paged(&exec, &sink, 2).unwrap();
        assert_eq!(report.scanned, 5, "every record visited across pages");
        assert_eq!(
            report.redriven, 5,
            "every terminal-with-results record healed"
        );

        for record_id in &record_ids {
            assert_eq!(
                receipt_store
                    .list_effect_dispatch_evidence_by_record(record_id)
                    .unwrap()
                    .len(),
                1,
                "each record's evidence healed exactly once"
            );
        }

        // Paging stays idempotent: a second pass writes no duplicates.
        let report2 = backfill_pending_dispatch_evidence_paged(&exec, &sink, 2).unwrap();
        assert_eq!(report2.redriven, 5);
        for record_id in &record_ids {
            assert_eq!(
                receipt_store
                    .list_effect_dispatch_evidence_by_record(record_id)
                    .unwrap()
                    .len(),
                1,
                "re-paging must not duplicate evidence"
            );
        }
    }

    /// Test D (concern #1 regression): the ordering contract that backfill must
    /// read a terminal execution record *before* retention cleanup prunes it.
    /// Evidence healed before a prune survives it; a record pruned before
    /// backfill is gone and its lost evidence can never be reconstructed — which
    /// is exactly why daemon startup defers cleanup until after the backfill.
    #[test]
    fn pruning_after_backfill_keeps_evidence_but_pruning_before_loses_it() {
        // Correct order: backfill THEN prune.
        let (receipt_store, ier) = receipt_store_with_ier("prop-order");
        let sink = DeferredDispatchEvidenceSink::new();
        sink.install_backend(receipt_store.clone() as Arc<dyn GovernanceReceiptBackend>);
        let exec: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        let key = format!("{EXECUTION_RECORD_KEY_PREFIX}hash-order");
        seed_terminal_record(
            &exec,
            "hash-order",
            "gov:dom:prop-order:receipt",
            "prop-order",
        );

        let report = backfill_pending_dispatch_evidence(&exec, &sink).unwrap();
        assert_eq!(report.redriven, 1, "backfill heals before any prune");
        assert_eq!(
            receipt_store
                .list_effect_dispatch_evidence_by_record(&ier.record_id)
                .unwrap()
                .len(),
            1
        );

        // Retention cleanup now prunes the (already-healed) terminal record.
        exec.delete(key.as_bytes()).unwrap();
        assert_eq!(
            receipt_store
                .list_effect_dispatch_evidence_by_record(&ier.record_id)
                .unwrap()
                .len(),
            1,
            "evidence healed before the prune survives it"
        );

        // Wrong order: prune THEN backfill — the regression this guards against.
        let (receipt_store2, ier2) = receipt_store_with_ier("prop-order2");
        let sink2 = DeferredDispatchEvidenceSink::new();
        sink2.install_backend(receipt_store2.clone() as Arc<dyn GovernanceReceiptBackend>);
        let exec2: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        let key2 = format!("{EXECUTION_RECORD_KEY_PREFIX}hash-order2");
        seed_terminal_record(
            &exec2,
            "hash-order2",
            "gov:dom:prop-order2:receipt",
            "prop-order2",
        );

        // Cleanup wins the race and prunes the record before backfill runs.
        exec2.delete(key2.as_bytes()).unwrap();
        let report2 = backfill_pending_dispatch_evidence(&exec2, &sink2).unwrap();
        assert_eq!(report2.scanned, 0, "pruned record is gone; nothing to scan");
        assert_eq!(report2.redriven, 0);
        assert!(
            receipt_store2
                .list_effect_dispatch_evidence_by_record(&ier2.record_id)
                .unwrap()
                .is_empty(),
            "evidence is lost forever when pruning beats backfill"
        );
    }
}
