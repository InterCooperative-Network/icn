//! Message handlers for GossipActor
//!
//! This module contains extracted message handler implementations,
//! organized by protocol category:
//!
//! - `push`: Announce, Request, Response (push protocol)
//! - `bloom`: RequestBloomFilter, SendBloomFilter (bloom filter sync)
//! - `pull`: Digest, PullRequest, PullResponse, RequestMissing (pull protocol)
//! - `replica`: ReplicaRequest, ReplicaOffer, ReplicaStatus (Phase 17)
//! - `partition`: PartitionHealRequest, PartitionHealResponse (Phase 18)
//! - `storage_challenge`: StorageChallengeMsg, StorageProofMsg (proof-of-storage)

mod bloom;
mod partition;
mod pull;
mod push;
mod replica;
mod storage_challenge;

// All handlers are impl blocks on GossipActor, no re-exports needed
