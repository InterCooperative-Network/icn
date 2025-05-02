# Identity System

The Identity System is the foundation of the ICN Runtime's trust model, providing verifiable, contextual, and privacy-preserving identities for all participants in the network.

## Core Concepts

### Decentralized Identifiers (DIDs)

The ICN Runtime uses Decentralized Identifiers (DIDs) as the primary mechanism for identity:
- Self-sovereign: Entities control their own identifiers
- Cryptographically verifiable: Based on public key cryptography
- Resolvable: Can be looked up to retrieve associated DID documents
- Persistent: Stable over time, even as control mechanisms change

### Identity Scopes

A key innovation in the ICN identity system is the concept of scoped identities, which contextualizes identity within specific governance domains:

#### Cooperative Scope

Identities in the Cooperative scope represent formal work organizations with democratic governance:
- Legally recognized cooperative entities
- Worker-owned and governed organizations
- Multi-stakeholder cooperatives
- Platform cooperatives

**Technical characteristics:**
- Requires multi-signature control
- Can issue member credentials
- Has governance authorities defined by bylaws
- Tracked in federation registries

#### Community Scope

Identities in the Community scope represent informal or commons-based governance structures:
- Neighborhood associations
- Open source communities
- Mutual aid networks
- Digital commons projects

**Technical characteristics:**
- More flexible governance structures
- Can issue membership and participation credentials
- Lighter verification requirements than Cooperatives
- Supports diverse decision-making protocols

#### Individual Scope

Identities in the Individual scope represent human participants:
- Members of cooperatives or communities
- Independent workers
- End users of cooperative services
- Contributors to community projects

**Technical characteristics:**
- Single-signature control
- Can receive credentials from Cooperatives and Communities
- Privacy-preserving capabilities
- Selective disclosure of attributes

#### Additional Scopes

Other important scopes include:
- **Federation**: Represents networks of Cooperatives and Communities
- **Node**: Represents infrastructure providers in the network
- **Guardian**: Represents constitution-enforcing entities

### Verifiable Credentials

Verifiable Credentials (VCs) are cryptographically signed attestations about identity attributes:
- Issued by trusted entities (Cooperatives, Communities, Federations, etc.)
- Held by the subject (often an Individual)
- Verifiable by any party without contacting the issuer
- Selective disclosure enabled through Zero-Knowledge proofs

Types of credentials in the ICN system include:
- Membership credentials
- Role credentials
- Reputation credentials
- Contribution records
- Skill attestations
- Delegation authorizations

### Trust Bundles

Trust Bundles are collections of cryptographically signed attestations that establish trust anchors across the federation:
- Include DAG roots for verifiable history
- Contain epoch information for time-based validation
- Signed by multiple federation participants
- Used for cross-community verification

### Anchor Credentials

Anchor Credentials link epochs to DAG roots, providing temporal context to identity operations:
- Define the authoritative state at a point in time
- Enable historical verification
- Support federation synchronization
- Used in consensus protocols

## Zero-Knowledge Capabilities

The ICN Identity system includes built-in support for Zero-Knowledge proofs:
- Selective disclosure of credential attributes
- Age verification without revealing birthdate
- Membership verification without revealing identity
- Threshold proofs (e.g., "at least 3 of 5 conditions are met")

## Multi-Context Identity

The ICN system is designed for individuals to maintain distinct but connected identities across contexts:
- Clear separation between roles in different Cooperatives
- Privacy-preserving connections between contexts
- Portable reputation with consent-based sharing
- Holistic identity without centralized tracking

## Reputation and Trust

The system includes mechanisms for contextual reputation:
- Contribution records attested by Cooperatives
- Skill endorsements with weighted trust
- Temporal decay of attestations for relevance
- Trust circles for local trust networks

## Guardian System

Guardians are specialized identity roles with constitutional enforcement capabilities:
- Limited-duration mandates for specific actions
- Quorum-based approval for interventions
- Transparent record of all guardian actions
- Constitutional constraints on guardian powers

## Technical Implementation

The Identity System is implemented using:
- DID Method Key for cryptographic operations
- JSON-LD for Verifiable Credentials
- DAG anchoring for temporal verification
- LibP2P for peer-to-peer identity operations

## Integration with Other Systems

The Identity System integrates with:

### Governance Kernel
- Identity scopes determine governance rights
- Credentials establish participation privileges
- Constitutional roles are expressed through identity

### DAG System
- Identity operations are recorded as DAG nodes
- Credential issuance is historically verifiable
- Identity changes maintain lineage attestations

### Economic System
- Resource tokens are bound to identities
- Authorization for resource usage is identity-based
- Budget participation rights tied to credentials

### Federation System
- Trust across boundaries established via credentials
- Multi-signature operations for federation actions
- Guardian mandates for cross-community governance

## Development Roadmap

The Identity System development is prioritized in the following order:

1. Basic DID implementation with key management
2. Scoped identity infrastructure (Coop/Community/Individual)
3. Verifiable Credential issuance and verification
4. Trust Bundle mechanics for federation
5. ZK-disclosure capabilities
6. Guardian mandate system
7. Advanced reputation and trust mechanisms

## Examples

Identity operations in the ICN Runtime include:
- Registering a new Cooperative with founding members
- Issuing membership credentials to Community participants
- Proving eligibility for proposal submission without revealing identity
- Establishing trust between communities for resource sharing
- Authorizing a Guardian intervention with federation quorum 