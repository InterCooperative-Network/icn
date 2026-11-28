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
//! This backfill, run once at daemon startup by the supervisor's governance
//! evidence initialization, *after* the dispatch-evidence sink backend is
//! installed and *after* the governance close-journal replay,
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

use crate::DeferredDispatchEvidenceSink;
use icn_kernel_api::execution::{ExecutionRecord, EXECUTION_RECORD_KEY_PREFIX};
use icn_store::Store;

/// Execution-store key prefix the decision executor writes under
/// (`exec:<decision_hash>`). Sourced from the canonical
/// [`EXECUTION_RECORD_KEY_PREFIX`] in `icn-kernel-api` (the same const
/// `icn-core`'s `SledExecutionStore` keys with), so the backfill reads exactly
/// the records the executor persists without the two sides drifting.
const EXECUTION_PREFIX: &[u8] = EXECUTION_RECORD_KEY_PREFIX.as_bytes();

/// Page size for the startup backfill scan.
///
/// Bounds peak memory to one page of execution records at a time instead of
/// materializing the entire `exec:` keyspace into a `Vec` — on a long-lived node
/// that keyspace can be large enough to spike memory and delay governance
/// readiness. A once-at-startup heal is not latency-sensitive, so a modest page
/// keeps the resident working set small without meaningfully slowing it.
const BACKFILL_PAGE_SIZE: usize = 256;

/// Summary of a dispatch-evidence backfill scan (for startup logging).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EvidenceBackfillReport {
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
pub fn backfill_pending_dispatch_evidence(
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
/// page, `O(n²/page_size)`).
///
/// Concurrency: the backfill runs at startup before any API serves and before
/// retention cleanup prunes, so nothing deletes records mid-walk. It is NOT
/// free of concurrent writers: governance readiness is released by close
/// recovery, which runs before this backfill, so the scheduler and the decision
/// executor (including executions driven by recovered acceptances) may insert
/// or update `exec:` records during the walk. That is tolerated: cursor
/// pagination resumes strictly after the last key seen, so an insert is either
/// visited or left for the live path (which writes its own evidence) and the
/// next startup's backfill; a record updated after it was read is re-driven
/// at most once per walk; and the sink dedups on read-back, so a record that
/// is both backfilled and recorded live, or re-observed, cannot double-write.
#[doc(hidden)]
pub fn backfill_pending_dispatch_evidence_paged(
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
