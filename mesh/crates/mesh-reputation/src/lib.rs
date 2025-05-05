use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cid::Cid;
use icn_dag::{DagEvent, DagInterface};
use icn_identity::Did;
use mesh_types::{
    ExecutionReceipt, MeshPolicy, ReputationSnapshot, TaskIntent, VerificationReceipt,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// The main reputation scoring system for the mesh network
pub struct ReputationSystem {
    /// Current reputation scores by peer DID
    scores: Arc<Mutex<HashMap<Did, f64>>>,
    
    /// Historical execution performance by peer DID
    execution_history: Arc<Mutex<HashMap<Did, Vec<ExecutionMetrics>>>>,
    
    /// Historical verification accuracy by peer DID
    verification_history: Arc<Mutex<HashMap<Did, Vec<VerificationMetrics>>>>,
    
    /// Current active policy
    policy: Arc<Mutex<MeshPolicy>>,
    
    /// Channel for emitting reputation updates
    update_sender: Option<mpsc::Sender<ReputationSnapshot>>,
}

/// Metrics related to task execution performance
#[derive(Debug, Clone)]
struct ExecutionMetrics {
    /// When the task was executed
    timestamp: DateTime<Utc>,
    
    /// Did the execution complete successfully?
    success: bool,
    
    /// How much fuel was used compared to estimated?
    efficiency: f64,
    
    /// Time to completion relative to estimated time
    time_efficiency: f64,
}

/// Metrics related to verification performance
#[derive(Debug, Clone)]
struct VerificationMetrics {
    /// When the verification was performed
    timestamp: DateTime<Utc>,
    
    /// Did this verification match consensus verdict?
    accuracy: bool,
    
    /// Was the verification submitted on time?
    timeliness: f64,
}

impl ReputationSystem {
    /// Create a new reputation system
    pub fn new(policy: MeshPolicy) -> Self {
        Self {
            scores: Arc::new(Mutex::new(HashMap::new())),
            execution_history: Arc::new(Mutex::new(HashMap::new())),
            verification_history: Arc::new(Mutex::new(HashMap::new())),
            policy: Arc::new(Mutex::new(policy)),
            update_sender: None,
        }
    }
    
    /// Set a channel for emitting reputation updates
    pub fn set_update_channel(&mut self, sender: mpsc::Sender<ReputationSnapshot>) {
        self.update_sender = Some(sender);
    }
    
    /// Update the active policy
    pub fn update_policy(&self, policy: MeshPolicy) -> Result<()> {
        let mut current = self.policy.lock().unwrap();
        *current = policy;
        info!("Updated reputation policy");
        Ok(())
    }
    
    /// Process an execution receipt
    pub async fn process_execution(&self, receipt: &ExecutionReceipt) -> Result<()> {
        let worker_did = &receipt.worker_did;
        
        // In a real implementation, this would analyze the execution
        // For simplicity, we'll just add a successful execution metric
        let metrics = ExecutionMetrics {
            timestamp: receipt.timestamp,
            success: true,
            efficiency: 1.0, // Perfect efficiency for now
            time_efficiency: 1.0, // Perfect time efficiency for now
        };
        
        // Add to execution history
        {
            let mut history = self.execution_history.lock().unwrap();
            let worker_history = history.entry(worker_did.clone()).or_insert_with(Vec::new);
            worker_history.push(metrics);
        }
        
        // Recalculate reputation
        self.recalculate_reputation(worker_did).await?;
        
        info!("Processed execution receipt for reputation scoring");
        Ok(())
    }
    
    /// Process a verification receipt
    pub async fn process_verification(&self, receipt: &VerificationReceipt, consensus_verdict: bool) -> Result<()> {
        let verifier_did = &receipt.verifier_did;
        
        // Calculate if this verification was accurate
        let accuracy = receipt.verdict == consensus_verdict;
        
        // Record the verification metrics
        let metrics = VerificationMetrics {
            timestamp: receipt.timestamp,
            accuracy,
            timeliness: 1.0, // Assume perfect timeliness for now
        };
        
        // Add to verification history
        {
            let mut history = self.verification_history.lock().unwrap();
            let verifier_history = history.entry(verifier_did.clone()).or_insert_with(Vec::new);
            verifier_history.push(metrics);
        }
        
        // Recalculate reputation
        self.recalculate_reputation(verifier_did).await?;
        
        info!("Processed verification receipt for reputation scoring");
        Ok(())
    }
    
    /// Slash a peer's reputation
    pub async fn slash_reputation(&self, peer_did: &Did, reason: &str, amount: f64) -> Result<()> {
        // Get current score
        let mut scores = self.scores.lock().unwrap();
        let current_score = scores.entry(peer_did.clone()).or_insert(0.5); // Default to 0.5
        
        // Apply slashing
        *current_score = (*current_score - amount).max(0.0);
        
        warn!("Slashed reputation for {} by {} because: {}", peer_did, amount, reason);
        
        // Emit update if channel exists
        if let Some(sender) = &self.update_sender {
            let snapshot = ReputationSnapshot {
                did: peer_did.clone(),
                score: *current_score,
                timestamp: Utc::now(),
                components: None, // Could add details here
            };
            
            if let Err(e) = sender.send(snapshot).await {
                error!("Failed to send reputation update: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Apply decay to all reputation scores
    pub async fn apply_decay(&self) -> Result<()> {
        let policy = self.policy.lock().unwrap();
        let decay_rate = policy.lambda;
        
        let mut scores = self.scores.lock().unwrap();
        
        for (did, score) in scores.iter_mut() {
            // Apply exponential decay
            *score = *score * (1.0 - decay_rate);
            
            debug!("Applied decay to {}: {} (rate: {})", did, score, decay_rate);
        }
        
        info!("Applied reputation decay to all peers");
        Ok(())
    }
    
    /// Checkpoint reputation scores to the DAG
    pub async fn checkpoint_to_dag<D: DagInterface>(&self, dag: &D) -> Result<()> {
        let scores = self.scores.lock().unwrap();
        
        // Convert to a format suitable for the DAG
        let scores_map: HashMap<String, f64> = scores
            .iter()
            .map(|(did, score)| (did.clone(), *score))
            .collect();
        
        // Create a DAG event
        let event = DagEvent::ReputationSnapshot {
            scores: scores_map,
            timestamp: Utc::now(),
        };
        
        // Submit to DAG
        dag.submit_event(event).await?;
        
        info!("Checkpointed reputation scores to DAG");
        Ok(())
    }
    
    /// Get a peer's current reputation score
    pub fn get_score(&self, peer_did: &Did) -> f64 {
        let scores = self.scores.lock().unwrap();
        *scores.get(peer_did).unwrap_or(&0.5) // Default to 0.5 if no score
    }
    
    /// Get reputation scores for all peers
    pub fn get_all_scores(&self) -> HashMap<Did, f64> {
        self.scores.lock().unwrap().clone()
    }
    
    /// Recalculate a peer's reputation score based on history
    async fn recalculate_reputation(&self, peer_did: &Did) -> Result<()> {
        // Get current policy
        let policy = self.policy.lock().unwrap();
        
        // Calculate execution component
        let execution_score = {
            let history = self.execution_history.lock().unwrap();
            if let Some(metrics) = history.get(peer_did) {
                if metrics.is_empty() {
                    0.5 // Default
                } else {
                    // Calculate average of recent executions, weighted by recency
                    let mut total = 0.0;
                    let mut weight_sum = 0.0;
                    
                    for (i, metric) in metrics.iter().enumerate() {
                        let weight = (i + 1) as f64; // More recent items get higher weight
                        let score = if metric.success { 
                            // Combine efficiency metrics
                            0.5 + 0.5 * (metric.efficiency + metric.time_efficiency) / 2.0
                        } else {
                            0.0 // Failed executions get zero
                        };
                        
                        total += score * weight;
                        weight_sum += weight;
                    }
                    
                    total / weight_sum
                }
            } else {
                0.5 // Default for new peers
            }
        };
        
        // Calculate verification component
        let verification_score = {
            let history = self.verification_history.lock().unwrap();
            if let Some(metrics) = history.get(peer_did) {
                if metrics.is_empty() {
                    0.5 // Default
                } else {
                    // Calculate average accuracy, weighted by recency
                    let mut total = 0.0;
                    let mut weight_sum = 0.0;
                    
                    for (i, metric) in metrics.iter().enumerate() {
                        let weight = (i + 1) as f64; // More recent items get higher weight
                        let score = if metric.accuracy {
                            0.5 + 0.5 * metric.timeliness 
                        } else {
                            0.0 // Inaccurate verifications get zero
                        };
                        
                        total += score * weight;
                        weight_sum += weight;
                    }
                    
                    total / weight_sum
                }
            } else {
                0.5 // Default for new verifiers
            }
        };
        
        // Apply weights from policy
        let new_score = policy.alpha * execution_score + policy.beta * verification_score;
        
        // Update score
        {
            let mut scores = self.scores.lock().unwrap();
            scores.insert(peer_did.clone(), new_score);
        }
        
        // Emit update if channel exists
        if let Some(sender) = &self.update_sender {
            let mut components = HashMap::new();
            components.insert("execution".to_string(), execution_score);
            components.insert("verification".to_string(), verification_score);
            
            let snapshot = ReputationSnapshot {
                did: peer_did.clone(),
                score: new_score,
                timestamp: Utc::now(),
                components: Some(components),
            };
            
            if let Err(e) = sender.send(snapshot).await {
                error!("Failed to send reputation update: {}", e);
            }
        }
        
        debug!("Recalculated reputation for {}: {} = {}*{} + {}*{}", 
               peer_did, new_score, policy.alpha, execution_score, policy.beta, verification_score);
        
        Ok(())
    }
}

/// Trait for reputation system implementations
#[async_trait]
pub trait ReputationInterface {
    /// Process an execution receipt
    async fn process_execution(&self, receipt: &ExecutionReceipt) -> Result<()>;
    
    /// Process a verification receipt
    async fn process_verification(&self, receipt: &VerificationReceipt, consensus_verdict: bool) -> Result<()>;
    
    /// Get a peer's current reputation score
    fn get_score(&self, peer_did: &Did) -> f64;
    
    /// Slash a peer's reputation
    async fn slash_reputation(&self, peer_did: &Did, reason: &str, amount: f64) -> Result<()>;
    
    /// Apply decay to all reputation scores
    async fn apply_decay(&self) -> Result<()>;
}

/// Implement the reputation interface for the reputation system
#[async_trait]
impl ReputationInterface for ReputationSystem {
    async fn process_execution(&self, receipt: &ExecutionReceipt) -> Result<()> {
        self.process_execution(receipt).await
    }
    
    async fn process_verification(&self, receipt: &VerificationReceipt, consensus_verdict: bool) -> Result<()> {
        self.process_verification(receipt, consensus_verdict).await
    }
    
    fn get_score(&self, peer_did: &Did) -> f64 {
        self.get_score(peer_did)
    }
    
    async fn slash_reputation(&self, peer_did: &Did, reason: &str, amount: f64) -> Result<()> {
        self.slash_reputation(peer_did, reason, amount).await
    }
    
    async fn apply_decay(&self) -> Result<()> {
        self.apply_decay().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_reputation_calculation() {
        // Create a test policy
        let policy = MeshPolicy {
            alpha: 0.6,
            beta: 0.4,
            gamma: 1.0,
            lambda: 0.01,
            stake_weight: 0.2,
            min_fee: 10,
            capacity_units: 100,
        };
        
        let reputation = ReputationSystem::new(policy);
        
        // Test peer
        let peer_did = "did:icn:test:worker".to_string();
        
        // Add some test executions
        let execution = ExecutionReceipt {
            worker_did: peer_did.clone(),
            task_cid: Cid::default(),
            output_cid: Cid::default(),
            fuel_consumed: 1000,
            timestamp: Utc::now(),
            signature: vec![],
            metadata: None,
        };
        
        reputation.process_execution(&execution).await.unwrap();
        
        // Add some test verifications
        let verification = VerificationReceipt {
            verifier_did: peer_did.clone(),
            receipt_cid: Cid::default(),
            verdict: true,
            proof_cid: Cid::default(),
            timestamp: Utc::now(),
            signature: vec![],
            metadata: None,
        };
        
        reputation.process_verification(&verification, true).await.unwrap();
        
        // Get score
        let score = reputation.get_score(&peer_did);
        
        // Score should be a combination of execution (1.0) and verification (1.0)
        // weighted by alpha and beta
        let expected = policy.alpha * 1.0 + policy.beta * 1.0;
        
        assert!((score - expected).abs() < 0.01, "Score should be close to expected");
        
        // Test decay
        reputation.apply_decay().await.unwrap();
        let decayed_score = reputation.get_score(&peer_did);
        
        assert!(decayed_score < score, "Score should decrease after decay");
    }
}
