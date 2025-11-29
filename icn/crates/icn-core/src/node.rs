//! Node Morphogenesis - Environment-adaptive node profiles
//!
//! This module implements ICN's "stem cell" model for nodes: devices start
//! undifferentiated and acquire roles based on hardware capabilities, operator
//! policy, and network needs.
//!
//! # Key Concepts
//!
//! - **Principal DID**: User/organization identity (long-lived)
//! - **Node DID**: Device identity for network operations (hardware lifecycle)
//! - **ServiceRole**: Unified capabilities (network + compute + storage)
//! - **NodeStage**: Lifecycle stage (Sensing → Active → Retiring)
//!
//! # Design Philosophy
//!
//! Roles are *acquired* based on capability, not *assigned* by category.
//! A phone might acquire `CclExecutor` if it has enough RAM. A server might
//! lack `Archive` if its trust score is low.

use icn_compute::NodeCapacity;
use icn_identity::Did;
use icn_net::TopologyInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Flexible Capability System
// ============================================================================

/// A capability value that can represent different types of hardware/software features.
///
/// This enables nodes to advertise arbitrary capabilities that can be discovered
/// and matched by the scheduler, replication manager, or applications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CapabilityValue {
    /// Binary capability (present or not)
    Present(bool),
    /// Numeric capability with optional unit
    Numeric { value: f64, unit: Option<String> },
    /// String capability (version, model name, etc.)
    Text(String),
    /// Structured capability (for complex hardware descriptions)
    Structured(serde_json::Value),
}

impl CapabilityValue {
    pub fn present() -> Self {
        Self::Present(true)
    }

    pub fn numeric(value: f64) -> Self {
        Self::Numeric { value, unit: None }
    }

    pub fn numeric_with_unit(value: f64, unit: &str) -> Self {
        Self::Numeric {
            value,
            unit: Some(unit.to_string()),
        }
    }

    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// Check if this capability meets a minimum numeric threshold
    pub fn meets_threshold(&self, min: f64) -> bool {
        match self {
            Self::Numeric { value, .. } => *value >= min,
            Self::Present(true) => true,
            _ => false,
        }
    }

    /// Get numeric value if present
    pub fn as_numeric(&self) -> Option<f64> {
        match self {
            Self::Numeric { value, .. } => Some(*value),
            _ => None,
        }
    }
}

/// Standard capability keys for common hardware/sensors.
///
/// Using constants ensures consistent naming across the network.
/// Custom capabilities can use any string key.
pub mod capability_keys {
    // GPU compute capabilities
    pub const GPU_AVAILABLE: &str = "hw.gpu.available";
    pub const GPU_MEMORY_GB: &str = "hw.gpu.memory_gb";
    pub const GPU_COMPUTE_CAPABILITY: &str = "hw.gpu.compute_capability";
    pub const GPU_MODEL: &str = "hw.gpu.model";
    pub const GPU_CUDA_CORES: &str = "hw.gpu.cuda_cores";
    pub const GPU_TENSOR_CORES: &str = "hw.gpu.tensor_cores";
    pub const GPU_VENDOR: &str = "hw.gpu.vendor"; // nvidia, amd, intel
    pub const GPU_DRIVER_VERSION: &str = "hw.gpu.driver_version";
    pub const GPU_COMPUTE_OFFERED: &str = "hw.gpu.compute_offered"; // willing to run GPU tasks

    // Other hardware accelerators
    pub const TPU_AVAILABLE: &str = "hw.tpu.available";
    pub const FPGA_AVAILABLE: &str = "hw.fpga.available";
    pub const NPU_AVAILABLE: &str = "hw.npu.available"; // Neural processing unit (Apple, Qualcomm)

    // Sensors
    pub const SENSOR_GPS: &str = "sensor.gps";
    pub const SENSOR_TEMPERATURE: &str = "sensor.temperature";
    pub const SENSOR_HUMIDITY: &str = "sensor.humidity";
    pub const SENSOR_AIR_QUALITY: &str = "sensor.air_quality";
    pub const SENSOR_ACCELEROMETER: &str = "sensor.accelerometer";
    pub const SENSOR_CAMERA: &str = "sensor.camera";
    pub const SENSOR_MICROPHONE: &str = "sensor.microphone";
    pub const SENSOR_POWER_METER: &str = "sensor.power_meter";
    pub const SENSOR_LIGHT: &str = "sensor.light";

    // Network capabilities
    pub const NET_PUBLIC_IP: &str = "net.public_ip";
    pub const NET_IPV6: &str = "net.ipv6";
    pub const NET_BANDWIDTH_MBPS: &str = "net.bandwidth_mbps";
    pub const NET_LORA: &str = "net.lora";
    pub const NET_BLUETOOTH: &str = "net.bluetooth";
    pub const NET_ZIGBEE: &str = "net.zigbee";
    pub const NET_MESH: &str = "net.mesh"; // Mesh networking capability

    // Software/runtime
    pub const RUNTIME_PYTHON: &str = "runtime.python";
    pub const RUNTIME_NODEJS: &str = "runtime.nodejs";
    pub const RUNTIME_WASM: &str = "runtime.wasm";
    pub const RUNTIME_DOCKER: &str = "runtime.docker";
    pub const ML_PYTORCH: &str = "ml.pytorch";
    pub const ML_TENSORFLOW: &str = "ml.tensorflow";
    pub const ML_ONNX: &str = "ml.onnx";
    pub const CUDA_VERSION: &str = "runtime.cuda_version";

    // Domain-specific
    pub const DOMAIN_PAYMENT_TERMINAL: &str = "domain.payment_terminal";
    pub const DOMAIN_IOT_GATEWAY: &str = "domain.iot_gateway";
    pub const DOMAIN_SOLAR_INVERTER: &str = "domain.solar_inverter";
    pub const DOMAIN_EV_CHARGER: &str = "domain.ev_charger";
    pub const DOMAIN_3D_PRINTER: &str = "domain.3d_printer";
    pub const DOMAIN_CNC_MACHINE: &str = "domain.cnc_machine";
}

/// Extended capabilities beyond standard compute resources.
///
/// This allows nodes to advertise arbitrary hardware, sensors, and software
/// capabilities that can be matched during task placement or service discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtendedCapabilities {
    /// Flexible key-value capability store
    #[serde(default)]
    pub capabilities: HashMap<String, CapabilityValue>,

    /// When capabilities were last sensed/updated
    #[serde(default)]
    pub sensed_at: u64,

    /// Which capabilities are verified vs self-reported
    #[serde(default)]
    pub verified: HashSet<String>,
}

impl ExtendedCapabilities {
    pub fn new() -> Self {
        Self {
            capabilities: HashMap::new(),
            sensed_at: current_timestamp(),
            verified: HashSet::new(),
        }
    }

    /// Add a capability
    pub fn set(&mut self, key: impl Into<String>, value: CapabilityValue) {
        self.capabilities.insert(key.into(), value);
        self.sensed_at = current_timestamp();
    }

    /// Check if a capability exists
    pub fn has(&self, key: &str) -> bool {
        self.capabilities.contains_key(key)
    }

    /// Get a capability value
    pub fn get(&self, key: &str) -> Option<&CapabilityValue> {
        self.capabilities.get(key)
    }

    /// Check if capability meets a minimum threshold
    pub fn meets(&self, key: &str, min: f64) -> bool {
        self.capabilities
            .get(key)
            .map(|v| v.meets_threshold(min))
            .unwrap_or(false)
    }

    /// Mark a capability as verified (e.g., by remote attestation)
    pub fn mark_verified(&mut self, key: &str) {
        if self.capabilities.contains_key(key) {
            self.verified.insert(key.to_string());
        }
    }

    /// Check if a capability is verified
    pub fn is_verified(&self, key: &str) -> bool {
        self.verified.contains(key)
    }

    /// Get all capabilities of a certain prefix (e.g., "sensor.*")
    pub fn with_prefix(&self, prefix: &str) -> HashMap<&str, &CapabilityValue> {
        self.capabilities
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    /// Merge capabilities from another source (e.g., plugin detection)
    pub fn merge(&mut self, other: ExtendedCapabilities) {
        for (key, value) in other.capabilities {
            self.capabilities.insert(key, value);
        }
        if other.sensed_at > self.sensed_at {
            self.sensed_at = other.sensed_at;
        }
    }
}

/// Service roles a node can acquire.
///
/// Unifies three existing concepts:
/// - Network roles (from `NodeRole`: Edge, Rendezvous, Archive)
/// - Compute roles (CCL/WASM/GPU execution)
/// - Storage roles (replica holding)
/// - Platform roles (Gateway API)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceRole {
    // Network roles (from NodeRole)
    /// Leaf node, minimal routing responsibility
    Edge,
    /// NAT traversal helper, connection broker
    Rendezvous,
    /// Long-term storage, history queries
    Archive,

    // Compute roles
    /// Can execute CCL contracts (low requirements: 128MB RAM)
    CclExecutor,
    /// Can execute WASM contracts (higher requirements: 512MB RAM)
    WasmExecutor,
    /// Can execute GPU compute tasks (requires GPU with sufficient VRAM)
    GpuExecutor,

    // Storage roles
    /// Stores replicas for other nodes
    ReplicaHolder,

    // Platform roles
    /// Runs HTTP/WebSocket gateway for applications
    Gateway,
}

impl ServiceRole {
    /// All possible service roles
    pub fn all() -> &'static [ServiceRole] {
        &[
            ServiceRole::Edge,
            ServiceRole::Rendezvous,
            ServiceRole::Archive,
            ServiceRole::CclExecutor,
            ServiceRole::WasmExecutor,
            ServiceRole::GpuExecutor,
            ServiceRole::ReplicaHolder,
            ServiceRole::Gateway,
        ]
    }

    /// Network-layer roles only
    pub fn network_roles() -> &'static [ServiceRole] {
        &[
            ServiceRole::Edge,
            ServiceRole::Rendezvous,
            ServiceRole::Archive,
        ]
    }

    /// Compute-layer roles only
    pub fn compute_roles() -> &'static [ServiceRole] {
        &[
            ServiceRole::CclExecutor,
            ServiceRole::WasmExecutor,
            ServiceRole::GpuExecutor,
        ]
    }
}

/// Node lifecycle stage.
///
/// Nodes progress: Sensing → Active → Retiring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeStage {
    /// Just started, probing environment and capabilities
    #[default]
    Sensing,

    /// Fully participating in network
    Active,

    /// Gracefully stepping down (draining tasks, transferring replicas)
    Retiring,
}

impl NodeStage {
    /// Check if node is accepting new work
    pub fn is_accepting_work(&self) -> bool {
        matches!(self, NodeStage::Active)
    }

    /// Check if node is in sensing phase
    pub fn is_sensing(&self) -> bool {
        matches!(self, NodeStage::Sensing)
    }
}

/// Complete profile describing a node's network role and capabilities.
///
/// This unifies `TopologyInfo` (network layer) with `NodeCapacity` (compute layer)
/// and adds service roles that emerge from capability sensing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProfile {
    /// This node's DID (device identity)
    pub node_did: Did,

    /// Principal DID operating this node (user/organization)
    pub operator_did: Did,

    /// Network topology information (region, cluster, latency metrics)
    pub topology: TopologyInfo,

    /// Hardware capacity (CPU, RAM, storage, network, GPU)
    pub capacity: NodeCapacity,

    /// Active service roles (acquired via capability sensing)
    pub roles: HashSet<ServiceRole>,

    /// Extended capabilities (sensors, hardware, software features)
    #[serde(default)]
    pub extended: ExtendedCapabilities,

    /// Current lifecycle stage
    pub stage: NodeStage,

    /// Policy constraints from operator
    pub policy: NodePolicy,

    /// Last profile update timestamp (Unix seconds)
    pub updated_at: u64,
}

impl NodeProfile {
    /// Create a new node profile in Sensing stage
    pub fn new(
        node_did: Did,
        operator_did: Did,
        topology: TopologyInfo,
        capacity: NodeCapacity,
    ) -> Self {
        Self {
            node_did,
            operator_did,
            topology,
            capacity,
            roles: HashSet::from([ServiceRole::Edge]), // All nodes start as Edge
            extended: ExtendedCapabilities::new(),
            stage: NodeStage::Sensing,
            policy: NodePolicy::default(),
            updated_at: current_timestamp(),
        }
    }

    /// Create a new node profile with extended capabilities
    pub fn with_extended(
        node_did: Did,
        operator_did: Did,
        topology: TopologyInfo,
        capacity: NodeCapacity,
        extended: ExtendedCapabilities,
    ) -> Self {
        Self {
            node_did,
            operator_did,
            topology,
            capacity,
            roles: HashSet::from([ServiceRole::Edge]),
            extended,
            stage: NodeStage::Sensing,
            policy: NodePolicy::default(),
            updated_at: current_timestamp(),
        }
    }

    /// Infer roles from current capacity and policy
    pub fn infer_roles(&mut self, role_policy: &RolePolicy, trust_score: f64) {
        // Start fresh but always include Edge
        self.roles.clear();
        self.roles.insert(ServiceRole::Edge);

        // CCL executor: low bar (128MB default)
        if self.capacity.memory_mb_available >= role_policy.ccl_min_ram_mb
            && !self
                .policy
                .disallowed_roles
                .contains(&ServiceRole::CclExecutor)
        {
            self.roles.insert(ServiceRole::CclExecutor);
        }

        // WASM executor: higher requirements (512MB default)
        if self.capacity.memory_mb_available >= role_policy.wasm_min_ram_mb
            && !self
                .policy
                .disallowed_roles
                .contains(&ServiceRole::WasmExecutor)
        {
            self.roles.insert(ServiceRole::WasmExecutor);
        }

        // GPU executor: requires GPU with sufficient VRAM and operator opt-in
        let has_gpu = self
            .extended
            .meets(capability_keys::GPU_MEMORY_GB, role_policy.gpu_min_vram_gb);
        let gpu_offered = self.extended.has(capability_keys::GPU_COMPUTE_OFFERED);
        if has_gpu
            && gpu_offered
            && !self
                .policy
                .disallowed_roles
                .contains(&ServiceRole::GpuExecutor)
        {
            self.roles.insert(ServiceRole::GpuExecutor);
        }

        // Replica holder: needs storage commitment (1GB default)
        if self.capacity.storage_mb_available >= role_policy.replica_min_storage_mb
            && !self
                .policy
                .disallowed_roles
                .contains(&ServiceRole::ReplicaHolder)
        {
            self.roles.insert(ServiceRole::ReplicaHolder);
        }

        // Archive: needs high trust + lots of storage
        if trust_score >= role_policy.archive_min_trust
            && self.capacity.storage_mb_available >= role_policy.replica_min_storage_mb * 10
            && !self.policy.disallowed_roles.contains(&ServiceRole::Archive)
        {
            self.roles.insert(ServiceRole::Archive);
        }

        // Note: Rendezvous and Gateway require explicit configuration (not auto-inferred)
        // They depend on network setup (public IP, port forwarding) not just hardware

        self.updated_at = current_timestamp();
    }

    /// Transition to Active stage
    pub fn activate(&mut self) {
        if self.stage == NodeStage::Sensing {
            self.stage = NodeStage::Active;
            self.updated_at = current_timestamp();
        }
    }

    /// Begin graceful retirement
    pub fn begin_retirement(&mut self) {
        if self.stage == NodeStage::Active {
            self.stage = NodeStage::Retiring;
            self.updated_at = current_timestamp();
        }
    }

    /// Check if node can execute CCL contracts
    pub fn can_execute_ccl(&self) -> bool {
        self.stage.is_accepting_work() && self.roles.contains(&ServiceRole::CclExecutor)
    }

    /// Check if node can execute WASM contracts
    pub fn can_execute_wasm(&self) -> bool {
        self.stage.is_accepting_work() && self.roles.contains(&ServiceRole::WasmExecutor)
    }

    /// Check if node can execute GPU compute tasks
    pub fn can_execute_gpu(&self) -> bool {
        self.stage.is_accepting_work() && self.roles.contains(&ServiceRole::GpuExecutor)
    }

    /// Check if node can store replicas
    pub fn can_store_replicas(&self) -> bool {
        self.stage.is_accepting_work() && self.roles.contains(&ServiceRole::ReplicaHolder)
    }

    /// Check if node has a specific role
    pub fn has_role(&self, role: ServiceRole) -> bool {
        self.roles.contains(&role)
    }

    /// Get roles as a sorted vector (for consistent serialization)
    pub fn roles_sorted(&self) -> Vec<ServiceRole> {
        let mut roles: Vec<_> = self.roles.iter().copied().collect();
        roles.sort_by_key(|r| format!("{r:?}"));
        roles
    }
}

/// Operator-defined constraints on node behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodePolicy {
    /// Roles this node may never acquire (even if capable)
    #[serde(default)]
    pub disallowed_roles: HashSet<ServiceRole>,

    /// Maximum resources to commit (prevent runaway usage)
    #[serde(default)]
    pub resource_caps: Option<ResourceCaps>,

    /// Cooperative memberships this node serves
    #[serde(default)]
    pub coop_ids: Vec<String>,
}

impl NodePolicy {
    /// Create a restrictive policy for shared/kiosk devices
    pub fn kiosk() -> Self {
        Self {
            disallowed_roles: HashSet::from([
                ServiceRole::WasmExecutor,
                ServiceRole::ReplicaHolder,
                ServiceRole::Archive,
            ]),
            resource_caps: Some(ResourceCaps {
                max_cpu_percent: 25,
                max_ram_mb: 512,
                max_storage_mb: 1024,
                max_bandwidth_mbps: 10,
            }),
            coop_ids: vec![],
        }
    }

    /// Create an unrestricted policy for dedicated servers
    pub fn server() -> Self {
        Self {
            disallowed_roles: HashSet::new(),
            resource_caps: None,
            coop_ids: vec![],
        }
    }
}

/// Resource usage caps set by operator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCaps {
    /// Maximum CPU usage (percent, e.g., 50 = use at most 50% CPU)
    pub max_cpu_percent: u8,

    /// Maximum RAM usage (MB)
    pub max_ram_mb: u64,

    /// Maximum storage usage (MB)
    pub max_storage_mb: u64,

    /// Maximum network bandwidth (Mbps)
    pub max_bandwidth_mbps: u32,
}

/// Policy for role inference (configurable thresholds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePolicy {
    /// Minimum RAM to enable CCL execution (MB)
    pub ccl_min_ram_mb: u64,

    /// Minimum RAM to enable WASM execution (MB)
    pub wasm_min_ram_mb: u64,

    /// Minimum storage to enable replica holding (MB)
    pub replica_min_storage_mb: u64,

    /// Minimum trust score to enable Archive role (0.0 - 1.0)
    pub archive_min_trust: f64,

    /// Minimum GPU memory to enable GPU execution (GB)
    pub gpu_min_vram_gb: f64,
}

impl Default for RolePolicy {
    fn default() -> Self {
        Self {
            ccl_min_ram_mb: 128,          // 128 MB for CCL
            wasm_min_ram_mb: 512,         // 512 MB for WASM
            replica_min_storage_mb: 1024, // 1 GB for replicas
            archive_min_trust: 0.6,       // High trust for Archive
            gpu_min_vram_gb: 4.0,         // 4 GB VRAM for GPU compute
        }
    }
}

impl RolePolicy {
    /// Strict policy requiring more resources
    pub fn strict() -> Self {
        Self {
            ccl_min_ram_mb: 256,
            wasm_min_ram_mb: 1024,
            replica_min_storage_mb: 10240, // 10 GB
            archive_min_trust: 0.8,
            gpu_min_vram_gb: 8.0, // 8 GB VRAM
        }
    }

    /// Lenient policy for resource-constrained networks
    pub fn lenient() -> Self {
        Self {
            ccl_min_ram_mb: 64,
            wasm_min_ram_mb: 256,
            replica_min_storage_mb: 512,
            archive_min_trust: 0.4,
            gpu_min_vram_gb: 2.0, // 2 GB VRAM
        }
    }
}

/// Message types for gossip-based profile synchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfileMessage {
    /// Announce new or updated profile
    Announce(NodeProfile),

    /// Request profile for a specific node DID
    Query(Did),

    /// Response to profile query
    Response(Option<NodeProfile>),
}

/// Gossip topic for node profile announcements
pub const TOPIC_NODE_PROFILES: &str = "network:profiles";

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Sense the current hardware capabilities of this node.
///
/// This provides a best-effort estimate of available resources. On systems where
/// resource detection fails, conservative defaults are used.
pub fn sense_hardware() -> NodeCapacity {
    // Try to detect CPU cores
    let cpu_cores = std::thread::available_parallelism()
        .map(|p| p.get() as f64)
        .unwrap_or(1.0);

    // Try to read memory info from /proc/meminfo (Linux)
    // Fall back to conservative defaults on other platforms
    let (memory_total, memory_available) = read_memory_info().unwrap_or((1024, 512)); // Default 1GB/512MB

    // Try to get available disk space from data directory
    // This is best-effort; we use a conservative default if detection fails
    let storage_available = 10 * 1024; // Default 10GB in MB

    NodeCapacity {
        cpu_cores_total: cpu_cores,
        cpu_cores_available: cpu_cores * 0.8, // Reserve 20% for system
        memory_mb_total: memory_total,
        memory_mb_available: memory_available,
        storage_mb_available: storage_available,
        network_mbps: 100.0, // Assume reasonable network
        gpu_devices: vec![], // GPU detection not implemented yet
        updated_at: current_timestamp(),
    }
}

/// Read memory information from /proc/meminfo (Linux-specific)
fn read_memory_info() -> Option<(u64, u64)> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/meminfo").ok()?;
        let mut total_kb: Option<u64> = None;
        let mut available_kb: Option<u64> = None;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            } else if line.starts_with("MemAvailable:") {
                available_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            }
        }

        match (total_kb, available_kb) {
            (Some(total), Some(available)) => Some((total / 1024, available / 1024)),
            _ => None,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux, return None to use defaults
        None
    }
}

/// Sense extended capabilities beyond basic compute resources.
///
/// This function attempts to detect:
/// - GPU/accelerator hardware
/// - Available sensors
/// - Network features
/// - Software runtimes
///
/// Detection is best-effort; capabilities that can't be detected are omitted.
pub fn sense_extended_capabilities() -> ExtendedCapabilities {
    let mut caps = ExtendedCapabilities::new();

    // GPU detection (placeholder - real implementation would use nvml, vulkan, etc.)
    #[cfg(target_os = "linux")]
    {
        // Check for NVIDIA GPU by looking for nvidia-smi or /dev/nvidia*
        if std::path::Path::new("/dev/nvidia0").exists() {
            caps.set(capability_keys::GPU_AVAILABLE, CapabilityValue::present());
            caps.set(capability_keys::GPU_VENDOR, CapabilityValue::text("nvidia"));

            // Try to get GPU memory from nvidia-smi (simplified)
            // In production, use nvml crate for proper detection
            if let Ok(output) = std::process::Command::new("nvidia-smi")
                .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
                .output()
            {
                if let Ok(mem_str) = String::from_utf8(output.stdout) {
                    if let Ok(mem_mb) = mem_str.trim().parse::<f64>() {
                        let mem_gb = mem_mb / 1024.0;
                        caps.set(
                            capability_keys::GPU_MEMORY_GB,
                            CapabilityValue::numeric_with_unit(mem_gb, "GB"),
                        );
                    }
                }
            }
        }

        // Check for AMD GPU
        if std::path::Path::new("/dev/dri/renderD128").exists()
            && !caps.has(capability_keys::GPU_AVAILABLE)
        {
            caps.set(capability_keys::GPU_AVAILABLE, CapabilityValue::present());
            caps.set(capability_keys::GPU_VENDOR, CapabilityValue::text("amd"));
        }
    }

    // Network capability detection
    if std::env::var("ICN_PUBLIC_IP").is_ok() {
        caps.set(capability_keys::NET_PUBLIC_IP, CapabilityValue::present());
    }

    // Runtime detection
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok()
    {
        caps.set(capability_keys::RUNTIME_PYTHON, CapabilityValue::present());
    }

    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok()
    {
        caps.set(capability_keys::RUNTIME_NODEJS, CapabilityValue::present());
    }

    if std::process::Command::new("docker")
        .arg("--version")
        .output()
        .is_ok()
    {
        caps.set(capability_keys::RUNTIME_DOCKER, CapabilityValue::present());
    }

    caps
}

/// Create a NodeProfile from config and identity.
///
/// This is the main entry point for creating a node profile during supervisor startup.
pub fn create_node_profile(
    node_did: Did,
    operator_did: Did,
    topology_config: &icn_net::TopologyConfig,
) -> NodeProfile {
    // Sense hardware capabilities
    let capacity = sense_hardware();

    // Sense extended capabilities (GPU, sensors, etc.)
    let extended = sense_extended_capabilities();

    // Convert TopologyConfig to TopologyInfo
    let topology = TopologyInfo {
        region: topology_config.region.clone(),
        cluster_id: topology_config.cluster_id.clone(),
        role: topology_config.role,
    };

    // Create profile in Sensing stage with extended capabilities
    let mut profile =
        NodeProfile::with_extended(node_did, operator_did, topology, capacity, extended);

    // Infer initial roles based on detected capacity
    // Use default role policy and assume no trust score yet (0.0)
    profile.infer_roles(&RolePolicy::default(), 0.0);

    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_net::NodeRole;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn test_capacity(ram_mb: u64, storage_mb: u64) -> NodeCapacity {
        NodeCapacity {
            cpu_cores_total: 4.0,
            cpu_cores_available: 4.0,
            memory_mb_total: ram_mb,
            memory_mb_available: ram_mb,
            storage_mb_available: storage_mb,
            network_mbps: 100.0,
            gpu_devices: vec![],
            updated_at: 0,
        }
    }

    fn test_topology() -> TopologyInfo {
        TopologyInfo {
            region: "us-west".to_string(),
            cluster_id: "cluster-1".to_string(),
            role: NodeRole::Edge,
        }
    }

    #[test]
    fn test_new_profile_starts_sensing() {
        let profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
        );

        assert_eq!(profile.stage, NodeStage::Sensing);
        assert!(profile.roles.contains(&ServiceRole::Edge));
        assert_eq!(profile.roles.len(), 1);
    }

    #[test]
    fn test_role_inference_ccl() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(256, 512), // Enough for CCL, not for replicas
        );

        profile.infer_roles(&RolePolicy::default(), 0.5);

        assert!(profile.roles.contains(&ServiceRole::Edge));
        assert!(profile.roles.contains(&ServiceRole::CclExecutor));
        assert!(!profile.roles.contains(&ServiceRole::WasmExecutor));
        assert!(!profile.roles.contains(&ServiceRole::ReplicaHolder));
    }

    #[test]
    fn test_role_inference_wasm() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 512), // Enough for WASM
        );

        profile.infer_roles(&RolePolicy::default(), 0.5);

        assert!(profile.roles.contains(&ServiceRole::CclExecutor));
        assert!(profile.roles.contains(&ServiceRole::WasmExecutor));
    }

    #[test]
    fn test_role_inference_replica_holder() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(256, 2048), // Enough for replicas
        );

        profile.infer_roles(&RolePolicy::default(), 0.5);

        assert!(profile.roles.contains(&ServiceRole::ReplicaHolder));
    }

    #[test]
    fn test_role_inference_archive_requires_trust() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 20480), // Lots of storage
        );

        // Low trust - no Archive role
        profile.infer_roles(&RolePolicy::default(), 0.3);
        assert!(!profile.roles.contains(&ServiceRole::Archive));

        // High trust - gets Archive role
        profile.infer_roles(&RolePolicy::default(), 0.7);
        assert!(profile.roles.contains(&ServiceRole::Archive));
    }

    #[test]
    fn test_disallowed_roles_respected() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
        );
        profile
            .policy
            .disallowed_roles
            .insert(ServiceRole::WasmExecutor);

        profile.infer_roles(&RolePolicy::default(), 0.5);

        assert!(profile.roles.contains(&ServiceRole::CclExecutor));
        assert!(!profile.roles.contains(&ServiceRole::WasmExecutor));
    }

    #[test]
    fn test_lifecycle_transitions() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
        );

        assert_eq!(profile.stage, NodeStage::Sensing);
        assert!(!profile.stage.is_accepting_work());

        profile.activate();
        assert_eq!(profile.stage, NodeStage::Active);
        assert!(profile.stage.is_accepting_work());

        profile.begin_retirement();
        assert_eq!(profile.stage, NodeStage::Retiring);
        assert!(!profile.stage.is_accepting_work());
    }

    #[test]
    fn test_kiosk_policy() {
        let policy = NodePolicy::kiosk();

        assert!(policy.disallowed_roles.contains(&ServiceRole::WasmExecutor));
        assert!(policy
            .disallowed_roles
            .contains(&ServiceRole::ReplicaHolder));
        assert!(policy.disallowed_roles.contains(&ServiceRole::Archive));
        assert!(policy.resource_caps.is_some());
    }

    #[test]
    fn test_can_execute_requires_active_stage() {
        let mut profile = NodeProfile::new(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
        );
        profile.infer_roles(&RolePolicy::default(), 0.5);

        // Sensing stage - can't execute
        assert!(!profile.can_execute_ccl());

        // Active stage - can execute
        profile.activate();
        assert!(profile.can_execute_ccl());

        // Retiring stage - can't accept new work
        profile.begin_retirement();
        assert!(!profile.can_execute_ccl());
    }

    #[test]
    fn test_extended_capabilities_basic() {
        let mut caps = ExtendedCapabilities::new();

        // Test setting and getting capabilities
        caps.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric_with_unit(8.0, "GB"),
        );
        caps.set(capability_keys::GPU_VENDOR, CapabilityValue::text("nvidia"));
        caps.set(capability_keys::SENSOR_GPS, CapabilityValue::present());

        assert!(caps.has(capability_keys::GPU_MEMORY_GB));
        assert!(caps.has(capability_keys::GPU_VENDOR));
        assert!(caps.has(capability_keys::SENSOR_GPS));
        assert!(!caps.has(capability_keys::TPU_AVAILABLE));

        // Test threshold checking
        assert!(caps.meets(capability_keys::GPU_MEMORY_GB, 4.0));
        assert!(!caps.meets(capability_keys::GPU_MEMORY_GB, 16.0));
    }

    #[test]
    fn test_extended_capabilities_prefix_query() {
        let mut caps = ExtendedCapabilities::new();

        caps.set(capability_keys::SENSOR_GPS, CapabilityValue::present());
        caps.set(
            capability_keys::SENSOR_TEMPERATURE,
            CapabilityValue::numeric_with_unit(25.0, "C"),
        );
        caps.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(8.0),
        );

        let sensors = caps.with_prefix("sensor.");
        assert_eq!(sensors.len(), 2);
        assert!(sensors.contains_key("sensor.gps"));
        assert!(sensors.contains_key("sensor.temperature"));
    }

    #[test]
    fn test_gpu_role_requires_opt_in() {
        let mut extended = ExtendedCapabilities::new();
        extended.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(8.0),
        );
        // Note: NOT setting GPU_COMPUTE_OFFERED

        let mut profile = NodeProfile::with_extended(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
            extended,
        );

        profile.infer_roles(&RolePolicy::default(), 0.5);

        // Has GPU but didn't opt-in to offering compute
        assert!(!profile.roles.contains(&ServiceRole::GpuExecutor));
    }

    #[test]
    fn test_gpu_role_with_opt_in() {
        let mut extended = ExtendedCapabilities::new();
        extended.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(8.0),
        );
        extended.set(
            capability_keys::GPU_COMPUTE_OFFERED,
            CapabilityValue::present(),
        );

        let mut profile = NodeProfile::with_extended(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
            extended,
        );

        profile.infer_roles(&RolePolicy::default(), 0.5);

        // Has GPU AND opted in
        assert!(profile.roles.contains(&ServiceRole::GpuExecutor));
    }

    #[test]
    fn test_gpu_role_insufficient_vram() {
        let mut extended = ExtendedCapabilities::new();
        extended.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(2.0),
        ); // Only 2GB
        extended.set(
            capability_keys::GPU_COMPUTE_OFFERED,
            CapabilityValue::present(),
        );

        let mut profile = NodeProfile::with_extended(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
            extended,
        );

        profile.infer_roles(&RolePolicy::default(), 0.5); // Default requires 4GB

        // Opted in but not enough VRAM
        assert!(!profile.roles.contains(&ServiceRole::GpuExecutor));
    }

    #[test]
    fn test_capability_verification() {
        let mut caps = ExtendedCapabilities::new();
        caps.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(8.0),
        );

        assert!(!caps.is_verified(capability_keys::GPU_MEMORY_GB));

        caps.mark_verified(capability_keys::GPU_MEMORY_GB);

        assert!(caps.is_verified(capability_keys::GPU_MEMORY_GB));
        assert!(!caps.is_verified(capability_keys::GPU_VENDOR)); // Never set
    }

    #[test]
    fn test_capability_merge() {
        let mut caps1 = ExtendedCapabilities::new();
        caps1.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(8.0),
        );

        let mut caps2 = ExtendedCapabilities::new();
        caps2.set(capability_keys::SENSOR_GPS, CapabilityValue::present());
        caps2.set(capability_keys::GPU_VENDOR, CapabilityValue::text("nvidia"));

        caps1.merge(caps2);

        // Should have all three capabilities
        assert!(caps1.has(capability_keys::GPU_MEMORY_GB));
        assert!(caps1.has(capability_keys::SENSOR_GPS));
        assert!(caps1.has(capability_keys::GPU_VENDOR));
    }

    #[test]
    fn test_can_execute_gpu_requires_active() {
        let mut extended = ExtendedCapabilities::new();
        extended.set(
            capability_keys::GPU_MEMORY_GB,
            CapabilityValue::numeric(8.0),
        );
        extended.set(
            capability_keys::GPU_COMPUTE_OFFERED,
            CapabilityValue::present(),
        );

        let mut profile = NodeProfile::with_extended(
            make_test_did(),
            make_test_did(),
            test_topology(),
            test_capacity(1024, 10240),
            extended,
        );
        profile.infer_roles(&RolePolicy::default(), 0.5);

        // Sensing stage - can't execute
        assert!(!profile.can_execute_gpu());

        // Active stage - can execute
        profile.activate();
        assert!(profile.can_execute_gpu());

        // Retiring stage - can't accept new work
        profile.begin_retirement();
        assert!(!profile.can_execute_gpu());
    }
}
