# ICN Runtime Events and Credentials

This document describes the events and Verifiable Credentials emitted by the ICN Runtime. It serves as a reference for components that consume these events, such as AgoraNet and wallet implementations.

## Event Architecture

The ICN Runtime emits events at key points in its operation. These events are broadcast to registered listeners and contain information about important occurrences within the system.

Events are accompanied by Verifiable Credentials (VCs) which provide cryptographic proof of the event's authenticity, allowing third parties to verify that the event was genuinely emitted by the ICN Runtime.

### Event Flow

```
┌────────────────┐    ┌────────────────┐    ┌────────────────┐
│                │    │                │    │                │
│  ICN Runtime   │───▶│  Event Emitter │───▶│  Subscribers   │
│                │    │                │    │ (e.g. AgoraNet)│
└────────────────┘    └────────────────┘    └────────────────┘
        │                                            │
        │                                            │
        ▼                                            ▼
┌────────────────┐                         ┌────────────────┐
│                │                         │                │
│ VC Credentials │                         │ Event Storage  │
│                │                         │                │
└────────────────┘                         └────────────────┘
```

## Event Types

The ICN Runtime emits the following types of events:

### Governance Events

| Event Type | Description | 
|------------|-------------|
| `ProposalCreated` | A new governance proposal has been created |
| `VoteCast` | A vote has been cast on a proposal |
| `ProposalFinalized` | A proposal has completed its voting period and been finalized |
| `ProposalExecuted` | A proposal has been executed |
| `MandateIssued` | A guardian mandate has been issued |

### Federation Events

| Event Type | Description |
|------------|-------------|
| `TrustBundleCreated` | A new trust bundle has been created |
| `TrustBundleUpdated` | A trust bundle has been updated |

### DAG Events

| Event Type | Description |
|------------|-------------|
| `DagNodeCreated` | A new node has been added to the DAG |
| `DagNodeUpdated` | A DAG node has been updated |

## Event Structure

All events have the following common structure:

```json
{
  "id": "unique-event-id-uuid",
  "eventType": "EventType",
  "timestamp": 1671234567,
  "issuer": "did:icn:issuer-identity",
  "scope": "Federation|Community|Other",
  "organization": "did:icn:organization-identity",  // Optional
  "proposalCid": "bafyrei...",  // Optional, CID of associated proposal
  "status": "Success|Failed|Pending",
  "data": {
    // Event-specific data fields
  }
}
```

### Event Field Definitions

| Field | Type | Description |
|-------|------|-------------|
| `id` | String (UUID) | Unique identifier for the event |
| `eventType` | String | Type of event from the event types listed above |
| `timestamp` | Integer | Unix timestamp (seconds since epoch) when the event was created |
| `issuer` | String (DID) | The DID of the identity that issued the event |
| `scope` | String | The scope within which the event applies |
| `organization` | String (DID) | Optional. The DID of the organization this event relates to |
| `proposalCid` | String (CID) | Optional. The content identifier of the proposal this event relates to |
| `status` | String | Status of the event: Success, Failed, or Pending |
| `data` | Object | Event-specific data payload |

## Event Details

### ProposalCreated

Emitted when a new governance proposal is created.

**Data fields:**

```json
{
  "title": "Proposal Title",
  "description": "Proposal description text",
  "proposer": "did:icn:proposer-identity",
  "votingPeriod": 86400,
  "templateCid": "bafyrei...",  // Optional, CID of the proposal template
}
```

### VoteCast

Emitted when a vote is cast on a proposal.

**Data fields:**

```json
{
  "voter": "did:icn:voter-identity",
  "choice": "For|Against|Abstain",
  "reason": "Reason for voting this way",  // Optional
  "weight": 1  // Optional, voting weight if applicable
}
```

### ProposalFinalized

Emitted when a proposal's voting period ends and the outcome is determined.

**Data fields:**

```json
{
  "outcome": "Passed|Rejected|Canceled",
  "forVotes": 10,
  "againstVotes": 5,
  "abstainVotes": 1,
  "finalTally": {
    "result": "Passed",
    "threshold": "50%",
    "quorum": "25%",
    "totalEligibleVotes": 20
  }
}
```

### ProposalExecuted

Emitted when a passed proposal is executed.

**Data fields:**

```json
{
  "executor": "did:icn:executor-identity",
  "executionTime": 1671234569,
  "executionOutcome": "Success|Failed",
  "resultCid": "bafyrei...",  // Optional, CID of execution result
  "errorDetails": "Error message"  // Only present if execution failed
}
```

### MandateIssued

Emitted when a guardian issues a mandate.

**Data fields:**

```json
{
  "guardian": "did:icn:guardian-identity",
  "action": "PauseProposals|FreezeAssets|Other",
  "reason": "Reason for the mandate",
  "scope": "Federation|Community|Other",
  "scopeId": "did:icn:target-scope",
  "quorumProof": "base64-encoded-proof",
  "dagNodeCid": "bafyrei..."
}
```

### TrustBundleCreated / TrustBundleUpdated

Emitted when a trust bundle is created or updated.

**Data fields:**

```json
{
  "epochId": 42,
  "bundleCid": "bafyrei...",
  "validFrom": 1671234567,
  "validUntil": 1671320967,
  "nodeCount": 5
}
```

## Verifiable Credentials

For each event, the ICN Runtime creates a Verifiable Credential that cryptographically attests to the authenticity of the event. These credentials follow the W3C Verifiable Credentials Data Model.

### Credential Structure

```json
{
  "@context": [
    "https://www.w3.org/2018/credentials/v1",
    "https://identity.foundation/JWK2020/contexts/jwk-2020-v1.json",
    "https://icn.network/2023/credentials/governance/v1"
  ],
  "id": "urn:uuid:credential-uuid",
  "type": [
    "VerifiableCredential",
    "GovernanceCredential",
    "<EventType>Credential"
  ],
  "issuer": "did:icn:runtime-identity",
  "issuanceDate": "2023-12-15T12:34:56Z",
  "credentialSubject": {
    "id": "did:icn:subject-identity",
    "eventId": "event-uuid",
    "eventType": "EventType",
    "timestamp": 1671234567,
    "issuerId": "did:icn:issuer-identity",
    "scope": "Federation",
    "organizationId": "did:icn:organization-identity",  // Optional
    "proposalCid": "bafyrei...",  // Optional
    "eventData": {
      // Event-specific data (same as event data fields)
    }
  },
  "proof": {
    "type": "JwtProof2020",
    "jwt": "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9..."
  }
}
```

### Credential Types

Each event type has a corresponding credential type:

| Event Type | Credential Type |
|------------|----------------|
| `ProposalCreated` | `ProposalCreationCredential` |
| `VoteCast` | `VoteCastCredential` |
| `ProposalFinalized` | `ProposalFinalizationCredential` |
| `ProposalExecuted` | `ProposalExecutionCredential` |
| `MandateIssued` | `MandateIssuanceCredential` |
| `TrustBundleCreated` | `TrustBundleCreationCredential` |
| `TrustBundleUpdated` | `TrustBundleUpdateCredential` |

## Usage for Integration Partners

### AgoraNet Integration

AgoraNet can subscribe to events and store them in its database:

```rust
// Example code for AgoraNet to process events
let governance_event = event_receiver.receive().await;
agoranet.register_governance_event(&governance_event).await?;

// Store credentials for verification
let credentials = runtime.get_credentials_for_event(governance_event.id).await?;
agoranet.store_credentials(credentials).await?;
```

### Wallet Integration

Wallets can verify credentials received from AgoraNet:

```rust
// Example wallet verification code
let credential = receive_credential_from_agoranet().await;

// Verify the credential
let is_valid = wallet.verify_credential(&credential).await?;

// Extract event data
if is_valid {
    let event_data = credential.credential_subject.event_data;
    // Process based on event type
    match credential.credential_subject.event_type {
        "ProposalCreated" => handle_new_proposal(event_data),
        "VoteCast" => update_vote_tally(event_data),
        // ...other event types
    }
}
```

## Further Reading

For more details on:
- [ICN Identity System](./IDENTITY_SYSTEM.md)
- [ICN Governance Kernel](./GOVERNANCE_KERNEL.md)
- [ICN Federation Protocol](./FEDERATION_PROTOCOL.md) 