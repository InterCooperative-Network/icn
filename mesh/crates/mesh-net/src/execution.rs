use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use cid::Cid;
use mesh_types::{
    ComputeOffer, ExecutionReceipt, PeerInfo, ReputationSnapshot, TaskIntent, VerificationReceipt,
    events::MeshEvent,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Status of a task in the execution engine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task is published but no offers received yet
    Published,
    
    /// Task has received offers and is waiting for selection
    OffersReceived,
    
    /// Task has been assigned to a worker
    Assigned,
    
    /// Task execution is in progress
    InProgress,
    
    /// Task execution has completed
    Completed,
    
    /// Task has been verified
    Verified,
    
    /// Task execution failed
    Failed,
    
    /// Task verification failed
    VerificationFailed,
}

/// Core execution engine for the Mesh Compute system
pub struct MeshExecutionEngine {
    /// Map of tasks by CID
    tasks: Arc<Mutex<HashMap<Cid, TaskIntent>>>,
    
    /// Map of execution receipts by task CID
    receipts: Arc<Mutex<HashMap<Cid, ExecutionReceipt>>>,
    
    /// Map of verification receipts by execution receipt CID
    verifications: Arc<Mutex<HashMap<Cid, Vec<VerificationReceipt>>>>,
    
    /// Map of task status by task CID
    task_status: Arc<Mutex<HashMap<Cid, TaskStatus>>>,
    
    /// Map of offers by task CID
    offers: Arc<Mutex<HashMap<Cid, Vec<ComputeOffer>>>>,
    
    /// Channel for sending events to the network
    event_sender: mpsc::Sender<MeshEvent>,
    
    /// Directory where WASM modules are stored
    wasm_dir: PathBuf,
    
    /// Directory where input/output data is stored
    data_dir: PathBuf,
}

impl MeshExecutionEngine {
    /// Create a new execution engine
    pub fn new(event_sender: mpsc::Sender<MeshEvent>, wasm_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            receipts: Arc::new(Mutex::new(HashMap::new())),
            verifications: Arc::new(Mutex::new(HashMap::new())),
            task_status: Arc::new(Mutex::new(HashMap::new())),
            offers: Arc::new(Mutex::new(HashMap::new())),
            event_sender,
            wasm_dir,
            data_dir,
        }
    }
    
    /// Process a new task intent
    pub async fn process_task(&self, task: TaskIntent) -> Result<()> {
        let task_cid = task_to_cid(&task)?;
        
        // Store the task
        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(task_cid.clone(), task.clone());
            
            let mut status = self.task_status.lock().unwrap();
            status.insert(task_cid.clone(), TaskStatus::Published);
        }
        
        info!("Processed new task with CID: {}", task_cid);
        Ok(())
    }
    
    /// Process a new execution offer
    pub async fn process_offer(&self, offer: ComputeOffer) -> Result<()> {
        // Validate the task exists
        {
            let tasks = self.tasks.lock().unwrap();
            if !tasks.contains_key(&offer.task_cid) {
                return Err(anyhow!("Task not found: {}", offer.task_cid));
            }
        }
        
        // Store the offer
        {
            let mut offers = self.offers.lock().unwrap();
            let task_offers = offers.entry(offer.task_cid.clone()).or_insert_with(Vec::new);
            task_offers.push(offer.clone());
            
            let mut status = self.task_status.lock().unwrap();
            status.insert(offer.task_cid.clone(), TaskStatus::OffersReceived);
        }
        
        info!("Processed new offer for task: {}", offer.task_cid);
        Ok(())
    }
    
    /// Process execution receipt
    pub async fn process_execution(&self, receipt: ExecutionReceipt) -> Result<()> {
        let receipt_cid = execution_to_cid(&receipt)?;
        
        // Validate the task exists
        {
            let tasks = self.tasks.lock().unwrap();
            if !tasks.contains_key(&receipt.task_cid) {
                return Err(anyhow!("Task not found: {}", receipt.task_cid));
            }
        }
        
        // Store the receipt
        {
            let mut receipts = self.receipts.lock().unwrap();
            receipts.insert(receipt.task_cid.clone(), receipt.clone());
            
            let mut status = self.task_status.lock().unwrap();
            status.insert(receipt.task_cid.clone(), TaskStatus::Completed);
        }
        
        info!("Processed execution receipt with CID: {}", receipt_cid);
        Ok(())
    }
    
    /// Process verification receipt
    pub async fn process_verification(&self, receipt: VerificationReceipt) -> Result<()> {
        // Validate the execution receipt exists
        {
            let receipts = self.receipts.lock().unwrap();
            let task_cid = receipts.iter()
                .find(|(_, r)| receipt.receipt_cid == execution_to_cid(r).unwrap())
                .map(|(cid, _)| cid.clone());
                
            if let Some(task_cid) = task_cid {
                // Store the verification
                let mut verifications = self.verifications.lock().unwrap();
                let receipt_verifications = verifications
                    .entry(receipt.receipt_cid.clone())
                    .or_insert_with(Vec::new);
                    
                receipt_verifications.push(receipt.clone());
                
                // Check if we have enough verifications
                let tasks = self.tasks.lock().unwrap();
                if let Some(task) = tasks.get(&task_cid) {
                    if receipt_verifications.len() >= task.verifiers as usize {
                        // Check if verifications are in agreement
                        let positive_verifications = receipt_verifications
                            .iter()
                            .filter(|v| v.verdict)
                            .count();
                            
                        let mut status = self.task_status.lock().unwrap();
                        if positive_verifications > receipt_verifications.len() / 2 {
                            status.insert(task_cid, TaskStatus::Verified);
                        } else {
                            status.insert(task_cid, TaskStatus::VerificationFailed);
                        }
                    }
                }
            } else {
                return Err(anyhow!("Execution receipt not found: {}", receipt.receipt_cid));
            }
        }
        
        info!("Processed verification receipt for execution: {}", receipt.receipt_cid);
        Ok(())
    }
    
    /// Select a worker for a task from the available offers
    pub async fn select_worker(&self, task_cid: &Cid) -> Result<ComputeOffer> {
        let offers = self.offers.lock().unwrap().get(task_cid).cloned();
        
        if let Some(task_offers) = offers {
            if task_offers.is_empty() {
                return Err(anyhow!("No offers available for task: {}", task_cid));
            }
            
            // Simple strategy: select the offer with the lowest cost
            let selected = task_offers
                .iter()
                .min_by_key(|o| o.cost_estimate)
                .cloned()
                .ok_or_else(|| anyhow!("Failed to select worker for task: {}", task_cid))?;
                
            // Update task status
            {
                let mut status = self.task_status.lock().unwrap();
                status.insert(task_cid.clone(), TaskStatus::Assigned);
            }
            
            info!("Selected worker {} for task {}", selected.worker_did, task_cid);
            Ok(selected)
        } else {
            Err(anyhow!("No offers available for task: {}", task_cid))
        }
    }
    
    /// Execute a WASM task locally
    pub async fn execute_task_locally(&self, task_cid: &Cid) -> Result<ExecutionReceipt> {
        // Get the task details
        let task = {
            let tasks = self.tasks.lock().unwrap();
            tasks.get(task_cid)
                .cloned()
                .ok_or_else(|| anyhow!("Task not found: {}", task_cid))?
        };
        
        // Update task status
        {
            let mut status = self.task_status.lock().unwrap();
            status.insert(task_cid.clone(), TaskStatus::InProgress);
        }
        
        // In a real implementation, this would:
        // 1. Fetch the WASM module from the WASM_DIR or IPFS
        // 2. Fetch the input data from the DATA_DIR or IPFS
        // 3. Set up a WASM runtime (wasmer, wasmtime, etc.)
        // 4. Execute the WASM module with the input data
        // 5. Measure execution metrics (time, memory, etc.)
        // 6. Store the output data
        // 7. Create and sign an execution receipt
        
        // For now, we'll return a mock execution receipt
        let output_cid = Cid::default(); // In reality, this would be the CID of the output data
        
        let receipt = ExecutionReceipt {
            worker_did: "did:icn:mesh:local".to_string(),
            task_cid: task_cid.clone(),
            output_cid,
            fuel_consumed: 1000, // Mock value
            timestamp: Utc::now(),
            signature: vec![], // In reality, this would be a cryptographic signature
            metadata: None,
        };
        
        // Update task status
        {
            let mut status = self.task_status.lock().unwrap();
            status.insert(task_cid.clone(), TaskStatus::Completed);
            
            let mut receipts = self.receipts.lock().unwrap();
            receipts.insert(task_cid.clone(), receipt.clone());
        }
        
        info!("Executed task locally: {}", task_cid);
        Ok(receipt)
    }
    
    /// Verify an execution receipt
    pub async fn verify_execution(&self, receipt_cid: &Cid) -> Result<VerificationReceipt> {
        // In a real implementation, this would:
        // 1. Fetch the task and execution receipt
        // 2. Fetch the WASM module and input data
        // 3. Re-execute the WASM module with the input data
        // 4. Compare the output with the claimed output
        // 5. Create and sign a verification receipt
        
        // For now, we'll return a mock verification receipt with a positive verdict
        let verdict = true; // Mock value - in reality, this would be based on verification
        
        let verification = VerificationReceipt {
            verifier_did: "did:icn:mesh:verifier".to_string(),
            receipt_cid: receipt_cid.clone(),
            verdict,
            proof_cid: Cid::default(), // In reality, this would be the CID of the proof data
            timestamp: Utc::now(),
            signature: vec![], // In reality, this would be a cryptographic signature
            metadata: None,
        };
        
        info!("Verified execution receipt: {}", receipt_cid);
        Ok(verification)
    }
    
    /// Get the status of a task
    pub fn get_task_status(&self, task_cid: &Cid) -> Option<TaskStatus> {
        self.task_status.lock().unwrap().get(task_cid).cloned()
    }
    
    /// Get all tasks with their status
    pub fn get_all_tasks(&self) -> HashMap<Cid, (TaskIntent, TaskStatus)> {
        let tasks = self.tasks.lock().unwrap();
        let status = self.task_status.lock().unwrap();
        
        tasks.iter()
            .filter_map(|(cid, task)| {
                status.get(cid).map(|s| (cid.clone(), (task.clone(), s.clone())))
            })
            .collect()
    }
}

/// Convert a task intent to a CID
fn task_to_cid(task: &TaskIntent) -> Result<Cid> {
    // In a real implementation, this would create a CID based on the task content
    // For now, we'll just return the WASM CID as a placeholder
    Ok(task.wasm_cid.clone())
}

/// Convert an execution receipt to a CID
fn execution_to_cid(receipt: &ExecutionReceipt) -> Result<Cid> {
    // In a real implementation, this would create a CID based on the receipt content
    // For now, we'll just return the output CID as a placeholder
    Ok(receipt.output_cid.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    
    #[tokio::test]
    async fn test_task_lifecycle() {
        let (sender, _) = mpsc::channel(100);
        let wasm_dir = PathBuf::from("/tmp/wasm");
        let data_dir = PathBuf::from("/tmp/data");
        
        let engine = MeshExecutionEngine::new(sender, wasm_dir, data_dir);
        
        // Create a mock task
        let task = TaskIntent {
            publisher_did: "did:icn:test:publisher".to_string(),
            wasm_cid: Cid::default(),
            input_cid: Cid::default(),
            fee: 100,
            verifiers: 3,
            expiry: Utc::now() + chrono::Duration::hours(24),
            metadata: None,
        };
        
        let task_cid = task_to_cid(&task).unwrap();
        
        // Process the task
        engine.process_task(task.clone()).await.unwrap();
        
        // Check task status
        let status = engine.get_task_status(&task_cid).unwrap();
        assert_eq!(status, TaskStatus::Published);
        
        // Create a mock offer
        let offer = ComputeOffer {
            worker_did: "did:icn:test:worker".to_string(),
            task_cid: task_cid.clone(),
            cost_estimate: 80,
            available_capacity: 100,
            estimated_time_ms: 5000,
            timestamp: Utc::now(),
            signature: vec![],
        };
        
        // Process the offer
        engine.process_offer(offer.clone()).await.unwrap();
        
        // Check task status
        let status = engine.get_task_status(&task_cid).unwrap();
        assert_eq!(status, TaskStatus::OffersReceived);
        
        // Select a worker
        let selected = engine.select_worker(&task_cid).await.unwrap();
        
        // Check task status
        let status = engine.get_task_status(&task_cid).unwrap();
        assert_eq!(status, TaskStatus::Assigned);
        
        // Execute the task locally
        let receipt = engine.execute_task_locally(&task_cid).await.unwrap();
        
        // Check task status
        let status = engine.get_task_status(&task_cid).unwrap();
        assert_eq!(status, TaskStatus::Completed);
        
        // Verify the execution
        let receipt_cid = execution_to_cid(&receipt).unwrap();
        let verification = engine.verify_execution(&receipt_cid).await.unwrap();
        
        // In a real test, we would add the verification to the engine
        // and check that the task status transitions to Verified
    }
} 