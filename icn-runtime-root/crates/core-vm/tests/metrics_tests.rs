#![cfg(test)]

use icn_core_vm::{
    monitor::{RuntimeMonitor, MonitorEvent, init_global_monitor, get_global_monitor},
    IdentityContext, VMContext, ResourceAuthorization, ResourceType, InternalHostError
};
use icn_identity::{IdentityId, KeyPair};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use cid::{Cid, Version};
use std::str::FromStr;

// Helper to create test identity
fn create_test_identity() -> (KeyPair, IdentityId) {
    let keypair = KeyPair::new(vec![1, 2, 3], vec![4, 5, 6]);
    let identity_id = IdentityId::new("did:icn:test:metrics");
    (keypair, identity_id)
}

#[tokio::test]
async fn test_monitor_execution_tracking() {
    // Create test identity
    let (keypair, identity_id) = create_test_identity();
    
    // Create the monitor
    let (monitor, mut rx) = RuntimeMonitor::new(Some("test-federation".to_string()));
    
    // Start monitoring execution
    let execution_id = "test-execution-1";
    monitor.start_execution(execution_id, &identity_id);
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::ExecutionStarted { execution_id: id, federation_id, caller_id, .. } => {
            assert_eq!(id, execution_id);
            assert_eq!(federation_id, Some("test-federation".to_string()));
            assert_eq!(caller_id, identity_id.to_string());
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
    
    // End monitoring
    monitor.end_execution(execution_id, Ok(()));
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::ExecutionCompleted { execution_id: id, success, .. } => {
            assert_eq!(id, execution_id);
            assert!(success);
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
    
    // Verify metrics
    let report = monitor.get_execution_report();
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].execution_id, execution_id);
    assert!(report[0].success);
}

#[tokio::test]
async fn test_monitor_resource_metering() {
    // Create the monitor
    let (monitor, mut rx) = RuntimeMonitor::new(Some("test-federation".to_string()));
    
    // Record resource metering
    let execution_id = "test-execution-2";
    let resource_type = ResourceType::Compute;
    let amount = 100u64;
    let overhead = Duration::from_micros(50);
    
    monitor.record_resource_metering(execution_id, resource_type, amount, overhead);
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::ResourceMetered { execution_id: id, resource_type: rt, amount: a, .. } => {
            assert_eq!(id, execution_id);
            assert_eq!(rt, "compute");
            assert_eq!(a, amount);
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
}

#[tokio::test]
async fn test_monitor_dag_anchoring() {
    // Create the monitor
    let (monitor, mut rx) = RuntimeMonitor::new(Some("test-federation".to_string()));
    
    // Record DAG anchoring
    let execution_id = "test-execution-3";
    let cid = Cid::new_v1(0x55, cid::multihash::Code::Sha2_256.digest(b"test"));
    let latency = Duration::from_millis(20);
    
    monitor.record_dag_anchoring(execution_id, &cid, latency);
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::DagAnchored { execution_id: id, cid: c, latency_ms, .. } => {
            assert_eq!(id, execution_id);
            assert_eq!(c, cid.to_string());
            assert_eq!(latency_ms, 20);
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
}

#[tokio::test]
async fn test_monitor_credential_issuance() {
    // Create the monitor
    let (monitor, mut rx) = RuntimeMonitor::new(Some("test-federation".to_string()));
    
    // Record credential issuance
    let execution_id = "test-execution-4";
    let issuer = IdentityId::new("did:icn:test:issuer");
    let subject = IdentityId::new("did:icn:test:subject");
    let success = true;
    
    monitor.record_credential_issuance(execution_id, &issuer, &subject, success);
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::CredentialIssued { execution_id: id, issuer: i, subject: s, success: succ, .. } => {
            assert_eq!(id, execution_id);
            assert_eq!(i, issuer.to_string());
            assert_eq!(s, subject.to_string());
            assert_eq!(succ, success);
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
}

#[tokio::test]
async fn test_global_monitor() {
    // Initialize the global monitor
    let mut rx = init_global_monitor(Some("global-federation".to_string()));
    
    // Get the global monitor
    let monitor = get_global_monitor().unwrap();
    
    // Record some events
    let execution_id = "global-execution";
    let identity_id = IdentityId::new("did:icn:test:global");
    
    monitor.start_execution(execution_id, &identity_id);
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::ExecutionStarted { federation_id, .. } => {
            assert_eq!(federation_id, Some("global-federation".to_string()));
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
    
    monitor.end_execution(execution_id, Ok(()));
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::ExecutionCompleted { success, .. } => {
            assert!(success);
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
}

#[tokio::test]
async fn test_monitor_error_handling() {
    // Create the monitor
    let (monitor, mut rx) = RuntimeMonitor::new(Some("test-federation".to_string()));
    
    // Start execution
    let execution_id = "error-execution";
    let identity_id = IdentityId::new("did:icn:test:error");
    
    monitor.start_execution(execution_id, &identity_id);
    rx.recv().await.unwrap(); // Consume start event
    
    // End with error
    let error = Err(icn_core_vm::VmError::ResourceLimitExceeded("Out of compute".into()));
    monitor.end_execution(execution_id, error);
    
    // Receive the event
    let event = rx.recv().await.unwrap();
    match event {
        MonitorEvent::ExecutionCompleted { execution_id: id, success, error_type, .. } => {
            assert_eq!(id, execution_id);
            assert!(!success);
            assert!(error_type.unwrap().contains("ResourceLimitExceeded"));
        }
        _ => panic!("Unexpected event type: {:?}", event),
    }
    
    // Verify metrics
    let report = monitor.get_execution_report();
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].execution_id, execution_id);
    assert!(!report[0].success);
    assert!(report[0].error.is_some());
}

#[tokio::test]
async fn test_monitor_performance_impact() {
    // Create the monitor
    let (monitor, _rx) = RuntimeMonitor::new(Some("perf-federation".to_string()));
    
    // Measure monitoring overhead
    let start = Instant::now();
    let iterations = 1000;
    
    for i in 0..iterations {
        let execution_id = format!("perf-execution-{}", i);
        let identity_id = IdentityId::new(&format!("did:icn:test:perf{}", i));
        
        monitor.start_execution(&execution_id, &identity_id);
        monitor.record_resource_metering(&execution_id, ResourceType::Compute, 100, Duration::from_nanos(10));
        monitor.record_dag_anchoring(&execution_id, 
            &Cid::new_v1(0x55, cid::multihash::Code::Sha2_256.digest(execution_id.as_bytes())), 
            Duration::from_millis(1));
        monitor.end_execution(&execution_id, Ok(()));
    }
    
    let duration = start.elapsed();
    let per_op = duration.as_nanos() as f64 / (iterations as f64 * 4.0); // 4 operations per iteration
    
    println!("Monitor performance: {} ns per operation, {} operations per second", 
        per_op, 1_000_000_000.0 / per_op);
    
    // Ensure monitoring overhead is reasonable (less than 10μs per operation)
    assert!(per_op < 10_000.0);
} 