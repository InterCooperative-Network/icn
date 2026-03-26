//! Decision Executor — idempotent, persistent wrapper around EffectDispatcher.
//!
//! The `DecisionExecutor` sits between the governance event subscription and the
//! `EffectDispatcher`. It provides:
//!
//! 1. **Idempotency**: If a decision_hash has already reached a terminal status
//!    ([`ExecutionStatus::Confirmed`], [`ExecutionStatus::NotExecuted`],
//!    [`ExecutionStatus::PermanentlyFailed`]), the executor returns early.
//!    This prevents double-execution from replayed events.
//!
//! 2. **Execution logging**: Every decision's execution status is persisted to sled.
//!    This survives restarts and enables audit.
//!
//! 3. **Saga-ready structure**: The `ExecutionRecord` status machine
//!    (Pending → Executing → Confirmed / NotExecuted / Failed) is the foundation for future
//!    saga patterns (retry, compensation, dead-letter routing).
//!
//! # Architecture
//!
//! ```text
//! [App Layer]
//! GovernanceEvent → translate_payload_to_effects() → Vec<KernelEffect>
//!                                                          |
//!                                                          v
//! [DecisionExecutor]  ←── idempotency check (sled)
//!       |   record Pending
//!       |   record Executing
//!       v
//! [EffectDispatcher]  ←── existing execution path
//!       |
//!       v
//! [DecisionExecutor]  ←── record Confirmed / NotExecuted / Failed
//! ```
//!
//! # Aggregation contract (per-effect vs aggregate)
//!
//! - Each [`EffectResult`] row: `success` means that effect applied its kernel work; `not_executed`
//!   means the row is structurally non-executable (NoOp, Deferred, etc.) and is **not** a hard
//!   failure. `is_hard_failure()` ≡ `!success && !not_executed`.
//! - Persisted [`ExecutionRecord`] has a **single** [`ExecutionStatus`] for the whole decision:
//!   - Any hard-failure row → `Failed` (`retries` incremented).
//!   - Else if every row is `not_executed` (and non-empty) → `NotExecuted` (terminal, no retry bump).
//!   - Else → `Confirmed` (includes **mixed** `success` + `not_executed` batches: intentional).
//! - **NoOp-only** batches must **not** reach `Confirmed` via aggregation (they are all
//!   `not_executed`).

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use icn_kernel_api::effects::{EffectResult, KernelEffect};
use icn_kernel_api::escrow::{EscrowStatus, EscrowStore};
use icn_kernel_api::execution::{
    ExecutionRecord, ExecutionStatus, ExecutionStore, ExecutionTruthSummary,
};

use super::effect_dispatcher::EffectDispatcher;

/// Maximum number of automatic retries before marking permanently failed.
const MAX_RETRIES: u32 = 3;
/// Maximum age of terminal records (seconds) to retain on startup.
pub const MAX_CONFIRMED_AGE_SECS: u64 = 30 * 24 * 60 * 60;
/// Maximum number of terminal records to retain after startup cleanup.
pub const MAX_CONFIRMED_COUNT: usize = 10_000;

/// Maximum number of concurrent decision executions (backpressure).
const MAX_CONCURRENT_EXECUTIONS: usize = 16;

/// Wraps `EffectDispatcher` with persistent idempotency and execution logging.
pub struct DecisionExecutor {
    dispatcher: Arc<EffectDispatcher>,
    store: Arc<dyn ExecutionStore>,
    escrow_store: Option<Arc<dyn EscrowStore>>,
    /// Bounded semaphore for backpressure on concurrent executions.
    concurrency_limit: Arc<Semaphore>,
}

impl DecisionExecutor {
    /// Create a new DecisionExecutor.
    pub fn new(dispatcher: Arc<EffectDispatcher>, store: Arc<dyn ExecutionStore>) -> Self {
        Self {
            dispatcher,
            store,
            escrow_store: None,
            concurrency_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_EXECUTIONS)),
        }
    }

    /// Attach escrow store for terminal remediation of stuck Releasing escrows.
    pub fn with_escrow_store(mut self, escrow_store: Arc<dyn EscrowStore>) -> Self {
        self.escrow_store = Some(escrow_store);
        self
    }

    /// Get a reference to the execution store.
    pub fn store(&self) -> &Arc<dyn ExecutionStore> {
        &self.store
    }

    /// Recover in-flight executions from a prior crash.
    ///
    /// Scans the ExecutionStore for records in non-terminal states
    /// (Pending, Executing, Failed with retries remaining) and re-executes
    /// them using the stored effects.
    ///
    /// Call this once at startup, after constructing the executor.
    pub async fn recover_in_flight(&self) -> Result<RecoveryReport> {
        // Log the store's status distribution before recovery begins
        match self.store.count_by_status() {
            Ok(counts) => {
                info!(
                    pending = counts.get(&ExecutionStatus::Pending).copied().unwrap_or(0),
                    executing = counts
                        .get(&ExecutionStatus::Executing)
                        .copied()
                        .unwrap_or(0),
                    failed = counts.get(&ExecutionStatus::Failed).copied().unwrap_or(0),
                    confirmed = counts
                        .get(&ExecutionStatus::Confirmed)
                        .copied()
                        .unwrap_or(0),
                    permanently_failed = counts
                        .get(&ExecutionStatus::PermanentlyFailed)
                        .copied()
                        .unwrap_or(0),
                    not_executed = counts
                        .get(&ExecutionStatus::NotExecuted)
                        .copied()
                        .unwrap_or(0),
                    "Recovery scan starting — execution store status distribution"
                );
            }
            Err(e) => {
                warn!(error = %e, "Could not read execution store counts before recovery");
            }
        }

        let mut report = RecoveryReport::default();

        // Collect records that need recovery
        let mut recoverable = Vec::new();

        for status in [
            ExecutionStatus::Executing,
            ExecutionStatus::Pending,
            ExecutionStatus::Failed,
        ] {
            let records = self.store.list_by_status(status)?;
            for record in records {
                if record.effects.is_empty() {
                    // Pre-existing record without stored effects — can't recover
                    warn!(
                        decision_hash = %record.decision_hash,
                        status = ?record.status,
                        "Cannot recover decision: no stored effects (pre-upgrade record)"
                    );
                    report.skipped_no_effects += 1;
                    continue;
                }

                if record.status == ExecutionStatus::Failed && record.retries >= MAX_RETRIES {
                    // Exhausted retries — mark permanently failed
                    let mut rec = record;
                    rec.mark_permanently_failed("Max retries exceeded (recovery)");
                    if let Err(e) = self.store.put(&rec) {
                        warn!(
                            decision_hash = %rec.decision_hash,
                            error = %e,
                            "Failed to mark exhausted decision as PermanentlyFailed"
                        );
                    } else {
                        self.mark_related_escrows_failed(&rec, "max retries exceeded (recovery)");
                    }
                    report.skipped_max_retries += 1;
                    continue;
                }

                recoverable.push(record);
            }
        }

        if recoverable.is_empty() {
            debug!("No in-flight decisions to recover");
            return Ok(report);
        }

        info!(
            count = recoverable.len(),
            "Recovering in-flight decisions from prior crash"
        );

        for record in recoverable {
            let decision_hash = record.decision_hash.clone();
            let receipt_id = record.decision_receipt_id.clone();
            let proposal_id = record.proposal_id.clone();
            let effects = record.effects.clone();

            match self
                .execute(effects, &receipt_id, &decision_hash, &proposal_id)
                .await
            {
                Ok(_results) => {
                    if let Some(rec) = self.store.get(&decision_hash)? {
                        match rec.status {
                            ExecutionStatus::Confirmed => {
                                info!(
                                    decision_hash = %decision_hash,
                                    "Recovered decision: confirmed"
                                );
                                report.recovered_confirmed += 1;
                            }
                            ExecutionStatus::NotExecuted => {
                                info!(
                                    decision_hash = %decision_hash,
                                    "Recovered decision: not executed (terminal)"
                                );
                                report.recovered_not_executed += 1;
                            }
                            ExecutionStatus::Failed => {
                                warn!(
                                    decision_hash = %decision_hash,
                                    "Recovered decision: some effects failed"
                                );
                                report.recovered_failed += 1;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    error!(
                        decision_hash = %decision_hash,
                        error = %e,
                        "Recovery failed for decision"
                    );
                    report.recovered_failed += 1;
                }
            }
        }

        info!(
            confirmed = report.recovered_confirmed,
            not_executed = report.recovered_not_executed,
            failed = report.recovered_failed,
            skipped_no_effects = report.skipped_no_effects,
            skipped_max_retries = report.skipped_max_retries,
            "Decision recovery complete"
        );

        Ok(report)
    }

    /// Best-effort startup cleanup of old terminal records.
    ///
    /// Deletes `Confirmed` and `PermanentlyFailed` records older than
    /// `MAX_CONFIRMED_AGE_SECS`. Then enforces `MAX_CONFIRMED_COUNT` cap by
    /// deleting oldest remaining terminal records.
    ///
    /// This method intentionally returns `Result` so callers can log and
    /// continue startup without aborting.
    pub fn cleanup_old_records(&self) -> Result<CleanupReport> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cutoff = now.saturating_sub(MAX_CONFIRMED_AGE_SECS);

        let mut terminal = Vec::new();
        terminal.extend(self.store.list_by_status(ExecutionStatus::Confirmed)?);
        terminal.extend(self.store.list_by_status(ExecutionStatus::NotExecuted)?);
        terminal.extend(
            self.store
                .list_by_status(ExecutionStatus::PermanentlyFailed)?,
        );

        terminal.sort_by_key(|r| r.finished_at.unwrap_or(r.started_at));

        let mut report = CleanupReport::default();

        for record in &terminal {
            let finished = record.finished_at.unwrap_or(record.started_at);
            if finished < cutoff {
                self.store.delete(&record.decision_hash)?;
                report.deleted_old += 1;
            }
        }

        // Re-query after age-based pruning and enforce count cap.
        let mut terminal_after = Vec::new();
        terminal_after.extend(self.store.list_by_status(ExecutionStatus::Confirmed)?);
        terminal_after.extend(self.store.list_by_status(ExecutionStatus::NotExecuted)?);
        terminal_after.extend(
            self.store
                .list_by_status(ExecutionStatus::PermanentlyFailed)?,
        );
        terminal_after.sort_by_key(|r| r.finished_at.unwrap_or(r.started_at));

        if terminal_after.len() > MAX_CONFIRMED_COUNT {
            let excess = terminal_after.len() - MAX_CONFIRMED_COUNT;
            for record in terminal_after.iter().take(excess) {
                self.store.delete(&record.decision_hash)?;
                report.deleted_excess += 1;
            }
        }

        Ok(report)
    }

    /// Best-effort remediation for escrows stuck in `Releasing` when a decision
    /// reaches terminal `PermanentlyFailed`.
    fn mark_related_escrows_failed(&self, record: &ExecutionRecord, reason: &str) {
        let Some(escrow_store) = &self.escrow_store else {
            return;
        };

        use icn_kernel_api::effects::TreasuryEffect;

        for effect in &record.effects {
            let KernelEffect::Treasury(TreasuryEffect::ReleaseEscrow {
                escrow_id,
                decision_hash,
                ..
            }) = effect
            else {
                continue;
            };

            if decision_hash != &record.decision_hash {
                continue;
            }

            match escrow_store.get(escrow_id) {
                Ok(Some(mut escrow)) => {
                    let is_stuck_for_decision = escrow.status == EscrowStatus::Releasing
                        && escrow.release_decision_hash.as_deref() == Some(decision_hash);
                    if !is_stuck_for_decision {
                        continue;
                    }

                    escrow.mark_failed(decision_hash, reason.to_string());
                    if let Err(e) = escrow_store.put(&escrow) {
                        warn!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            error = %e,
                            "Failed to persist terminal escrow failure state"
                        );
                    } else {
                        warn!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            "Escrow transitioned Releasing → Failed due to permanent decision failure"
                        );
                    }
                }
                Ok(None) => {
                    warn!(
                        escrow_id = %escrow_id,
                        decision_hash = %decision_hash,
                        "Escrow referenced by failed decision was not found"
                    );
                }
                Err(e) => {
                    warn!(
                        escrow_id = %escrow_id,
                        decision_hash = %decision_hash,
                        error = %e,
                        "Failed to read escrow for terminal remediation"
                    );
                }
            }
        }
    }

    /// Execute effects for a governance decision, with idempotency.
    ///
    /// # Arguments
    /// * `effects` - Pre-translated kernel effects
    /// * `decision_receipt_id` - Receipt ID for audit linkage
    /// * `decision_hash` - Canonical decision hash (idempotency key)
    /// * `proposal_id` - The originating proposal ID
    ///
    /// # Returns
    /// Effect results, or an empty vec if already executed.
    pub async fn execute(
        &self,
        effects: Vec<KernelEffect>,
        decision_receipt_id: &str,
        decision_hash: &str,
        proposal_id: &str,
    ) -> Result<Vec<EffectResult>> {
        // 1. Idempotency check
        if let Some(existing) = self.store.get(decision_hash)? {
            if existing.is_terminal() {
                debug!(
                    decision_hash = %decision_hash,
                    status = ?existing.status,
                    "Decision already executed (idempotency check), skipping"
                );
                return Ok(vec![]);
            }

            // If it's in Executing state, another execution may be in progress
            // or a prior attempt crashed. For the skeleton, we log and proceed.
            if existing.status == ExecutionStatus::Executing {
                warn!(
                    decision_hash = %decision_hash,
                    "Decision in Executing state (possible crash recovery), re-executing"
                );
            }

            // If it's in Failed state, check retry limit
            if existing.status == ExecutionStatus::Failed && existing.retries >= MAX_RETRIES {
                warn!(
                    decision_hash = %decision_hash,
                    receipt_id = %decision_receipt_id,
                    retries = existing.retries,
                    status_before = ?ExecutionStatus::Failed,
                    status_after = ?ExecutionStatus::PermanentlyFailed,
                    "Decision state transition: Failed → PermanentlyFailed (max retries)"
                );
                let mut record = existing;
                record.mark_permanently_failed("Max retries exceeded");
                self.store.put(&record)?;
                self.mark_related_escrows_failed(&record, "max retries exceeded");
                return Ok(vec![]);
            }
        }

        // 2. Preserve existing record if present (retains retry count),
        //    otherwise create a new Pending record.
        let (mut record, status_before) = if let Some(existing) = self.store.get(decision_hash)? {
            if !existing.is_terminal() {
                // Preserve retry count, update effects for this attempt
                let prev = existing.status;
                let mut rec = existing;
                rec.effects = effects.clone();
                (rec, Some(prev))
            } else {
                // Terminal records were already caught by idempotency check above,
                // but handle defensively.
                (
                    ExecutionRecord::new_pending(
                        decision_hash,
                        proposal_id,
                        decision_receipt_id,
                        effects.clone(),
                    ),
                    None,
                )
            }
        } else {
            (
                ExecutionRecord::new_pending(
                    decision_hash,
                    proposal_id,
                    decision_receipt_id,
                    effects.clone(),
                ),
                None,
            )
        };
        record.status = ExecutionStatus::Pending;
        self.store.put(&record)?;

        // 3. Transition to Executing
        record.mark_executing();
        self.store.put(&record)?;

        info!(
            decision_hash = %decision_hash,
            proposal_id = %proposal_id,
            receipt_id = %decision_receipt_id,
            effect_count = effects.len(),
            attempt = record.retries + 1,
            status_before = ?status_before.unwrap_or(ExecutionStatus::Pending),
            status_after = ?ExecutionStatus::Executing,
            "Decision state transition: → Executing"
        );

        // 4. Delegate to EffectDispatcher
        let results = match self
            .dispatcher
            .execute_effects(effects, decision_receipt_id)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                error!(
                    decision_hash = %decision_hash,
                    receipt_id = %decision_receipt_id,
                    attempt = record.retries + 1,
                    status_before = ?ExecutionStatus::Executing,
                    status_after = ?ExecutionStatus::Failed,
                    error = %e,
                    "Decision state transition: Executing → Failed (dispatcher error)"
                );
                record.mark_failed(e.to_string());
                self.store.put(&record)?;
                return Err(e);
            }
        };

        // 5. Check results and record outcome
        let hard_failure_messages: Vec<String> = results
            .iter()
            .filter(|r| r.is_hard_failure())
            .map(|r| r.message.clone())
            .collect();

        let total_effects = results.len();
        let not_executed_effects = results.iter().filter(|r| r.not_executed).count();
        let hard_failed_effects = results.iter().filter(|r| r.is_hard_failure()).count();
        let executed_effects = total_effects
            .saturating_sub(not_executed_effects)
            .saturating_sub(hard_failed_effects);
        let execution_complete =
            total_effects > 0 && not_executed_effects == 0 && hard_failed_effects == 0;
        record.truth_summary = Some(ExecutionTruthSummary {
            total_effects,
            executed_effects,
            not_executed_effects,
            hard_failed_effects,
            execution_complete,
        });

        let all_non_executable = !results.is_empty() && results.iter().all(|r| r.not_executed);
        let mixed_non_executable =
            results.iter().any(|r| r.success) && results.iter().any(|r| r.not_executed);

        if !hard_failure_messages.is_empty() {
            let error_msg = hard_failure_messages.join("; ");

            warn!(
                decision_hash = %decision_hash,
                receipt_id = %decision_receipt_id,
                attempt = record.retries,
                status_before = ?ExecutionStatus::Executing,
                status_after = ?ExecutionStatus::Failed,
                failures = ?hard_failure_messages,
                "Decision state transition: Executing → Failed"
            );

            record.mark_failed(error_msg);
            self.store.put(&record)?;
        } else if all_non_executable {
            let note = results
                .iter()
                .map(|r| r.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");

            info!(
                decision_hash = %decision_hash,
                receipt_id = %decision_receipt_id,
                result_count = results.len(),
                attempt = record.retries + 1,
                status_before = ?ExecutionStatus::Executing,
                status_after = ?ExecutionStatus::NotExecuted,
                "Decision state transition: Executing → NotExecuted"
            );

            record.mark_not_executed(note);
            self.store.put(&record)?;
        } else {
            let ledger_entry_ids: Vec<String> = results
                .iter()
                .filter_map(|r| r.ledger_entry_id.clone())
                .collect();
            let state_change_hashes: Vec<String> = results
                .iter()
                .filter_map(|r| r.state_change_hash.clone())
                .collect();
            record.mark_confirmed(ledger_entry_ids, state_change_hashes);
            if mixed_non_executable {
                let mixed_count = results.iter().filter(|r| r.not_executed).count();
                let total = results.len();
                let note = format!(
                    "Confirmed with partial execution: {mixed_count}/{total} effect(s) were not executed"
                );
                record.error = Some(note.clone());
                warn!(
                    decision_hash = %decision_hash,
                    receipt_id = %decision_receipt_id,
                    mixed_not_executed = mixed_count,
                    total_effects = total,
                    note = %note,
                    "Decision confirmed with mixed executable/non-executable effect rows"
                );
            } else {
                record.error = None;
            }
            self.store.put(&record)?;

            info!(
                decision_hash = %decision_hash,
                receipt_id = %decision_receipt_id,
                result_count = results.len(),
                attempt = record.retries + 1,
                status_before = ?ExecutionStatus::Executing,
                status_after = ?ExecutionStatus::Confirmed,
                "Decision state transition: Executing → Confirmed"
            );
        }

        Ok(results)
    }

    /// Query execution status for a decision hash.
    pub fn get_status(&self, decision_hash: &str) -> Result<Option<ExecutionRecord>> {
        self.store.get(decision_hash)
    }

    /// List all records with a given status.
    pub fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
        self.store.list_by_status(status)
    }
}

/// Report from startup recovery scan.
#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Decisions that were re-executed and confirmed.
    pub recovered_confirmed: usize,
    /// Decisions that were re-executed and completed as structurally not executed.
    pub recovered_not_executed: usize,
    /// Decisions that were re-executed but failed again.
    pub recovered_failed: usize,
    /// Decisions skipped because they have no stored effects (pre-upgrade records).
    pub skipped_no_effects: usize,
    /// Decisions skipped because they exhausted retry limits.
    pub skipped_max_retries: usize,
}

impl RecoveryReport {
    /// Total decisions processed during recovery.
    pub fn total(&self) -> usize {
        self.recovered_confirmed
            + self.recovered_not_executed
            + self.recovered_failed
            + self.skipped_no_effects
            + self.skipped_max_retries
    }
}

/// Report from startup retention cleanup.
#[derive(Debug, Default)]
pub struct CleanupReport {
    /// Number of terminal records deleted for exceeding age threshold.
    pub deleted_old: usize,
    /// Number of terminal records deleted to enforce count threshold.
    pub deleted_excess: usize,
}

/// Create an effect executor callback that routes through the DecisionExecutor.
///
/// The callback acquires a semaphore permit (backpressure) before spawning
/// the execution task. If the semaphore is full, the task waits until a
/// slot opens, preventing unbounded concurrent executions.
pub fn create_decision_executor_callback(
    executor: Arc<DecisionExecutor>,
) -> Arc<dyn Fn(Vec<KernelEffect>, String) + Send + Sync> {
    Arc::new(move |effects, decision_receipt_id| {
        if effects.is_empty() {
            debug!(
                receipt_id = %decision_receipt_id,
                "No effects to execute (empty batch)"
            );
            return;
        }

        let executor = executor.clone();
        let semaphore = executor.concurrency_limit.clone();
        let effect_count = effects.len();

        // Extract decision_hash from first effect that carries one.
        // Treasury and some other effects embed decision_hash.
        let decision_hash =
            extract_decision_hash(&effects).unwrap_or_else(|| decision_receipt_id.clone());

        // Use receipt_id as proposal_id fallback — the app layer should
        // pass a richer context once the bridge is fully wired.
        let proposal_id = decision_receipt_id.clone();

        tokio::spawn(async move {
            // Backpressure: wait for a slot before executing.
            // Track wait time to detect saturation.
            let wait_start = std::time::Instant::now();
            let _permit = match semaphore.acquire().await {
                Ok(permit) => {
                    let waited = wait_start.elapsed();
                    if waited > std::time::Duration::from_millis(100) {
                        warn!(
                            decision_hash = %decision_hash,
                            waited_ms = waited.as_millis() as u64,
                            "Semaphore saturation: decision waited for execution slot"
                        );
                    }
                    permit
                }
                Err(_) => {
                    error!(
                        decision_hash = %decision_hash,
                        "Semaphore closed — executor shutting down"
                    );
                    return;
                }
            };

            match executor
                .execute(effects, &decision_receipt_id, &decision_hash, &proposal_id)
                .await
            {
                Ok(results) => {
                    let success_count = results.iter().filter(|r| r.success).count();
                    info!(
                        receipt_id = %decision_receipt_id,
                        decision_hash = %decision_hash,
                        total = results.len(),
                        success = success_count,
                        "Decision execution complete"
                    );
                }
                Err(e) => {
                    error!(
                        receipt_id = %decision_receipt_id,
                        decision_hash = %decision_hash,
                        effect_count = effect_count,
                        error = %e,
                        "Decision execution failed"
                    );
                }
            }
            // _permit drops here, releasing the semaphore slot
        });
    })
}

/// Extract `decision_hash` from the first effect that carries one.
fn extract_decision_hash(effects: &[KernelEffect]) -> Option<String> {
    use icn_kernel_api::effects::TreasuryEffect;
    for effect in effects {
        if let KernelEffect::Treasury(
            TreasuryEffect::Spend { decision_hash, .. }
            | TreasuryEffect::CreateBudget { decision_hash, .. }
            | TreasuryEffect::ReleaseEscrow { decision_hash, .. },
        ) = effect
        {
            if !decision_hash.is_empty() {
                return Some(decision_hash.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::effects::{EffectResult, KernelEffect, ResourceEffect, TreasuryEffect};
    use icn_kernel_api::escrow::{EscrowRecord, EscrowStatus, EscrowStore};
    use icn_kernel_api::execution::ExecutionRecord;
    use icn_kernel_api::services::{LedgerEvent, TreasuryEntryRequest, TreasuryEntryResult};
    use icn_kernel_api::{AllowAllOracle, Did, LedgerService, PolicyOracle};
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// Regression guard: per-effect row semantics the [`DecisionExecutor`] aggregation assumes.
    fn assert_per_effect_row_contract(r: &EffectResult) {
        assert!(
            !(r.success && r.not_executed),
            "per-effect contract: success and not_executed are mutually exclusive: {:?}",
            r
        );
        if r.not_executed {
            assert!(
                r.ledger_entry_id.is_none(),
                "per-effect contract: not_executed must not set ledger_entry_id: {:?}",
                r
            );
            assert!(
                r.state_change_hash.is_none(),
                "per-effect contract: not_executed must not set state_change_hash: {:?}",
                r
            );
        }
    }

    /// In-memory execution store for testing.
    struct MemoryExecutionStore {
        records: RwLock<HashMap<String, ExecutionRecord>>,
    }

    impl MemoryExecutionStore {
        fn new() -> Self {
            Self {
                records: RwLock::new(HashMap::new()),
            }
        }
    }

    impl ExecutionStore for MemoryExecutionStore {
        fn get(&self, decision_hash: &str) -> Result<Option<ExecutionRecord>> {
            Ok(self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .get(decision_hash)
                .cloned())
        }

        fn put(&self, record: &ExecutionRecord) -> Result<()> {
            self.records
                .write()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .insert(record.decision_hash.clone(), record.clone());
            Ok(())
        }

        fn delete(&self, decision_hash: &str) -> Result<()> {
            self.records
                .write()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .remove(decision_hash);
            Ok(())
        }

        fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
            Ok(self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .values()
                .filter(|r| r.status == status)
                .cloned()
                .collect())
        }

        fn count_by_status(&self) -> Result<HashMap<ExecutionStatus, usize>> {
            let mut counts = HashMap::new();
            for record in self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .values()
            {
                *counts.entry(record.status).or_insert(0) += 1;
            }
            Ok(counts)
        }
    }

    /// In-memory escrow store for testing terminal remediation behavior.
    struct MemoryEscrowStore {
        records: RwLock<HashMap<String, EscrowRecord>>,
    }

    impl MemoryEscrowStore {
        fn new() -> Self {
            Self {
                records: RwLock::new(HashMap::new()),
            }
        }
    }

    impl EscrowStore for MemoryEscrowStore {
        fn get(&self, escrow_id: &str) -> Result<Option<EscrowRecord>> {
            Ok(self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .get(escrow_id)
                .cloned())
        }

        fn put(&self, record: &EscrowRecord) -> Result<()> {
            self.records
                .write()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .insert(record.escrow_id.clone(), record.clone());
            Ok(())
        }

        fn list_by_scope(&self, scope_id: &str) -> Result<Vec<EscrowRecord>> {
            Ok(self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .values()
                .filter(|r| r.scope_id == scope_id)
                .cloned()
                .collect())
        }
    }

    /// Test-only `LedgerService` that accepts treasury writes without pulling in the ledger crate
    /// (meaning-firewall ratchet counts qualified ledger paths in icn-core `src/`).
    struct StubTreasuryLedgerService {
        oracle: Arc<dyn PolicyOracle>,
    }

    impl LedgerService for StubTreasuryLedgerService {
        fn oracle(&self) -> Arc<dyn PolicyOracle> {
            self.oracle.clone()
        }

        fn balance(&self, _account: &Did, _currency: &str) -> i64 {
            0
        }

        fn credit_limit(&self, _account: &Did, _currency: &str) -> i64 {
            0
        }

        fn record_event(&self, _event: LedgerEvent) {}

        fn submit_treasury_entry(
            &self,
            entry: TreasuryEntryRequest,
        ) -> Result<TreasuryEntryResult, String> {
            Ok(TreasuryEntryResult {
                entry_hash: "0000000000000000000000000000000000000000000000000000000000000001"
                    .to_string(),
                decision_receipt_id: entry.decision_receipt_id,
                decision_hash: entry.decision_hash,
            })
        }

        fn get_treasury_nonce(&self, _treasury_id: &str) -> Result<u64, String> {
            Ok(0)
        }
    }

    /// Helper: create a DecisionExecutor with in-memory stores and no ledger-backed treasury.
    fn make_test_executor() -> (Arc<DecisionExecutor>, Arc<MemoryExecutionStore>) {
        use icn_kernel_api::protocol_params::*;

        let param_store = Arc::new(StubParamStore);
        let kernel_executor =
            Arc::new(super::super::governance_executor::KernelGovernanceExecutor::new(param_store));
        let dispatcher = Arc::new(EffectDispatcher::new(kernel_executor));
        let exec_store = Arc::new(MemoryExecutionStore::new());

        let decision_executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));

        (decision_executor, exec_store)
    }

    /// Same as [`make_test_executor`] but with a stub `LedgerService` so treasury spend can confirm.
    fn make_test_executor_with_stub_ledger() -> (Arc<DecisionExecutor>, Arc<MemoryExecutionStore>) {
        use icn_kernel_api::protocol_params::StubParamStore;

        let oracle = Arc::new(AllowAllOracle::wildcard());
        let ledger_service: Arc<dyn LedgerService> = Arc::new(StubTreasuryLedgerService {
            oracle: oracle.clone(),
        });
        let param_store = Arc::new(StubParamStore);
        let kernel_executor = Arc::new(
            super::super::governance_executor::KernelGovernanceExecutor::new(param_store)
                .with_ledger_service(ledger_service),
        );
        let dispatcher = Arc::new(EffectDispatcher::new(kernel_executor));
        let exec_store = Arc::new(MemoryExecutionStore::new());
        let decision_executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
        (decision_executor, exec_store)
    }

    fn make_test_executor_with_escrow() -> (
        Arc<DecisionExecutor>,
        Arc<MemoryExecutionStore>,
        Arc<MemoryEscrowStore>,
    ) {
        use icn_kernel_api::protocol_params::*;

        let param_store = Arc::new(StubParamStore);
        let kernel_executor =
            Arc::new(super::super::governance_executor::KernelGovernanceExecutor::new(param_store));
        let dispatcher = Arc::new(EffectDispatcher::new(kernel_executor));
        let exec_store = Arc::new(MemoryExecutionStore::new());
        let escrow_store = Arc::new(MemoryEscrowStore::new());

        let decision_executor = Arc::new(
            DecisionExecutor::new(dispatcher, exec_store.clone())
                .with_escrow_store(escrow_store.clone()),
        );

        (decision_executor, exec_store, escrow_store)
    }

    #[tokio::test]
    async fn test_execute_records_status() {
        let (executor, store) = make_test_executor();

        let effects = vec![KernelEffect::NoOp {
            reason: "test".to_string(),
        }];

        let results = executor
            .execute(effects, "receipt-1", "hash-1", "proposal-1")
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            !results[0].success,
            "NoOp must not report success when nothing was executed"
        );
        assert!(
            results[0]
                .message
                .contains("no executable kernel effect (not executed)"),
            "expected honest NoOp message, got {:?}",
            results[0].message
        );
        assert!(results[0].not_executed);
        assert_per_effect_row_contract(&results[0]);

        // Verify record was persisted as NotExecuted (terminal, not retryable failure)
        let record = store.get("hash-1").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::NotExecuted);
        assert_ne!(
            record.status,
            ExecutionStatus::Confirmed,
            "contract: NoOp-only batch must not aggregate to Confirmed"
        );
        assert!(record.is_terminal());
        assert_eq!(record.proposal_id, "proposal-1");
        assert!(record.finished_at.is_some());
        assert_eq!(record.retries, 0);
    }

    #[tokio::test]
    async fn test_idempotency_skips_confirmed() {
        let (executor, store) = make_test_executor();

        // Terminal Confirmed records must not re-execute (idempotency).
        // NoOp alone no longer reaches Confirmed, so seed Confirmed directly.
        let mut record = ExecutionRecord::new_pending(
            "hash-1",
            "proposal-1",
            "receipt-1",
            vec![KernelEffect::NoOp {
                reason: "seed".to_string(),
            }],
        );
        record.mark_executing();
        record.mark_confirmed(vec![], vec![]);
        store.put(&record).unwrap();

        let effects2 = vec![KernelEffect::NoOp {
            reason: "duplicate".to_string(),
        }];
        let results2 = executor
            .execute(effects2, "receipt-1", "hash-1", "proposal-1")
            .await
            .unwrap();
        assert!(
            results2.is_empty(),
            "Duplicate execution should return empty when already Confirmed"
        );

        let record = store.get("hash-1").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
    }

    #[tokio::test]
    async fn test_idempotency_skips_permanently_failed() {
        let (executor, store) = make_test_executor();

        // Manually insert a permanently failed record
        let mut record = ExecutionRecord::new_pending("hash-pf", "p-1", "r-1", vec![]);
        record.mark_permanently_failed("Non-recoverable");
        store.put(&record).unwrap();

        // Attempt execution — should be skipped
        let effects = vec![KernelEffect::NoOp {
            reason: "retry".to_string(),
        }];
        let results = executor
            .execute(effects, "r-1", "hash-pf", "p-1")
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_idempotency_skips_not_executed() {
        let (executor, store) = make_test_executor();

        let mut record = ExecutionRecord::new_pending(
            "hash-nx",
            "p-nx",
            "r-nx",
            vec![KernelEffect::NoOp {
                reason: "seed".to_string(),
            }],
        );
        record.mark_executing();
        record.mark_not_executed("seed terminal");
        store.put(&record).unwrap();

        let results = executor
            .execute(
                vec![KernelEffect::NoOp {
                    reason: "replay".to_string(),
                }],
                "r-nx",
                "hash-nx",
                "p-nx",
            )
            .await
            .unwrap();
        assert!(results.is_empty(), "terminal NotExecuted must not re-run");
    }

    #[tokio::test]
    async fn test_treasury_effect_with_decision_hash() {
        let (executor, store) = make_test_executor_with_stub_ledger();

        let effects = vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9".to_string(),
            recipient_did: "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse".to_string(),
            amount: 500,
            currency: "HOURS".to_string(),
            memo: "Test".to_string(),
            budget_id: None,
            expected_nonce: 0,
            decision_receipt_id: "receipt-t1".to_string(),
            decision_hash: "sha256:treasury-hash-1".to_string(),
        })];

        let results = executor
            .execute(
                effects,
                "receipt-t1",
                "sha256:treasury-hash-1",
                "proposal-t1",
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_per_effect_row_contract(&results[0]);

        let record = store.get("sha256:treasury-hash-1").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
    }

    #[tokio::test]
    async fn test_extract_decision_hash_from_effects() {
        let effects = vec![
            KernelEffect::NoOp {
                reason: "first".to_string(),
            },
            KernelEffect::Treasury(TreasuryEffect::Spend {
                treasury_did: "t".to_string(),
                recipient_did: "r".to_string(),
                amount: 100,
                currency: "ICN".to_string(),
                memo: "m".to_string(),
                budget_id: None,
                expected_nonce: 0,
                decision_receipt_id: "r1".to_string(),
                decision_hash: "extracted-hash".to_string(),
            }),
        ];

        assert_eq!(
            extract_decision_hash(&effects),
            Some("extracted-hash".to_string())
        );

        // No treasury effects → None
        let no_hash = vec![KernelEffect::NoOp {
            reason: "only".to_string(),
        }];
        assert_eq!(extract_decision_hash(&no_hash), None);
    }

    #[tokio::test]
    async fn test_different_decisions_execute_independently() {
        let (executor, store) = make_test_executor();

        // Decision A
        executor
            .execute(
                vec![KernelEffect::NoOp { reason: "a".into() }],
                "r-a",
                "hash-a",
                "p-a",
            )
            .await
            .unwrap();

        // Decision B
        executor
            .execute(
                vec![KernelEffect::NoOp { reason: "b".into() }],
                "r-b",
                "hash-b",
                "p-b",
            )
            .await
            .unwrap();

        // Both should be independently recorded as NotExecuted (NoOp is not execution)
        let a = store.get("hash-a").unwrap().unwrap();
        let b = store.get("hash-b").unwrap().unwrap();
        assert_eq!(a.status, ExecutionStatus::NotExecuted);
        assert_eq!(b.status, ExecutionStatus::NotExecuted);
        assert_ne!(a.proposal_id, b.proposal_id);
    }

    #[tokio::test]
    async fn test_exec_record_retention_cleanup_old_records_but_keeps_recent() {
        let (executor, store) = make_test_executor();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut old_confirmed = ExecutionRecord::new_pending("old-c", "p-old", "r-old", vec![]);
        old_confirmed.status = ExecutionStatus::Confirmed;
        old_confirmed.started_at = now.saturating_sub(MAX_CONFIRMED_AGE_SECS + 100);
        old_confirmed.finished_at = Some(now.saturating_sub(MAX_CONFIRMED_AGE_SECS + 100));
        store.put(&old_confirmed).unwrap();

        let mut recent_confirmed = ExecutionRecord::new_pending("new-c", "p-new", "r-new", vec![]);
        recent_confirmed.status = ExecutionStatus::Confirmed;
        recent_confirmed.started_at = now.saturating_sub(60);
        recent_confirmed.finished_at = Some(now.saturating_sub(60));
        store.put(&recent_confirmed).unwrap();

        let report = executor.cleanup_old_records().unwrap();
        assert_eq!(report.deleted_old, 1);
        assert!(store.get("old-c").unwrap().is_none());
        assert!(store.get("new-c").unwrap().is_some());
    }

    #[tokio::test]
    async fn test_permanent_failure_marks_releasing_escrow_failed() {
        let (executor, exec_store, escrow_store) = make_test_executor_with_escrow();

        let decision_hash = "sha256:escrow-fail-hash";
        let escrow_id = "escrow-fail-1";

        let mut escrow = EscrowRecord::new_locked(
            escrow_id,
            "coop-alpha",
            "did:treasury",
            "did:beneficiary",
            1000,
            "HOURS",
        );
        escrow.begin_release(decision_hash).unwrap();
        escrow_store.put(&escrow).unwrap();

        let mut failed_record = ExecutionRecord::new_pending(
            decision_hash,
            "proposal-escrow-fail",
            "receipt-escrow-fail",
            vec![KernelEffect::Treasury(TreasuryEffect::ReleaseEscrow {
                escrow_id: escrow_id.to_string(),
                treasury_did: "did:treasury".to_string(),
                beneficiary_did: "did:beneficiary".to_string(),
                amount: 1000,
                currency: "HOURS".to_string(),
                decision_receipt_id: "receipt-escrow-fail".to_string(),
                decision_hash: decision_hash.to_string(),
            })],
        );
        failed_record.status = ExecutionStatus::Failed;
        failed_record.retries = MAX_RETRIES;
        failed_record.error = Some("insufficient funds".to_string());
        exec_store.put(&failed_record).unwrap();

        let results = executor
            .execute(
                vec![KernelEffect::NoOp {
                    reason: "ignored".to_string(),
                }],
                "receipt-escrow-fail",
                decision_hash,
                "proposal-escrow-fail",
            )
            .await
            .unwrap();
        assert!(results.is_empty());

        let updated_record = exec_store.get(decision_hash).unwrap().unwrap();
        assert_eq!(updated_record.status, ExecutionStatus::PermanentlyFailed);

        let updated_escrow = escrow_store.get(escrow_id).unwrap().unwrap();
        assert_eq!(updated_escrow.status, EscrowStatus::Failed);
        assert_eq!(
            updated_escrow.failed_decision_hash.as_deref(),
            Some(decision_hash)
        );
        assert!(updated_escrow
            .failed_reason
            .as_deref()
            .unwrap_or_default()
            .contains("max retries exceeded"));
    }

    /// When at least one effect reports `success` and there are no hard failures
    /// (`!success && !not_executed`), aggregation uses the `Confirmed` branch even if another
    /// effect is `not_executed`. Per-effect results remain truthful; the persisted
    /// [`ExecutionRecord`] does not encode per-effect outcomes.
    #[tokio::test]
    async fn test_aggregate_confirmed_when_mixed_success_and_not_executed() {
        let (executor, store) = make_test_executor_with_stub_ledger();

        let decision_hash = "sha256:mixed-aggregate-1";
        let effects = vec![
            KernelEffect::NoOp {
                reason: "not executable".to_string(),
            },
            KernelEffect::Treasury(TreasuryEffect::Spend {
                treasury_did: "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9".to_string(),
                recipient_did: "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse".to_string(),
                amount: 10,
                currency: "HOURS".to_string(),
                memo: "mixed batch".to_string(),
                budget_id: None,
                expected_nonce: 0,
                decision_receipt_id: "receipt-mixed-1".to_string(),
                decision_hash: decision_hash.to_string(),
            }),
        ];

        let results = executor
            .execute(
                effects,
                "receipt-mixed-1",
                decision_hash,
                "proposal-mixed-1",
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].not_executed && !results[0].success);
        assert!(results[1].success && !results[1].not_executed);
        for r in &results {
            assert_per_effect_row_contract(r);
        }

        let record = store.get(decision_hash).unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
        assert!(
            record
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("partial execution"),
            "mixed execution should preserve explicit partial note in record.error"
        );
    }

    #[tokio::test]
    async fn test_deferred_treasury_outcome_persists_not_executed() {
        // Ledger is wired (production-shaped); `TreasuryEffect::Allocate` maps to a
        // `TreasuryOperation` without `decision_hash`, so `KernelTreasuryExecutor` returns
        // `ExecutionOutcome::Deferred` — not the "no ledger" branch.
        let (executor, exec_store) = make_test_executor_with_stub_ledger();

        let decision_hash = "sha256:deferred-proof-1";
        let effects = vec![KernelEffect::Treasury(TreasuryEffect::Allocate {
            treasury_did: "did:icn:treasury".to_string(),
            budget_id: "budget-defer-proof".to_string(),
            amount: 100,
            currency: "HOURS".to_string(),
        })];

        let results = executor
            .execute(
                effects,
                "receipt-defer-1",
                decision_hash,
                "proposal-defer-1",
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].not_executed);
        assert!(
            results[0].message.contains("Deferred:"),
            "expected Deferred prefix in message, got {:?}",
            results[0].message
        );
        assert!(
            results[0].message.contains("decision_hash"),
            "expected production defer path (missing provenance), got {:?}",
            results[0].message
        );
        assert_per_effect_row_contract(&results[0]);

        let record = exec_store.get(decision_hash).unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::NotExecuted);
        assert_eq!(record.retries, 0);
        assert!(
            record.error.as_deref().unwrap_or("").contains("Deferred:"),
            "persisted note should mention Deferred, got {:?}",
            record.error
        );
    }

    #[tokio::test]
    async fn contract_noop_only_batch_never_aggregates_to_confirmed() {
        let (executor, store) = make_test_executor();

        let results = executor
            .execute(
                vec![KernelEffect::NoOp {
                    reason: "contract".into(),
                }],
                "receipt-contract-noop",
                "hash-contract-noop-only",
                "proposal-contract-noop",
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_per_effect_row_contract(&results[0]);

        let record = store.get("hash-contract-noop-only").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::NotExecuted);
        assert_ne!(
            record.status,
            ExecutionStatus::Confirmed,
            "contract: NoOp-only batch must not aggregate to Confirmed"
        );
        assert!(record.is_terminal());
        assert_eq!(record.retries, 0);
    }

    #[tokio::test]
    async fn contract_hard_failure_row_aggregates_to_failed_and_increments_retries() {
        let (executor, store) = make_test_executor();

        let decision_hash = "sha256:contract-resource-hard-fail";
        let effects = vec![KernelEffect::Resource(ResourceEffect::GrantAccess {
            grantee_did: "did:icn:g".into(),
            resource_type: "rt".into(),
            access_model_hash: "mh".into(),
        })];

        let results = executor
            .execute(effects, "receipt-rf", decision_hash, "proposal-rf")
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].is_hard_failure());
        assert_per_effect_row_contract(&results[0]);

        let record = store.get(decision_hash).unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Failed);
        assert_eq!(record.retries, 1);
        assert!(!record.is_terminal());
    }
}
