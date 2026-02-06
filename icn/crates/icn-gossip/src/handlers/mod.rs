//! Message handlers for GossipActor
//!
//! This module contains extracted message handler implementations,
//! organized by protocol category:
//!
//! - `dispatch`: Shared message dispatch logic (single source of truth)
//! - `push`: Announce, Request, Response (push protocol)
//! - `bloom`: RequestBloomFilter, SendBloomFilter (bloom filter sync)
//! - `pull`: Digest, PullRequest, PullResponse, RequestMissing (pull protocol)
//! - `replica`: ReplicaRequest, ReplicaOffer, ReplicaStatus (Phase 17)
//! - `partition`: PartitionHealRequest, PartitionHealResponse (Phase 18)
//! - `storage_challenge`: StorageChallengeMsg, StorageProofMsg (proof-of-storage)
//! - `blob_transfer`: BlobRequest, BlobTransferChunk (Pilot blob protocol)

pub mod blob_transfer;
mod bloom;
mod dispatch;
mod partition;
mod pull;
mod push;
mod replica;
mod storage_challenge;

// Re-export dispatch types for use in protocol.rs
pub use dispatch::DispatchResult;
