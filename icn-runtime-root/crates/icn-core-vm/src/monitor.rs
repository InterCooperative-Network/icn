/*!
# Runtime Monitoring and Metrics

This module implements a monitoring service for the ICN Runtime that provides:
1. Prometheus-compatible metrics for performance tracking
2. Runtime execution result and error logging
3. DAG anchoring and resource consumption tracking
*/

use crate::{ResourceType, VmError};
use icn_identity::IdentityId;
use prometheus::{
    register_counter_vec, register_histogram_vec, register_gauge_vec,
    CounterVec, HistogramVec, GaugeVec,
};
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use cid::Cid;
use tokio::sync::mpsc::{self, Sender, Receiver};
use once_cell::sync::Lazy;
use std::collections::HashMap;

// Default metric labels
const FEDERATION_LABEL: &str = "federation";
const IDENTITY_LABEL: &str = "identity";
const RESOURCE_TYPE_LABEL: &str = "resource_type";
const STATUS_LABEL: &str = "status";
const ERROR_TYPE_LABEL: &str = "error_type";

// Metrics registration with Prometheus
static EXECUTION_TIME: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "icn_runtime_execution_time_seconds",
        "Time taken for WASM execution",
        &[FEDERATION_LABEL, IDENTITY_LABEL, STATUS_LABEL],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]
    ).unwrap()
});

static RESOURCE_METERING_OVERHEAD: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "icn_runtime_metering_overhead_seconds",
        "Overhead time for resource metering",
        &[RESOURCE_TYPE_LABEL],
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]
    ).unwrap()
});

static DAG_ANCHORING_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "icn_runtime_dag_anchoring_seconds",
        "Time taken for DAG anchoring operations",
        &[FEDERATION_LABEL],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
    ).unwrap()
});

static CREDENTIAL_ISSUANCE_THROUGHPUT: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "icn_runtime_credential_issuance_total",
        "Total number of credentials issued",
        &[FEDERATION_LABEL, STATUS_LABEL]
    ).unwrap()
});

static RESOURCE_CONSUMPTION: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "icn_runtime_resource_consumption",
        "Current resource consumption levels",
        &[FEDERATION_LABEL, RESOURCE_TYPE_LABEL]
    ).unwrap()
});

static EXECUTION_ERRORS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "icn_runtime_execution_errors_total",
        "Total number of execution errors by type",
        &[FEDERATION_LABEL, ERROR_TYPE_LABEL]
    ).unwrap()
});

/// Types of events the RuntimeMonitor can track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorEvent {
    /// Execution of WASM module started
    ExecutionStarted {
        execution_id: String,
        federation_id: Option<String>,
        caller_id: String,
        timestamp: u64,
    },
    
    /// Execution of WASM module completed
    ExecutionCompleted {
        execution_id: String,
        duration_ms: u64,
        success: bool,
        error_type: Option<String>,
        error_message: Option<String>,
    },
    
    /// Resource metering event
    ResourceMetered {
        execution_id: String,
        resource_type: String,
        amount: u64,
        overhead_ns: u64,
    },
    
    /// DAG anchoring event
    DagAnchored {
        execution_id: String,
        federation_id: Option<String>,
        cid: String,
        latency_ms: u64,
    },
    
    /// Credential issuance event
    CredentialIssued {
        execution_id: String,
        federation_id: Option<String>,
        issuer: String,
        subject: String,
        success: bool,
    },
}

/// Result of a runtime execution with metrics
#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    /// Unique execution ID
    pub execution_id: String,
    
    /// Total execution time
    pub total_duration: Duration,
    
    /// Resource consumption by type
    pub resources: HashMap<ResourceType, u64>,
    
    /// Number of DAG operations
    pub dag_operations: usize,
    
    /// Number of credential operations
    pub credential_operations: usize,
    
    /// Execution successful
    pub success: bool,
    
    /// Error if any
    pub error: Option<VmError>,
}

/// Runtime monitor implementation
pub struct RuntimeMonitor {
    /// Channel sender for monitor events
    event_sender: Sender<MonitorEvent>,
    
    /// Current federation ID
    federation_id: Option<String>,
    
    /// Active execution timers
    execution_timers: Mutex<HashMap<String, Instant>>,
    
    /// Aggregated metrics for periodic reporting
    metrics: Mutex<HashMap<String, ExecutionMetrics>>,
}

impl RuntimeMonitor {
    /// Create a new RuntimeMonitor instance
    pub fn new(federation_id: Option<String>) -> (Self, Receiver<MonitorEvent>) {
        let (tx, rx) = mpsc::channel(1000);
        
        (Self {
            event_sender: tx,
            federation_id,
            execution_timers: Mutex::new(HashMap::new()),
            metrics: Mutex::new(HashMap::new()),
        }, rx)
    }
    
    /// Start execution monitoring
    pub fn start_execution(&self, execution_id: &str, caller_id: &IdentityId) {
        let federation = self.federation_id.clone().unwrap_or_else(|| "unknown".to_string());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Record start time
        self.execution_timers.lock().unwrap().insert(
            execution_id.to_string(),
            Instant::now()
        );
        
        // Send event
        let _ = self.event_sender.try_send(MonitorEvent::ExecutionStarted {
            execution_id: execution_id.to_string(),
            federation_id: self.federation_id.clone(),
            caller_id: caller_id.to_string(),
            timestamp,
        });
        
        debug!(
            execution_id = execution_id,
            federation = federation,
            caller = %caller_id,
            "Execution started"
        );
    }
    
    /// End execution monitoring
    pub fn end_execution(&self, execution_id: &str, result: Result<(), VmError>) {
        let mut timers = self.execution_timers.lock().unwrap();
        let start_time = timers.remove(execution_id);
        
        if let Some(start) = start_time {
            let duration = start.elapsed();
            let duration_ms = duration.as_millis() as u64;
            
            // Record metrics
            let federation = self.federation_id.clone().unwrap_or_else(|| "unknown".to_string());
            let status = if result.is_ok() { "success" } else { "error" };
            
            EXECUTION_TIME
                .with_label_values(&[&federation, execution_id, status])
                .observe(duration.as_secs_f64());
            
            // Log the result
            match &result {
                Ok(_) => {
                    info!(
                        execution_id = execution_id,
                        duration_ms = duration_ms,
                        "Execution completed successfully"
                    );
                }
                Err(e) => {
                    let error_type = match e {
                        VmError::ResourceLimitExceeded(_) => "resource_limit",
                        VmError::ExecutionError(_) => "execution",
                        VmError::LinkerError(_) => "linker",
                        VmError::CompileError(_) => "compile",
                        VmError::RuntimeError(_) => "runtime",
                        VmError::HostError(_) => "host",
                        VmError::IdentityError(_) => "identity",
                        VmError::StorageError(_) => "storage",
                        _ => "other",
                    };
                    
                    EXECUTION_ERRORS
                        .with_label_values(&[&federation, error_type])
                        .inc();
                    
                    error!(
                        execution_id = execution_id,
                        duration_ms = duration_ms,
                        error_type = error_type,
                        error = %e,
                        "Execution failed"
                    );
                }
            }
            
            // Send event
            let _ = self.event_sender.try_send(MonitorEvent::ExecutionCompleted {
                execution_id: execution_id.to_string(),
                duration_ms,
                success: result.is_ok(),
                error_type: result.as_ref().err().map(|e| format!("{:?}", e)),
                error_message: result.as_ref().err().map(|e| e.to_string()),
            });
            
            // Update metrics store for reporting
            let mut metrics = self.metrics.lock().unwrap();
            metrics.insert(execution_id.to_string(), ExecutionMetrics {
                execution_id: execution_id.to_string(),
                total_duration: duration,
                resources: HashMap::new(),
                dag_operations: 0,
                credential_operations: 0,
                success: result.is_ok(),
                error: result.err(),
            });
        }
    }
    
    /// Record resource metering
    pub fn record_resource_metering(
        &self,
        execution_id: &str,
        resource_type: ResourceType,
        amount: u64,
        overhead: Duration
    ) {
        let resource_str = match resource_type {
            ResourceType::Compute => "compute",
            ResourceType::Storage => "storage",
            ResourceType::Network => "network",
            ResourceType::Token => "token",
        };
        
        RESOURCE_METERING_OVERHEAD
            .with_label_values(&[resource_str])
            .observe(overhead.as_secs_f64());
        
        if let Some(federation) = &self.federation_id {
            RESOURCE_CONSUMPTION
                .with_label_values(&[federation, resource_str])
                .set(amount as f64);
        }
        
        // Send event
        let _ = self.event_sender.try_send(MonitorEvent::ResourceMetered {
            execution_id: execution_id.to_string(),
            resource_type: resource_str.to_string(),
            amount,
            overhead_ns: overhead.as_nanos() as u64,
        });
        
        // Update metrics
        let mut metrics = self.metrics.lock().unwrap();
        if let Some(metric) = metrics.get_mut(execution_id) {
            *metric.resources.entry(resource_type).or_insert(0) += amount;
        }
        
        debug!(
            execution_id = execution_id,
            resource_type = resource_str,
            amount = amount,
            overhead_ns = overhead.as_nanos(),
            "Resource metered"
        );
    }
    
    /// Record DAG anchoring
    pub fn record_dag_anchoring(
        &self,
        execution_id: &str,
        cid: &Cid,
        latency: Duration
    ) {
        if let Some(federation) = &self.federation_id {
            DAG_ANCHORING_LATENCY
                .with_label_values(&[federation])
                .observe(latency.as_secs_f64());
        }
        
        // Send event
        let _ = self.event_sender.try_send(MonitorEvent::DagAnchored {
            execution_id: execution_id.to_string(),
            federation_id: self.federation_id.clone(),
            cid: cid.to_string(),
            latency_ms: latency.as_millis() as u64,
        });
        
        // Update metrics
        let mut metrics = self.metrics.lock().unwrap();
        if let Some(metric) = metrics.get_mut(execution_id) {
            metric.dag_operations += 1;
        }
        
        debug!(
            execution_id = execution_id,
            cid = %cid,
            latency_ms = latency.as_millis(),
            "DAG anchoring completed"
        );
    }
    
    /// Record credential issuance
    pub fn record_credential_issuance(
        &self,
        execution_id: &str,
        issuer: &IdentityId,
        subject: &IdentityId,
        success: bool
    ) {
        let status = if success { "success" } else { "failure" };
        
        if let Some(federation) = &self.federation_id {
            CREDENTIAL_ISSUANCE_THROUGHPUT
                .with_label_values(&[federation, status])
                .inc();
        }
        
        // Send event
        let _ = self.event_sender.try_send(MonitorEvent::CredentialIssued {
            execution_id: execution_id.to_string(),
            federation_id: self.federation_id.clone(),
            issuer: issuer.to_string(),
            subject: subject.to_string(),
            success,
        });
        
        // Update metrics
        let mut metrics = self.metrics.lock().unwrap();
        if let Some(metric) = metrics.get_mut(execution_id) {
            metric.credential_operations += 1;
        }
        
        debug!(
            execution_id = execution_id,
            issuer = %issuer,
            subject = %subject,
            success = success,
            "Credential issuance processed"
        );
    }
    
    /// Get a report of all execution metrics
    pub fn get_execution_report(&self) -> Vec<ExecutionMetrics> {
        self.metrics.lock().unwrap().values().cloned().collect()
    }
    
    /// Clear old metrics data
    pub fn cleanup_old_metrics(&self, older_than: Duration) {
        let now = Instant::now();
        let mut metrics = self.metrics.lock().unwrap();
        
        // For simple cleanup we'll just remove all metrics
        // In a real implementation we would check execution timestamps
        metrics.clear();
    }
}

// Global instance for easy access throughout the codebase
static RUNTIME_MONITOR: Lazy<Arc<Mutex<Option<RuntimeMonitor>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

/// Initialize the global RuntimeMonitor instance
pub fn init_global_monitor(federation_id: Option<String>) -> Receiver<MonitorEvent> {
    let (monitor, rx) = RuntimeMonitor::new(federation_id);
    *RUNTIME_MONITOR.lock().unwrap() = Some(monitor);
    rx
}

/// Get a reference to the global RuntimeMonitor instance
pub fn get_global_monitor() -> Option<RuntimeMonitor> {
    RUNTIME_MONITOR.lock().unwrap().clone()
} 