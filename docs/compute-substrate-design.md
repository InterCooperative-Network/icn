# Compute Substrate Design

**Date**: 2025-11-18
**Status**: Design Document
**Phase**: Pre-implementation (C0 ready to start)

---

## 1. Overview

### 1.1 Vision

ICN's compute substrate transforms distributed computing resources into a **shared cooperative asset**. Nodes contribute CPU, memory, storage, and GPU capacity; the network tracks contributions, schedules workloads, meters usage, and settles payments in mutual credit.

This is **not** another cloud provider or blockchain compute market. It's:
- **Cooperative-owned**: Resources belong to participating coops, not a platform
- **Trust-integrated**: Placement and pricing depend on trust relationships
- **Governance-driven**: Policies set by democratic decision-making
- **Economically aligned**: Contributors earn credit, consumers pay credit

### 1.2 Design Principles

1. **ICN coordinates, doesn't orchestrate**: We use existing tools (K8s, Nomad, Proxmox) for actual workload execution
2. **Trust gates everything**: Untrusted nodes can't access sensitive jobs
3. **Metering is transparent**: All resource consumption is cryptographically attested
4. **Governance controls policy**: Pricing, pools, and access are decided democratically
5. **Progressive complexity**: Start simple (C0), add sophistication incrementally

### 1.3 Non-Goals

- Replacing Kubernetes/Nomad/Slurm (we integrate with them)
- Real-time or latency-critical scheduling (batch-oriented first)
- Verifiable computation proofs (trust + spot-checks, not ZK proofs)
- Token-based payment (mutual credit only)

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      ICN Compute Layer                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │   Capacity   │  │     Job      │  │   Metering   │       │
│  │   Registry   │  │  Scheduler   │  │   & Billing  │       │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘       │
│         │                 │                 │                │
│         ▼                 ▼                 ▼                │
│  ┌─────────────────────────────────────────────────┐        │
│  │              Gossip Topics                       │        │
│  │  compute:nodes | compute:jobs | compute:usage    │        │
│  └─────────────────────────────────────────────────┘        │
│                          │                                   │
├──────────────────────────┼───────────────────────────────────┤
│                          │                                   │
│  ┌───────────────────────▼───────────────────────┐          │
│  │            Gateway API (/v1/compute/*)         │          │
│  └───────────────────────────────────────────────┘          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    Compute Nodes                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ ComputeAgent │  │ ComputeAgent │  │ ComputeAgent │       │
│  │   (Node A)   │  │   (Node B)   │  │   (Node C)   │       │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘       │
│         │                 │                 │                │
│         ▼                 ▼                 ▼                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │  K8s/Nomad/  │  │  K8s/Nomad/  │  │  K8s/Nomad/  │       │
│  │   Proxmox    │  │   Proxmox    │  │   Proxmox    │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Data Structures

### 3.1 Compute Profile

Advertises a node's available resources and capabilities.

```rust
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Compute capabilities a node can provide
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ComputeCapability {
    /// Can run OCI containers (Docker/Podman)
    Containers,
    /// Can run WebAssembly modules
    Wasm,
    /// Has GPU available
    Gpu,
    /// Supports Intel SGX enclaves
    Sgx,
    /// Supports AMD SEV
    Sev,
    /// Has high-bandwidth network (>1Gbps)
    HighBandwidth,
    /// Has SSD storage
    SsdStorage,
    /// Custom capability (for future extensibility)
    Custom(String),
}

/// GPU information if available
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProfile {
    /// GPU model (e.g., "NVIDIA RTX 4090")
    pub model: String,
    /// VRAM in GB
    pub vram_gb: u32,
    /// CUDA compute capability (e.g., "8.9")
    pub compute_capability: Option<String>,
}

/// A node's advertised compute capacity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeProfile {
    /// Node's DID
    pub node_did: Did,

    /// CPU cores available for compute jobs
    pub cpu_cores: u32,

    /// RAM available in GB
    pub ram_gb: u32,

    /// Disk space available in GB
    pub disk_gb: u32,

    /// GPU if available
    pub gpu: Option<GpuProfile>,

    /// Supported capabilities
    pub capabilities: Vec<ComputeCapability>,

    /// Network bandwidth in Mbps
    pub network_mbps: u32,

    /// Geographic region (e.g., "us-east", "eu-west")
    pub region: String,

    /// Legal jurisdiction for data sovereignty
    pub jurisdiction: String,

    /// Average availability (0.0-1.0, e.g., 0.99 = 99% uptime)
    pub availability: f32,

    /// Maximum concurrent jobs this node will accept
    pub max_concurrent_jobs: u32,

    /// Unix timestamp of last update
    pub updated_at: u64,

    /// Signature over profile by node's DID
    pub signature: Vec<u8>,
}

impl ComputeProfile {
    /// Check if node has all required capabilities
    pub fn has_capabilities(&self, required: &[ComputeCapability]) -> bool {
        required.iter().all(|cap| self.capabilities.contains(cap))
    }

    /// Check if node can satisfy resource request
    pub fn can_satisfy(&self, request: &ResourceRequest) -> bool {
        self.cpu_cores >= request.cpu_cores
            && self.ram_gb >= request.ram_gb
            && self.disk_gb >= request.disk_gb.unwrap_or(0)
            && (request.gpu.is_none() || self.gpu.is_some())
    }
}
```

### 3.2 Job Specification

Defines a compute job to be scheduled and executed.

```rust
use std::collections::HashMap;

/// Unique job identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct JobId(pub String);

/// Reference to a container image or WASM module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageRef {
    /// OCI container image (e.g., "ghcr.io/coop/app:v1.0")
    Container(String),
    /// WASM module by content hash
    Wasm { hash: String, size_bytes: u64 },
    /// WASM module by URL
    WasmUrl(String),
}

/// Resource requirements for a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequest {
    /// Minimum CPU cores required
    pub cpu_cores: u32,
    /// Minimum RAM in GB
    pub ram_gb: u32,
    /// Minimum disk space in GB (optional)
    pub disk_gb: Option<u32>,
    /// Requires GPU
    pub gpu: Option<GpuRequest>,
}

/// GPU requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuRequest {
    /// Minimum VRAM in GB
    pub min_vram_gb: u32,
    /// Required compute capability (optional)
    pub min_compute_capability: Option<String>,
}

/// Constraints on where a job can be placed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementConstraints {
    /// Allowed regions (empty = any)
    pub regions: Vec<String>,
    /// Allowed jurisdictions for data sovereignty (empty = any)
    pub jurisdictions: Vec<String>,
    /// Minimum trust score required (0.0-1.0)
    pub min_trust: f32,
    /// Required capabilities
    pub capabilities: Vec<ComputeCapability>,
    /// Preferred nodes (DIDs) - scheduler will try these first
    pub preferred_nodes: Vec<Did>,
    /// Excluded nodes (DIDs) - scheduler will never use these
    pub excluded_nodes: Vec<Did>,
}

/// Reference to input data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataRef {
    /// ICN ledger entry
    LedgerEntry { entry_id: String },
    /// ICN gossip entry
    GossipEntry { topic: String, hash: String },
    /// External URL
    Url(String),
    /// Inline data (for small inputs)
    Inline(Vec<u8>),
}

/// Job priority level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// A compute job to be scheduled
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeJob {
    /// Unique job ID
    pub id: JobId,

    /// Job owner's DID
    pub owner: Did,

    /// Cooperative this job belongs to
    pub coop_id: String,

    /// Container image or WASM module to run
    pub image: ImageRef,

    /// Resource requirements
    pub resources: ResourceRequest,

    /// Placement constraints
    pub constraints: PlacementConstraints,

    /// Input data references
    pub inputs: Vec<DataRef>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Command to run (overrides image entrypoint)
    pub command: Option<Vec<String>>,

    /// Maximum runtime in seconds (0 = no limit)
    pub timeout_secs: u64,

    /// Job priority
    pub priority: JobPriority,

    /// CCL contract for settlement (optional - uses default if not specified)
    pub settlement_contract: Option<String>,

    /// Unix timestamp when job was submitted
    pub submitted_at: u64,

    /// Signature over job spec by owner
    pub signature: Vec<u8>,
}

/// Current state of a job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobState {
    /// Job submitted, waiting for scheduling
    Pending,
    /// Job assigned to a node, waiting to start
    Scheduled { node: Did },
    /// Job is currently running
    Running { node: Did, started_at: u64 },
    /// Job completed successfully
    Completed { node: Did, completed_at: u64 },
    /// Job failed
    Failed { node: Did, error: String, failed_at: u64 },
    /// Job was cancelled
    Cancelled { cancelled_at: u64 },
    /// Job timed out
    TimedOut { node: Did, timed_out_at: u64 },
}

/// Job with current state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub job: ComputeJob,
    pub state: JobState,
    /// Output data references (populated on completion)
    pub outputs: Vec<DataRef>,
}
```

### 3.3 Usage Report

Metering data from job execution.

```rust
/// Resource usage report from a completed job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeUsageReport {
    /// Job that was executed
    pub job_id: JobId,

    /// Node that executed the job
    pub executor: Did,

    /// CPU time consumed in seconds
    pub cpu_seconds: u64,

    /// Peak memory usage in MB
    pub memory_peak_mb: u64,

    /// Average memory usage in MB
    pub memory_avg_mb: u64,

    /// Total disk read in bytes
    pub disk_read_bytes: u64,

    /// Total disk write in bytes
    pub disk_write_bytes: u64,

    /// Total network ingress in bytes
    pub network_in_bytes: u64,

    /// Total network egress in bytes
    pub network_out_bytes: u64,

    /// GPU time in seconds (if applicable)
    pub gpu_seconds: Option<u64>,

    /// Wall-clock duration in milliseconds
    pub duration_ms: u64,

    /// Exit code (0 = success)
    pub exit_code: i32,

    /// Unix timestamp when report was generated
    pub reported_at: u64,

    /// Signature over report by executor
    pub signature: Vec<u8>,
}

impl ComputeUsageReport {
    /// Calculate memory GB-seconds for billing
    pub fn memory_gb_seconds(&self) -> f64 {
        (self.memory_avg_mb as f64 / 1024.0) * (self.duration_ms as f64 / 1000.0)
    }

    /// Calculate total IO in GB
    pub fn total_io_gb(&self) -> f64 {
        let total_bytes = self.disk_read_bytes
            + self.disk_write_bytes
            + self.network_in_bytes
            + self.network_out_bytes;
        total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}
```

### 3.4 Gossip Messages

Messages for the compute gossip topics.

```rust
/// Messages for compute:nodes topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeNodeMessage {
    /// Node advertising/updating its profile
    ProfileUpdate(ComputeProfile),
    /// Node going offline
    GoingOffline { node: Did, until: Option<u64> },
    /// Node back online
    BackOnline { node: Did },
}

/// Messages for compute:jobs topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeJobMessage {
    /// New job submitted
    JobSubmitted(ComputeJob),
    /// Job assigned to node
    JobScheduled { job_id: JobId, node: Did },
    /// Job started running
    JobStarted { job_id: JobId, node: Did },
    /// Job completed
    JobCompleted { job_id: JobId, node: Did, outputs: Vec<DataRef> },
    /// Job failed
    JobFailed { job_id: JobId, node: Did, error: String },
    /// Job cancelled
    JobCancelled { job_id: JobId },
}

/// Messages for compute:usage topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeUsageMessage {
    /// Usage report from completed job
    UsageReport(ComputeUsageReport),
    /// Verification result (for spot-checked jobs)
    VerificationResult {
        job_id: JobId,
        primary_node: Did,
        verifier_node: Did,
        matches: bool,
        discrepancy: Option<String>,
    },
}
```

---

## 4. Gateway API

### 4.1 Endpoints

```
# Node capacity management
GET    /v1/compute/nodes              # List all compute nodes
GET    /v1/compute/nodes/:did         # Get specific node profile
POST   /v1/compute/nodes              # Register/update own profile
DELETE /v1/compute/nodes              # Unregister own node

# Job management
POST   /v1/compute/jobs               # Submit a new job
GET    /v1/compute/jobs               # List jobs (filterable)
GET    /v1/compute/jobs/:id           # Get job status
DELETE /v1/compute/jobs/:id           # Cancel a job

# Usage and billing
GET    /v1/compute/usage              # Get usage reports (filterable)
GET    /v1/compute/usage/:job_id      # Get usage for specific job
GET    /v1/compute/billing/summary    # Get billing summary for coop

# Pool management (Phase C3)
GET    /v1/compute/pools              # List compute pools
GET    /v1/compute/pools/:id          # Get pool details
POST   /v1/compute/pools              # Create pool (requires gov approval)
PUT    /v1/compute/pools/:id          # Update pool settings
```

### 4.2 Request/Response Types

```rust
/// Request to register/update compute profile
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterNodeRequest {
    pub cpu_cores: u32,
    pub ram_gb: u32,
    pub disk_gb: u32,
    pub gpu: Option<GpuProfile>,
    pub capabilities: Vec<ComputeCapability>,
    pub network_mbps: u32,
    pub region: String,
    pub jurisdiction: String,
    pub availability: f32,
    pub max_concurrent_jobs: u32,
}

/// Request to submit a job
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitJobRequest {
    pub image: ImageRef,
    pub resources: ResourceRequest,
    pub constraints: PlacementConstraints,
    pub inputs: Vec<DataRef>,
    pub env: HashMap<String, String>,
    pub command: Option<Vec<String>>,
    pub timeout_secs: u64,
    pub priority: JobPriority,
    pub settlement_contract: Option<String>,
}

/// Response with job ID
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitJobResponse {
    pub job_id: JobId,
    pub estimated_start: Option<u64>,
}

/// Query parameters for listing jobs
#[derive(Debug, Serialize, Deserialize)]
pub struct ListJobsQuery {
    pub state: Option<String>,       // pending, running, completed, failed
    pub owner: Option<String>,       // filter by owner DID
    pub coop_id: Option<String>,     // filter by coop
    pub since: Option<u64>,          // jobs submitted after timestamp
    pub limit: Option<u32>,          // max results
    pub offset: Option<u32>,         // pagination offset
}

/// Billing summary for a coop
#[derive(Debug, Serialize, Deserialize)]
pub struct BillingSummary {
    pub coop_id: String,
    pub period_start: u64,
    pub period_end: u64,
    pub total_jobs: u64,
    pub total_cpu_seconds: u64,
    pub total_memory_gb_seconds: f64,
    pub total_io_gb: f64,
    pub total_cost: f64,           // in mutual credit units
    pub breakdown_by_node: HashMap<String, NodeBilling>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeBilling {
    pub node_did: String,
    pub jobs_executed: u64,
    pub cpu_seconds: u64,
    pub memory_gb_seconds: f64,
    pub cost: f64,
}
```

---

## 5. Scheduling

### 5.1 Placement Algorithm

```rust
/// Scheduler state
pub struct Scheduler {
    /// Known compute nodes
    nodes: HashMap<Did, ComputeProfile>,
    /// Trust graph for trust lookups
    trust_graph: Arc<RwLock<TrustGraph>>,
    /// Pending jobs queue
    pending: VecDeque<ComputeJob>,
    /// Running jobs by node
    running: HashMap<Did, Vec<JobId>>,
}

impl Scheduler {
    /// Find best node for a job
    pub fn find_placement(&self, job: &ComputeJob) -> Option<Did> {
        let candidates: Vec<_> = self.nodes
            .iter()
            .filter(|(did, profile)| {
                // Check basic capacity
                if !profile.can_satisfy(&job.resources) {
                    return false;
                }

                // Check capabilities
                if !profile.has_capabilities(&job.constraints.capabilities) {
                    return false;
                }

                // Check region constraint
                if !job.constraints.regions.is_empty()
                    && !job.constraints.regions.contains(&profile.region) {
                    return false;
                }

                // Check jurisdiction constraint
                if !job.constraints.jurisdictions.is_empty()
                    && !job.constraints.jurisdictions.contains(&profile.jurisdiction) {
                    return false;
                }

                // Check exclusion list
                if job.constraints.excluded_nodes.contains(did) {
                    return false;
                }

                // Check concurrent job limit
                let running_count = self.running
                    .get(did)
                    .map(|jobs| jobs.len() as u32)
                    .unwrap_or(0);
                if running_count >= profile.max_concurrent_jobs {
                    return false;
                }

                true
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Score candidates
        let mut scored: Vec<_> = candidates
            .into_iter()
            .map(|(did, profile)| {
                let score = self.score_candidate(did, profile, job);
                (did.clone(), score)
            })
            .collect();

        // Sort by score (highest first)
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Some(scored[0].0.clone())
    }

    /// Score a candidate node for a job
    fn score_candidate(&self, did: &Did, profile: &ComputeProfile, job: &ComputeJob) -> f64 {
        let mut score = 0.0;

        // Trust score (0-1) weighted heavily
        let trust = self.trust_graph
            .read()
            .unwrap()
            .get_trust_score(&job.owner, did)
            .unwrap_or(0.0);

        if trust < job.constraints.min_trust {
            return 0.0; // Below minimum trust threshold
        }
        score += trust * 40.0;

        // Availability score
        score += profile.availability * 20.0;

        // Prefer nodes with fewer running jobs (load balancing)
        let running_count = self.running
            .get(did)
            .map(|jobs| jobs.len())
            .unwrap_or(0);
        let load_factor = 1.0 - (running_count as f64 / profile.max_concurrent_jobs as f64);
        score += load_factor * 20.0;

        // Bonus for preferred nodes
        if job.constraints.preferred_nodes.contains(did) {
            score += 15.0;
        }

        // Slight preference for closer match to requested resources (efficiency)
        let cpu_efficiency = job.resources.cpu_cores as f64 / profile.cpu_cores as f64;
        let ram_efficiency = job.resources.ram_gb as f64 / profile.ram_gb as f64;
        let efficiency = (cpu_efficiency + ram_efficiency) / 2.0;
        score += efficiency * 5.0;

        score
    }
}
```

### 5.2 Fair Scheduling (Phase C3)

For cooperatives with governance policies around fair resource allocation:

```rust
/// Fair scheduling policy
pub struct FairSchedulingPolicy {
    /// Minimum share per coop (0.0-1.0)
    pub min_share: HashMap<String, f64>,
    /// Maximum share per coop (0.0-1.0)
    pub max_share: HashMap<String, f64>,
    /// Priority boost for underutilized coops
    pub fairness_weight: f64,
}

impl Scheduler {
    /// Adjust job priority based on fairness
    fn apply_fair_scheduling(&self, job: &mut ComputeJob, policy: &FairSchedulingPolicy) {
        // Calculate current utilization per coop
        let utilization = self.calculate_coop_utilization();

        let coop_util = utilization.get(&job.coop_id).unwrap_or(&0.0);
        let min_share = policy.min_share.get(&job.coop_id).unwrap_or(&0.0);

        // Boost priority if coop is below fair share
        if coop_util < min_share {
            let boost = (min_share - coop_util) * policy.fairness_weight;
            // Increase effective priority
            // (implementation detail: could use a priority queue with adjusted scores)
        }
    }
}
```

---

## 6. Compute Agent

The ComputeAgent runs alongside `icnd` on each compute node.

### 6.1 Architecture

```rust
/// Compute agent for executing jobs
pub struct ComputeAgent {
    /// Node's DID
    node_did: Did,
    /// Node's keypair for signing
    keypair: KeyPair,
    /// Connection to ICN gateway
    gateway_client: GatewayClient,
    /// Execution backend
    executor: Box<dyn JobExecutor>,
    /// Currently running jobs
    running_jobs: HashMap<JobId, RunningJob>,
    /// Usage collector
    usage_collector: UsageCollector,
}

/// Trait for job execution backends
#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// Start a job
    async fn start(&self, job: &ComputeJob) -> Result<ExecutionHandle, ExecutorError>;
    /// Check job status
    async fn status(&self, handle: &ExecutionHandle) -> Result<ExecutionStatus, ExecutorError>;
    /// Stop a job
    async fn stop(&self, handle: &ExecutionHandle) -> Result<(), ExecutorError>;
    /// Get job logs
    async fn logs(&self, handle: &ExecutionHandle) -> Result<String, ExecutorError>;
}

/// Docker/Podman executor
pub struct ContainerExecutor {
    runtime: String, // "docker" or "podman"
}

/// Kubernetes executor
pub struct KubernetesExecutor {
    kubeconfig: PathBuf,
    namespace: String,
}

/// Nomad executor
pub struct NomadExecutor {
    nomad_addr: String,
}

/// WASM executor
pub struct WasmExecutor {
    runtime: WasmRuntime,
}
```

### 6.2 Main Loop

```rust
impl ComputeAgent {
    pub async fn run(&mut self, shutdown: broadcast::Receiver<()>) -> Result<()> {
        let mut shutdown = shutdown;

        loop {
            tokio::select! {
                // Check for new job assignments
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    self.poll_for_jobs().await?;
                    self.check_running_jobs().await?;
                }

                // Handle shutdown
                _ = shutdown.recv() => {
                    self.graceful_shutdown().await?;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn poll_for_jobs(&mut self) -> Result<()> {
        // Query gateway for jobs assigned to this node
        let jobs = self.gateway_client
            .list_jobs(ListJobsQuery {
                state: Some("scheduled".to_string()),
                // Filter for jobs assigned to this node
                ..Default::default()
            })
            .await?;

        for job_status in jobs {
            if let JobState::Scheduled { node } = &job_status.state {
                if node == &self.node_did {
                    self.start_job(job_status.job).await?;
                }
            }
        }

        Ok(())
    }

    async fn start_job(&mut self, job: ComputeJob) -> Result<()> {
        let handle = self.executor.start(&job).await?;

        let running_job = RunningJob {
            job_id: job.id.clone(),
            handle,
            started_at: now_unix(),
            usage_tracker: UsageTracker::new(),
        };

        self.running_jobs.insert(job.id.clone(), running_job);

        // Notify ICN that job started
        self.gateway_client
            .update_job_state(&job.id, JobState::Running {
                node: self.node_did.clone(),
                started_at: now_unix(),
            })
            .await?;

        Ok(())
    }

    async fn check_running_jobs(&mut self) -> Result<()> {
        let mut completed = Vec::new();

        for (job_id, running_job) in &mut self.running_jobs {
            let status = self.executor.status(&running_job.handle).await?;

            match status {
                ExecutionStatus::Running { cpu, memory } => {
                    running_job.usage_tracker.record(cpu, memory);
                }
                ExecutionStatus::Completed { exit_code, outputs } => {
                    let report = self.generate_usage_report(job_id, &running_job, exit_code);
                    self.submit_usage_report(report).await?;

                    if exit_code == 0 {
                        self.gateway_client
                            .update_job_state(job_id, JobState::Completed {
                                node: self.node_did.clone(),
                                completed_at: now_unix(),
                            })
                            .await?;
                    } else {
                        self.gateway_client
                            .update_job_state(job_id, JobState::Failed {
                                node: self.node_did.clone(),
                                error: format!("Exit code: {}", exit_code),
                                failed_at: now_unix(),
                            })
                            .await?;
                    }

                    completed.push(job_id.clone());
                }
                ExecutionStatus::Failed { error } => {
                    self.gateway_client
                        .update_job_state(job_id, JobState::Failed {
                            node: self.node_did.clone(),
                            error,
                            failed_at: now_unix(),
                        })
                        .await?;

                    completed.push(job_id.clone());
                }
            }
        }

        for job_id in completed {
            self.running_jobs.remove(&job_id);
        }

        Ok(())
    }

    fn generate_usage_report(
        &self,
        job_id: &JobId,
        running_job: &RunningJob,
        exit_code: i32,
    ) -> ComputeUsageReport {
        let usage = running_job.usage_tracker.finalize();

        ComputeUsageReport {
            job_id: job_id.clone(),
            executor: self.node_did.clone(),
            cpu_seconds: usage.cpu_seconds,
            memory_peak_mb: usage.memory_peak_mb,
            memory_avg_mb: usage.memory_avg_mb,
            disk_read_bytes: usage.disk_read_bytes,
            disk_write_bytes: usage.disk_write_bytes,
            network_in_bytes: usage.network_in_bytes,
            network_out_bytes: usage.network_out_bytes,
            gpu_seconds: usage.gpu_seconds,
            duration_ms: now_unix() - running_job.started_at,
            exit_code,
            reported_at: now_unix(),
            signature: vec![], // Sign with keypair
        }
    }
}
```

---

## 7. Billing & Settlement

### 7.1 Pricing Policy

```rust
/// Pricing policy for compute resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputePricingPolicy {
    /// Price per CPU-second
    pub cpu_second_price: f64,
    /// Price per GB-second of memory
    pub memory_gb_second_price: f64,
    /// Price per GB of IO
    pub io_gb_price: f64,
    /// Price per GPU-second
    pub gpu_second_price: f64,
    /// Discount for internal (same coop) jobs
    pub internal_discount: f64,
    /// Premium for high-priority jobs
    pub priority_premium: HashMap<JobPriority, f64>,
}

impl ComputePricingPolicy {
    /// Calculate cost for a usage report
    pub fn calculate_cost(&self, report: &ComputeUsageReport, is_internal: bool) -> f64 {
        let mut cost = 0.0;

        // CPU cost
        cost += report.cpu_seconds as f64 * self.cpu_second_price;

        // Memory cost
        cost += report.memory_gb_seconds() * self.memory_gb_second_price;

        // IO cost
        cost += report.total_io_gb() * self.io_gb_price;

        // GPU cost
        if let Some(gpu_secs) = report.gpu_seconds {
            cost += gpu_secs as f64 * self.gpu_second_price;
        }

        // Apply internal discount
        if is_internal {
            cost *= 1.0 - self.internal_discount;
        }

        cost
    }
}
```

### 7.2 CCL Settlement Contract

Example CCL contract for compute billing:

```rust
// Pseudocode for CCL contract structure
contract ComputeBilling {
    // Pricing policy stored in contract state
    state pricing: ComputePricingPolicy;

    // Process a usage report and create ledger entry
    rule settle_usage(report: ComputeUsageReport) {
        // Verify report signature
        require(verify_signature(report.executor, report.signature));

        // Calculate cost
        let cost = pricing.calculate_cost(report);

        // Get job details
        let job = lookup_job(report.job_id);

        // Create ledger entry
        // Debit job owner, credit executor
        ledger.transfer(
            from: job.owner,
            to: report.executor,
            amount: cost,
            currency: "COMPUTE",
            memo: format!("Job {}", report.job_id),
        );

        // Emit event
        emit ComputeSettled {
            job_id: report.job_id,
            cost: cost,
            executor: report.executor,
        };
    }
}
```

---

## 8. Verification & Anti-Cheating

### 8.1 Spot Check System

```rust
/// Configuration for spot checks
pub struct SpotCheckConfig {
    /// Percentage of jobs to spot check (0.0-1.0)
    pub check_rate: f64,
    /// Tolerance for resource usage discrepancy (e.g., 0.1 = 10%)
    pub tolerance: f64,
    /// Trust penalty for failed verification
    pub trust_penalty: f64,
}

impl Scheduler {
    /// Decide if a job should be spot-checked
    fn should_spot_check(&self, job: &ComputeJob) -> bool {
        // Higher check rate for:
        // - Low trust executors
        // - High-value jobs
        // - Jobs with sensitive constraints

        let base_rate = self.spot_check_config.check_rate;

        // Random selection
        rand::random::<f64>() < base_rate
    }

    /// Schedule a spot check job
    async fn schedule_spot_check(&self, original_job: &ComputeJob, original_executor: &Did) {
        // Create verification job
        let verify_job = ComputeJob {
            id: JobId(format!("verify-{}", original_job.id.0)),
            constraints: PlacementConstraints {
                // Must run on different node
                excluded_nodes: vec![original_executor.clone()],
                ..original_job.constraints.clone()
            },
            ..original_job.clone()
        };

        // Submit verification job
        self.submit_job(verify_job).await;
    }

    /// Compare original and verification results
    async fn verify_results(
        &self,
        original: &ComputeUsageReport,
        verification: &ComputeUsageReport,
    ) -> VerificationResult {
        let tolerance = self.spot_check_config.tolerance;

        // Compare CPU usage
        let cpu_diff = (original.cpu_seconds as f64 - verification.cpu_seconds as f64).abs()
            / original.cpu_seconds as f64;

        // Compare memory usage
        let mem_diff = (original.memory_avg_mb as f64 - verification.memory_avg_mb as f64).abs()
            / original.memory_avg_mb as f64;

        if cpu_diff > tolerance || mem_diff > tolerance {
            VerificationResult::Failed {
                discrepancy: format!(
                    "CPU diff: {:.1}%, Memory diff: {:.1}%",
                    cpu_diff * 100.0,
                    mem_diff * 100.0
                ),
            }
        } else {
            VerificationResult::Passed
        }
    }
}
```

### 8.2 Trust Integration

```rust
impl TrustGraph {
    /// Apply penalty for failed verification
    pub fn apply_compute_penalty(&mut self, node: &Did, penalty: f64) {
        if let Some(edge) = self.get_edge_mut(node) {
            edge.score = (edge.score - penalty).max(0.0);
            edge.reason = Some(format!(
                "Compute verification failed at {}",
                chrono::Utc::now()
            ));
        }
    }

    /// Apply bonus for consistent good behavior
    pub fn apply_compute_bonus(&mut self, node: &Did, bonus: f64) {
        if let Some(edge) = self.get_edge_mut(node) {
            edge.score = (edge.score + bonus).min(1.0);
        }
    }
}
```

---

## 9. Governance Integration

### 9.1 Compute Pool Domain

```rust
/// Governance domain for compute policy
pub struct ComputePoolDomain {
    /// Domain ID (e.g., "coop:food:compute")
    pub domain_id: String,
    /// Nodes in this pool
    pub nodes: Vec<Did>,
    /// Pricing policy
    pub pricing: ComputePricingPolicy,
    /// Access policy
    pub access: PoolAccessPolicy,
    /// Resource allocation
    pub allocation: ResourceAllocation,
}

/// Who can use this compute pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PoolAccessPolicy {
    /// Only members of this coop
    Internal,
    /// Members and specific federations
    Federation { allowed: Vec<String> },
    /// Anyone with minimum trust
    Public { min_trust: f32 },
}

/// How resources are allocated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// Percentage reserved for internal use
    pub internal_reserve: f64,
    /// Percentage available to federation
    pub federation_share: f64,
    /// Percentage available to public
    pub public_share: f64,
}
```

### 9.2 Governance Proposals

Proposal types for compute governance:

```rust
/// Compute-related proposal payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeProposalPayload {
    /// Add node to compute pool
    AddNode { node: Did, pool_id: String },
    /// Remove node from pool
    RemoveNode { node: Did, pool_id: String },
    /// Update pricing policy
    UpdatePricing { pool_id: String, pricing: ComputePricingPolicy },
    /// Update access policy
    UpdateAccess { pool_id: String, access: PoolAccessPolicy },
    /// Update resource allocation
    UpdateAllocation { pool_id: String, allocation: ResourceAllocation },
    /// Create new compute pool
    CreatePool { pool: ComputePoolDomain },
}
```

---

## 10. CLI Commands

### 10.1 icnctl compute commands

```bash
# Node registration
icnctl compute register \
  --cpu 8 \
  --ram 32 \
  --disk 500 \
  --gpu "NVIDIA RTX 4090,24GB" \
  --capabilities containers,wasm,gpu \
  --region us-east \
  --jurisdiction US \
  --availability 0.99 \
  --max-jobs 4

# Update profile
icnctl compute update --cpu 16 --ram 64

# List nodes
icnctl compute nodes
icnctl compute nodes --region us-east --capability gpu

# Show node details
icnctl compute node show did:icn:abc123

# Unregister
icnctl compute unregister

# Job submission
icnctl compute job submit \
  --image ghcr.io/mycoop/processor:v1 \
  --cpu 2 \
  --ram 8 \
  --timeout 3600 \
  --region us-east \
  --min-trust 0.5 \
  --input "icn://ledger/entry/xyz"

# Job management
icnctl compute job list
icnctl compute job list --state running
icnctl compute job status job-123
icnctl compute job cancel job-123
icnctl compute job logs job-123

# Usage reports
icnctl compute usage --since 2025-01-01
icnctl compute usage --job job-123

# Billing
icnctl compute billing summary
icnctl compute billing summary --period 2025-01
```

---

## 11. Metrics

### 11.1 Prometheus Metrics

```rust
// Node metrics
icn_compute_nodes_total                    // Total registered compute nodes
icn_compute_nodes_online                   // Currently online nodes
icn_compute_capacity_cpu_cores_total       // Total CPU cores across all nodes
icn_compute_capacity_ram_gb_total          // Total RAM across all nodes
icn_compute_capacity_gpu_count             // Total GPUs

// Job metrics
icn_compute_jobs_submitted_total           // Total jobs submitted
icn_compute_jobs_completed_total           // Total jobs completed
icn_compute_jobs_failed_total              // Total jobs failed
icn_compute_jobs_running                   // Currently running jobs
icn_compute_jobs_pending                   // Jobs waiting to be scheduled
icn_compute_job_duration_seconds           // Histogram of job durations

// Usage metrics
icn_compute_cpu_seconds_total              // Total CPU seconds consumed
icn_compute_memory_gb_seconds_total        // Total memory GB-seconds
icn_compute_io_gb_total                    // Total IO in GB
icn_compute_gpu_seconds_total              // Total GPU seconds

// Billing metrics
icn_compute_credits_earned_total           // Credits earned by this node
icn_compute_credits_spent_total            // Credits spent by this coop

// Verification metrics
icn_compute_spot_checks_total              // Total spot checks performed
icn_compute_spot_checks_failed_total       // Failed spot checks
icn_compute_trust_penalties_total          // Trust penalties applied
```

---

## 12. Implementation Phases

### Phase C0: Representation (2-3 weeks)

**Goal**: Make compute capacity visible in ICN.

**Deliverables**:
- [ ] `ComputeProfile` and related types in new `icn-compute` crate
- [ ] `compute:nodes` gossip topic
- [ ] Gateway endpoints: GET/POST /v1/compute/nodes
- [ ] icnctl commands: register, update, list, show
- [ ] Unit tests for serialization
- [ ] Integration test for profile gossip

**Non-goals**:
- Job scheduling
- Metering
- Billing

### Phase C1: Off-Chain Integration (3-4 weeks)

**Goal**: Run real jobs with flat-rate billing.

**Deliverables**:
- [ ] External scheduler service (Rust or TS)
- [ ] `ComputeJob` spec and `compute:jobs` topic
- [ ] Gateway endpoints for job submission
- [ ] Basic placement algorithm
- [ ] Flat-rate ledger entries for completed jobs
- [ ] icnctl job commands
- [ ] Console: nodes list, jobs table

**Non-goals**:
- Real metering
- Verification

### Phase C2: Metering & Settlement (4-5 weeks)

**Goal**: Meter actual usage and bill accurately.

**Deliverables**:
- [ ] `ComputeUsageReport` and `compute:usage` topic
- [ ] ComputeAgent v1 with container executor
- [ ] Usage collection and reporting
- [ ] CCL billing contract
- [ ] Spot check system
- [ ] Trust integration for penalties
- [ ] Billing summary endpoint

### Phase C3: Pools & Governance (4-5 weeks)

**Goal**: Democratically governed compute resources.

**Deliverables**:
- [ ] `ComputePoolDomain` types
- [ ] Governance proposals for compute
- [ ] Pool-aware scheduling
- [ ] Access policies and allocation
- [ ] Federation integration
- [ ] Console: Compute tab with full UX
- [ ] Fair scheduling policies

---

## 13. Open Questions

1. **Determinism**: How do we handle non-deterministic workloads for verification?
2. **Data transfer**: How do large inputs/outputs get to/from nodes?
3. **Secrets**: How do jobs access credentials securely?
4. **Preemption**: Can high-priority jobs preempt running jobs?
5. **Quotas**: Hard limits per coop beyond soft fair-share?
6. **Multi-tenant**: Multiple coops sharing a single physical node?

---

## 14. References

- [Gap Analysis](gap-analysis.md) - Section 7 (Compute Substrate Gaps)
- [Economic Safety](economic-safety.md) - Credit limits and billing integration
- [Governance](governance.md) - Proposal patterns for compute policies
- [CLAUDE.md](../CLAUDE.md) - Actor patterns and integration points

---

*Document created: 2025-11-18*
*Status: Design complete, ready for C0 implementation*
