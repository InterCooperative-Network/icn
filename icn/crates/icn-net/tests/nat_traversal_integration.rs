//! Integration test for NAT traversal candidate exchange
//!
//! This test validates the connection candidate flow:
//! 1. Node creates ConnectionCandidate with STUN-discovered address
//! 2. Candidate is cached with TTL validation
//! 3. Connection attempts use cached candidates
//!
//! Note: Full NAT hole punching requires network access and is better tested
//! via manual testing or deployment. This test focuses on the data flow.

use anyhow::Result;
use icn_identity::KeyPair;
use icn_net::{CandidateCache, ConnectionCandidate};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_candidate_cache_flow() -> Result<()> {
    // Create two test identities
    let keypair1 = KeyPair::generate()?;
    let keypair2 = KeyPair::generate()?;
    let did1 = keypair1.did().clone();
    let did2 = keypair2.did().clone();

    // Create candidate cache (as supervisor does)
    let cache = CandidateCache::new();

    // Node 1 creates and publishes its candidate
    let candidate1 = ConnectionCandidate::new(
        did1.clone(),
        "192.168.1.100:5000".parse()?,
        Some("203.0.113.5:5000".parse()?), // STUN-discovered public address
        None,
    );

    // Node 2 creates and publishes its candidate
    let candidate2 = ConnectionCandidate::new(
        did2.clone(),
        "192.168.1.101:5001".parse()?,
        Some("203.0.113.6:5001".parse()?),
        None,
    );

    // Simulate Node 1 receiving Node 2's candidate (via gossip)
    assert!(cache.store(candidate2.clone()).await);

    // Verify candidate was stored
    let retrieved = cache.get(&did2).await;
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.did, did2);
    assert_eq!(retrieved.local_addr, candidate2.local_addr);
    assert_eq!(retrieved.public_addr, candidate2.public_addr);

    // Simulate Node 2 receiving Node 1's candidate
    assert!(cache.store(candidate1.clone()).await);

    // Verify both candidates are cached
    assert_eq!(cache.len().await, 2);

    // At this point, both nodes have each other's candidates
    // The supervisor would now attempt connections using:
    // 1. Try local_addr first (LAN)
    // 2. Try public_addr if local fails (NAT hole punching)
    // 3. Try relay_addr if both fail (Phase 4: TURN)

    // Verify candidates remain fresh
    let candidate1_retrieved = cache.get(&did1).await;
    assert!(candidate1_retrieved.is_some());
    assert!(candidate1_retrieved.unwrap().is_fresh(300));

    let candidate2_retrieved = cache.get(&did2).await;
    assert!(candidate2_retrieved.is_some());
    assert!(candidate2_retrieved.unwrap().is_fresh(300));

    Ok(())
}

#[tokio::test]
async fn test_stale_candidate_rejection() -> Result<()> {
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();

    // Create cache with short TTL (1 second)
    let cache = CandidateCache::with_ttl(1);

    // Create fresh candidate
    let candidate = ConnectionCandidate::new(
        did.clone(),
        "192.168.1.100:5000".parse()?,
        Some("203.0.113.5:5000".parse()?),
        None,
    );

    // Store fresh candidate
    assert!(cache.store(candidate.clone()).await);
    assert_eq!(cache.len().await, 1);

    // Wait for candidate to become stale
    sleep(Duration::from_secs(2)).await;

    // Verify stale candidate is not returned
    let retrieved = cache.get(&did).await;
    assert!(retrieved.is_none());

    // Verify cleanup removes stale entry
    let removed = cache.cleanup_expired().await;
    assert_eq!(removed, 1);
    assert_eq!(cache.len().await, 0);

    Ok(())
}

#[tokio::test]
async fn test_candidate_update_priority() -> Result<()> {
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();
    let cache = CandidateCache::new();

    // Store initial candidate
    let candidate1 = ConnectionCandidate::new(
        did.clone(),
        "192.168.1.100:5000".parse()?,
        None,
        None,
    );
    let timestamp1 = candidate1.timestamp;
    assert!(cache.store(candidate1.clone()).await);

    // Try to store older candidate (should be rejected)
    let mut older_candidate = ConnectionCandidate::new(
        did.clone(),
        "192.168.1.100:5001".parse()?,
        None,
        None,
    );
    older_candidate.timestamp = timestamp1 - 10; // 10 seconds older
    assert!(!cache.store(older_candidate).await);

    // Verify original candidate is still cached
    let retrieved = cache.get(&did).await.unwrap();
    assert_eq!(retrieved.local_addr, candidate1.local_addr);

    // Store newer candidate (should update)
    let mut newer_candidate = ConnectionCandidate::new(
        did.clone(),
        "192.168.1.100:5002".parse()?,
        Some("203.0.113.5:5002".parse()?),
        None,
    );
    newer_candidate.timestamp = timestamp1 + 10; // 10 seconds newer
    assert!(cache.store(newer_candidate.clone()).await);

    // Verify newer candidate replaced the old one
    let retrieved = cache.get(&did).await.unwrap();
    assert_eq!(retrieved.local_addr, newer_candidate.local_addr);
    assert_eq!(retrieved.public_addr, newer_candidate.public_addr);
    assert_eq!(cache.len().await, 1); // Still just one entry

    Ok(())
}

#[tokio::test]
async fn test_multiple_peer_candidates() -> Result<()> {
    let cache = CandidateCache::new();

    // Simulate receiving candidates from multiple peers
    let mut candidates = Vec::new();
    for i in 0..10 {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let candidate = ConnectionCandidate::new(
            did.clone(),
            format!("192.168.1.{}:5000", 100 + i).parse()?,
            Some(format!("203.0.113.{}:5000", 10 + i).parse()?),
            None,
        );
        candidates.push((did, candidate));
    }

    // Store all candidates
    for (_, candidate) in &candidates {
        assert!(cache.store(candidate.clone()).await);
    }

    // Verify all candidates are cached
    assert_eq!(cache.len().await, 10);

    // Verify each candidate can be retrieved
    for (did, original) in &candidates {
        let retrieved = cache.get(did).await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.did, *did);
        assert_eq!(retrieved.local_addr, original.local_addr);
        assert_eq!(retrieved.public_addr, original.public_addr);
    }

    Ok(())
}
