# Economic System

The Economic System of the ICN Runtime provides non-extractive, value-aligned resource allocation mechanisms that extend beyond conventional currency models. It focuses on capability representation, participatory allocation, and transparent resource stewardship.

## Core Concepts

### Scoped Resource Tokens

Scoped Resource Tokens (SRTs) are the fundamental unit of the ICN economic system:
- Tokens represent **capabilities and access rights**, not speculative value
- Each token is bound to a specific scope (Cooperative, Community, etc.)
- Tokens have concrete resource types (Compute, Storage, Labor, etc.)
- Token transfers occur within governance-defined rules

Key characteristics of SRTs:
- Non-speculative by design
- Actual resources back tokens (not artificial scarcity)
- Usage rights are time-bound and context-specific
- Tokens can be created, transferred, and burned within appropriate governance contexts

Resource types include:
- **Compute**: Processing capability
- **Storage**: Data storage capacity
- **Network**: Communication bandwidth
- **Labor**: Human work contributions
- **Access**: Rights to use specific resources
- **Custom**: Domain-specific resources defined by communities

### Resource Authorization

Resource Authorization is the mechanism by which token holders gain permission to use specific resources:
- Explicit authorization request creates a signed claim
- Resource providers validate authorization
- Usage is metered and tracked
- Expiration ensures resource recovery

### Economic Boundaries

The ICN economic system enforces clear boundaries:
- Tokens cannot be exchanged outside their scope without governance approval
- Extraction (value leaving the commons) requires explicit constitutional allowance
- External resource incorporation follows value-aligned protocols
- Speculative uses of tokens are structurally prevented

## Participatory Budgeting

### Budget Framework

Participatory budgeting forms the foundation of resource allocation in the ICN ecosystem:
- Democratic decision-making for resource distribution
- Transparent allocation processes
- Multi-phase deliberation and decision cycles
- Impact tracking and accountability

### Budget Process

The typical budget process includes:
1. **Proposal Phase**: Community members propose resource allocations
2. **Deliberation Phase**: Public discussion and proposal refinement
3. **Decision Phase**: Voting or consensus-based allocation decisions
4. **Implementation Phase**: Resource distribution and tracking
5. **Evaluation Phase**: Impact assessment and process improvement

### Budget Mechanisms

Various decision-making mechanisms are supported:
- **Quadratic Voting**: Optimized for preference strength expression
- **Consensus Process**: Focus on deep agreement and proposal refinement
- **Category Allocation**: Pre-defined minimums for essential functions
- **Delegation**: Trust-based decision authority transfer

### Budget Integration

Budgets are integrated with governance and operations:
- CCL templates define budget parameters
- DAG records capture allocation decisions
- Identity scopes determine participation rights
- Resource tokens implement allocation execution

## Treasury Operations

### Resource Pooling

Resource contributions are pooled according to governance rules:
- Cooperative members pool labor and capital resources
- Communities pool infrastructure and participation
- Federations facilitate cross-boundary resource sharing

### Value Accounting

Multi-dimensional value tracking goes beyond monetary metrics:
- Labor hours with skill/context weighting
- Infrastructure contribution with depreciation models
- Knowledge contributions with impact assessment
- Care work and maintenance properly valued

### External Resource Integration

The system provides clear paths for integrating external resources:
- Non-extractive investment mechanisms
- Value-aligned external partnerships
- Resource conversion with governance oversight
- Clear boundaries on external influence

## Metering System

### Resource Usage Tracking

The metering system provides accountability for resource usage:
- Fine-grained tracking of computational resources
- Labor contribution verification
- Storage and bandwidth monitoring
- API-based access to usage statistics

### Fair Resource Allocation

Fairness mechanisms ensure equitable distribution:
- Usage caps prevent monopolization
- Priority tiers for essential functions
- Dynamic scaling based on availability
- Circuit breakers for unexpected demands

### Accountability

The system ensures responsible resource usage:
- All consumption is attributable to identities
- Usage patterns are analyzable for optimization
- Governance can adjust allocation based on impact
- Historical usage informs future budgeting

## Integration with Other Systems

The Economic System integrates with:

### Governance Kernel
- Budget proposals flow through governance processes
- Economic policies are defined in CCL
- Constitutional constraints on economic activity
- Resource allocation through democratic mechanisms

### Identity System
- Resource tokens are bound to identity scopes
- Authorization requires identity verification
- Contribution tracking is identity-attributable
- Reputation affects economic participation rights

### DAG System
- Economic transactions are recorded in the DAG
- Token transfers have verifiable history
- Budget decisions are permanently recorded
- Resource authorizations are cryptographically verifiable

### Federation System
- Cross-community resource sharing
- Standardized resource definitions
- Federation-level commons management
- Economic solidarity mechanisms

## Technical Implementation

The Economic System is implemented with:
- Token representation using DAG entries
- Resource authorization with cryptographic signatures
- Metering through the Core VM's resource tracking
- Budgeting interfaces in CCL templates

## Development Roadmap

The Economic System development is prioritized in the following order:

1. Basic SRT (Scoped Resource Token) implementation
2. Resource metering and authorization system
3. Token transfer and validation logic
4. Participatory budgeting primitives
5. Cross-scope resource sharing protocols
6. Advanced economic governance tools

## Examples

Examples of Economic System operations:
- Creating a participatory budget for a community's computing resources
- Tracking and compensating labor contributions in a cooperative
- Authorizing limited resource usage to external collaborators
- Implementing a solidarity fund across federation members
- Dynamic resource reallocation during usage spikes 