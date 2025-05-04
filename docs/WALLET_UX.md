# ICN Wallet User Experience Specification

## Introduction

This document specifies the user experience design for the Intercooperative Network (ICN) wallet application. It covers interface design, user flows, security interactions, and integration guidelines to ensure a consistent, secure, and intuitive experience across implementations.

> **Related Documentation:**
> - [ARCHITECTURE.md](ARCHITECTURE.md) - Overall system architecture
> - [SECURITY.md](SECURITY.md) - Security model and threat mitigations
> - [INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md) - Integration guidance

## Core Design Principles

The ICN wallet adheres to the following UX design principles:

1. **Simplicity First**: Complex operations are abstracted into intuitive actions
2. **Progressive Disclosure**: Information and options revealed as needed
3. **Contextual Guidance**: Help and explanations provided in context
4. **Error Prevention**: Design that prevents errors before they occur
5. **Accessibility**: Inclusive design for users of all abilities
6. **Federation-Aware**: Clearly communicates cross-federation interactions

## Wallet Components & Views

### 1. Account Dashboard

The primary interface showing balances, recent transactions, and core actions:

```
┌─────────────────────────────────────────────────────────┐
│                     ICN Wallet                          │
├─────────────────────┬───────────────────────────────────┤
│                     │                                   │
│  Account Overview   │  Transaction History              │
│                     │                                   │
│  Total Balance:     │  • Payment to @alice             │
│  1,250 ICN          │    250 ICN - 2 minutes ago       │
│                     │                                   │
│  Federation: Coop1  │  • Received from @bob            │
│  Status: Connected  │    500 ICN - Yesterday           │
│                     │                                   │
│  Security Level:    │  • Federation Dividend           │
│  ●●●●○ (4/5)        │    50 ICN - 3 days ago           │
│                     │                                   │
├─────────────────────┴───────────────────────────────────┤
│                                                         │
│    Send    │    Receive    │    Swap    │    Stake      │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 2. Send Transaction Flow

Multi-step flow with progressive disclosure for transaction creation:

1. **Recipient Selection**:
   - Contact lookup (with federation context)
   - Recent recipients
   - Address input with validation

2. **Amount Configuration**:
   - Amount entry
   - Fee options (economy, standard, priority)
   - Currency conversion display

3. **Transaction Confirmation**:
   - Transaction summary
   - Fee breakdown
   - Recipient verification
   - Privacy implications

4. **Authentication**:
   - Biometric/PIN/password verification
   - Hardware wallet integration

5. **Transaction Success**:
   - Confirmation with transaction ID
   - Estimated finalization time
   - Share/save receipt option

```rust
pub struct SendTransactionFlow {
    // Current step in the flow
    pub current_step: SendFlowStep,
    
    // Transaction details being built
    pub transaction_draft: TransactionDraft,
    
    // Validation state for current step
    pub validation_state: ValidationState,
    
    // Fee estimates based on current network conditions
    pub fee_estimates: FeeEstimates,
    
    // Federation context for cross-federation transactions
    pub federation_context: Option<FederationContext>,
}

pub enum SendFlowStep {
    RecipientSelection,
    AmountConfiguration,
    TransactionConfirmation,
    Authentication,
    TransactionSuccess,
    TransactionError,
}
```

### 3. Receive Transaction View

Interface for receiving funds with contextual information:

```
┌─────────────────────────────────────────────────────────┐
│                   Receive Funds                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│                      [QR CODE]                          │
│                                                         │
│  Your Address: icn://coop1/user/alice.smith             │
│                                                         │
│  Federation: Coop1                                      │
│                                                         │
│  • Shareable Link                                       │
│  • Request Specific Amount                              │
│  • Cross-Federation Instructions                        │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 4. Transaction Details

Detailed view of transaction information with actions:

```rust
pub struct TransactionDetailsView {
    // Transaction data
    pub transaction: Transaction,
    
    // Confirmation status
    pub confirmation_status: ConfirmationStatus,
    
    // Related transactions (if applicable)
    pub related_transactions: Vec<TransactionSummary>,
    
    // Available actions for this transaction
    pub available_actions: Vec<TransactionAction>,
    
    // Display preferences
    pub display_preferences: TransactionDisplayPreferences,
}

pub enum TransactionAction {
    ViewOnExplorer,
    AddMemo,
    RepeatTransaction,
    ExportReceipt,
    DisputeTransaction,
    ViewPath,
}
```

### 5. Settings & Security

User-configurable wallet settings with security focus:

```
┌─────────────────────────────────────────────────────────┐
│                  Wallet Settings                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Security                                               │
│  • Authentication Method: [Biometric & PIN]             │
│  • Transaction Confirmations: [Always]                  │
│  • Automatic Locking: [After 5 minutes]                 │
│  • Trusted Devices                                      │
│                                                         │
│  Privacy                                                │
│  • Transaction History: [Stored Locally]                │
│  • Contact Management: [Local Only]                     │
│  • Analytics: [Disabled]                                │
│                                                         │
│  Federation                                             │
│  • Primary Federation: [Coop1]                          │
│  • Cross-Federation Permissions: [Ask Every Time]       │
│  • Guardian Settings                                    │
│                                                         │
│  Display & Notifications                                │
│  • Currency Display: [ICN & USD]                        │
│  • Theme: [System Default]                              │
│  • Notification Preferences                             │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## User Flow Specifications

### Onboarding Flow

First-time user experience with progressive education:

1. **Welcome & Introduction**:
   - Brief overview of ICN and federation concept
   - Privacy and security overview

2. **Account Creation Options**:
   - Create new account
   - Import existing account
   - Connect hardware wallet

3. **Federation Selection**:
   - Choose primary federation
   - Federation explainer
   - Cross-federation capabilities preview

4. **Security Setup**:
   - PIN/password creation
   - Biometric authentication setup (if available)
   - Recovery phrase generation & verification

5. **Feature Introduction**:
   - Guided tour of core features
   - Sample transaction demo (optional)
   - Customization options

```rust
pub struct OnboardingFlow {
    // Current onboarding step
    pub current_step: OnboardingStep,
    
    // User selections during onboarding
    pub user_selections: OnboardingSelections,
    
    // Completion status for each step
    pub step_completion: HashMap<OnboardingStep, CompletionStatus>,
    
    // Federation context
    pub federation_context: FederationContext,
}

pub enum OnboardingStep {
    Welcome,
    AccountCreation,
    FederationSelection,
    SecuritySetup,
    RecoveryPhraseGeneration,
    RecoveryPhraseVerification,
    FeatureIntroduction,
    Customization,
    Completion,
}
```

### Cross-Federation Transaction Flow

Special considerations for transactions across federations:

1. **Federation Selection**:
   - Federation browser/selector
   - Federation trust indicators

2. **Cross-Federation Context**:
   - Federation relationship indicators
   - Exchange rate information
   - Fee differences highlight

3. **Transaction Verification**:
   - Extended verification for cross-federation
   - Explainer for validation requirements
   - Estimated finalization time

4. **Completion Status**:
   - Dual-federation status indicators
   - Path visualization between federations

## Error Recovery Patterns

### Transaction Failures

Consistent patterns for handling transaction errors:

```
┌─────────────────────────────────────────────────────────┐
│              Transaction Failed                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ⚠️ Your transaction could not be completed             │
│                                                         │
│  Error: Insufficient funds for gas fees                 │
│                                                         │
│  Details:                                               │
│  • Transaction required: 0.05 ICN for fees              │
│  • Available balance: 0.03 ICN                          │
│                                                         │
│  Recommended Actions:                                   │
│  [Add Funds]  [Adjust Gas]  [Try Later]                │
│                                                         │
│  [View Detailed Error Information]                      │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Recovery & Backup

Interface for backup and recovery operations:

```rust
pub struct RecoveryFlow {
    // Recovery method
    pub recovery_method: RecoveryMethod,
    
    // Verification steps
    pub verification_steps: Vec<VerificationStep>,
    
    // Recovery progress
    pub recovery_progress: RecoveryProgress,
    
    // Federation context for recovery
    pub federation_context: Option<FederationContext>,
}

pub enum RecoveryMethod {
    RecoveryPhrase(RecoveryPhraseConfig),
    SocialRecovery(SocialRecoveryConfig),
    FederationGuardian(GuardianRecoveryConfig),
    BackupFile(BackupFileConfig),
}
```

## Accessibility Guidelines

### Visual Accessibility

```
┌─────────────────────────────────────────────────────────┐
│              Accessibility Features                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Text & Display                                         │
│  • Dynamic text sizing (16pt - 32pt)                    │
│  • High contrast mode                                   │
│  • Screen reader compatible labels                      │
│  • Color blind safe palette                             │
│                                                         │
│  Interaction                                            │
│  • Voice commands support                               │
│  • Gesture alternatives                                 │
│  • Extended timeout options                             │
│  • Alternative authentication methods                   │
│                                                         │
│  Cognitive                                              │
│  • Simplified view option                               │
│  • Step-by-step guides                                  │
│  • Critical action confirmations                        │
│  • Consistent navigation patterns                       │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Implementation Requirements

Minimum requirements for accessible implementation:

```typescript
interface AccessibilityRequirements {
  // Minimum touch target size
  minimumTouchTargetSize: {
    width: '44px',
    height: '44px',
  },
  
  // Minimum contrast ratios
  contrastRequirements: {
    normalText: 4.5,
    largeText: 3.0,
    uiComponents: 3.0,
  },
  
  // Animation controls
  animationRequirements: {
    respects_reduced_motion: true,
    maximum_flashing_frequency: '3Hz',
    pausable: true,
  },
  
  // Keyboard navigation
  keyboardNavigation: {
    tabIndex_properly_set: true,
    keyboard_focus_visible: true,
    logical_navigation_order: true,
    keyboard_shortcuts: true,
  },
}
```

## Wallet Integration Patterns

### Mobile App Integration

Guidelines for embedding wallet functionality in mobile apps:

```dart
// Flutter integration example
class ICNWalletIntegration {
  // Initialize wallet module
  Future<WalletStatus> initializeWallet({
    required FederationConfig federationConfig,
    required SecurityLevel securityLevel,
    required UiCustomization uiCustomization,
  }) async {
    // Implementation details
  }
  
  // Launch transaction flow
  Future<TransactionResult> launchTransactionFlow({
    required TransactionRequest request,
    required TransactionUIConfig uiConfig,
  }) async {
    // Implementation details
  }
  
  // Wallet widget for embedding
  Widget buildWalletWidget({
    required WalletViewConfig viewConfig,
    required Function(WalletEvent) onWalletEvent,
  }) {
    // Return embedded wallet widget
  }
}
```

### Web Integration

JavaScript API for integrating wallet functions into web applications:

```javascript
// ICN Wallet Web SDK
class ICNWalletSDK {
  constructor(config) {
    this.federationId = config.federationId;
    this.securityLevel = config.securityLevel;
    this.uiCustomization = config.uiCustomization;
  }
  
  // Connect wallet to application
  async connect() {
    // Implementation details
  }
  
  // Request transaction
  async requestTransaction(transactionDetails) {
    // Implementation details
  }
  
  // Embed wallet interface
  embedWalletInterface(containerElement, viewOptions) {
    // Implementation details
  }
  
  // Listen for wallet events
  onWalletEvent(eventType, callback) {
    // Implementation details
  }
}
```

## Federation-Specific UX Considerations

### Guardian Interaction Design

Interface for guardian interactions and recovery:

```
┌─────────────────────────────────────────────────────────┐
│              Guardian Management                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Active Guardians: 3 of 5                               │
│                                                         │
│  • Alice Smith (Family) - Last verified: 2 days ago     │
│    Status: Active ✓                                     │
│                                                         │
│  • Bob Johnson (Friend) - Last verified: 1 month ago    │
│    Status: Active ✓                                     │
│                                                         │
│  • Coop Treasury - Last verified: 1 week ago            │
│    Status: Active ✓                                     │
│                                                         │
│  • Dana White (Work) - Last verified: 3 months ago      │
│    Status: Verification Needed ⚠️                       │
│                                                         │
│  • Evan Brown (Friend) - Last verified: Never           │
│    Status: Pending Acceptance ⏱️                        │
│                                                         │
│  [Add Guardian]  [Verify Guardians]  [Recovery Options] │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Federation Governance Participation

Wallet interface for governance participation:

```rust
pub struct GovernanceView {
    // Active proposals
    pub active_proposals: Vec<GovernanceProposal>,
    
    // User's voting power
    pub voting_power: VotingPower,
    
    // Past votes
    pub past_votes: Vec<Vote>,
    
    // Delegation settings
    pub delegation: Option<DelegationSettings>,
    
    // Federation context
    pub federation_context: FederationContext,
}

pub struct GovernanceProposal {
    // Proposal identifier
    pub id: ProposalId,
    
    // Proposal title
    pub title: String,
    
    // Proposal summary
    pub summary: String,
    
    // Detailed description
    pub description: String,
    
    // Voting deadline
    pub deadline: DateTime<Utc>,
    
    // Current voting results
    pub current_results: VotingResults,
    
    // User's vote (if cast)
    pub user_vote: Option<Vote>,
}
```

## Biometric Authentication Flow

Secure biometric integration for transaction authentication:

```swift
// Swift example for iOS integration
class BiometricAuthenticationFlow {
    // Available biometric methods
    enum BiometricMethod {
        case faceID
        case touchID
        case none
    }
    
    // Authentication contexts
    enum AuthenticationContext {
        case appLogin
        case transaction(amount: Decimal, recipient: String)
        case sensitiveOperation(operationType: String)
        case federationChange
    }
    
    // Request authentication
    func requestAuthentication(
        for context: AuthenticationContext,
        fallbackMethod: FallbackMethod,
        timeout: TimeInterval
    ) -> BiometricAuthResult {
        // Implementation details
    }
    
    // Custom UI for biometric prompts
    func customizeBiometricPrompt(
        title: String,
        description: String,
        cancelButtonText: String
    ) {
        // Implementation details
    }
}
```

## Hardware Wallet Integration

Specifications for hardware wallet interaction flows:

```typescript
interface HardwareWalletIntegration {
  // Supported hardware wallet types
  supportedWallets: HardwareWalletType[];
  
  // Connection methods
  connectionMethods: {
    usb: boolean;
    bluetooth: boolean;
    nfc: boolean;
  };
  
  // Device connection flow
  connectDevice(options: ConnectionOptions): Promise<DeviceConnection>;
  
  // Transaction signing flow
  signTransaction(
    deviceConnection: DeviceConnection,
    transaction: TransactionRequest
  ): Promise<SignedTransaction>;
  
  // Device management functions
  deviceManagement: {
    checkFirmware(): Promise<FirmwareStatus>;
    updateFirmware(): Promise<FirmwareUpdateResult>;
    manageApps(): Promise<AppManagementResult>;
  };
  
  // Error handling
  handleDeviceError(error: HardwareWalletError): ErrorResolution;
}
```

## Offline Functionality

Wallet capabilities when operating in offline mode:

```
┌─────────────────────────────────────────────────────────┐
│                  Offline Mode                           │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ⚠️ You are currently in offline mode                   │
│                                                         │
│  Available Functions:                                   │
│  • View cached balances (as of 2 hours ago)             │
│  • Prepare transactions (will send when online)         │
│  • Generate receive addresses                           │
│  • Access backup & security features                    │
│                                                         │
│  Unavailable Functions:                                 │
│  • Send transactions                                    │
│  • Update balances                                      │
│  • Participate in governance                            │
│  • Cross-federation operations                          │
│                                                         │
│  [Check Connection]  [Offline Transaction]              │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Metrics & Analytics

Privacy-respecting analytics implementation:

```typescript
interface WalletAnalytics {
  // Analytics collection mode
  collectionMode: 'opt-in' | 'anonymous' | 'minimal' | 'none';
  
  // Collected metrics
  metrics: {
    // Performance metrics
    performance: {
      startup_time: boolean;
      transaction_completion_time: boolean;
      network_request_latency: boolean;
    },
    
    // Usage metrics (anonymized)
    usage: {
      feature_usage_frequency: boolean;
      session_duration: boolean;
      error_frequency: boolean;
    },
    
    // Federation metrics
    federation: {
      federation_distribution: boolean;
      cross_federation_frequency: boolean;
    }
  };
  
  // User controls
  userControls: {
    allow_opt_out: boolean;
    data_export: boolean;
    data_deletion: boolean;
  };
}
```

## Glossary

| Term | Definition |
|------|------------|
| **Address** | A human-readable identifier for receiving transactions in the ICN network. |
| **Biometric Authentication** | Use of biological traits (fingerprint, face, etc.) to verify user identity. |
| **Cross-Federation Transaction** | A transaction that occurs between users in different federations. |
| **Federation** | A cooperative group operating as a trust domain within the ICN network. |
| **Guardian** | A trusted entity that can help with account recovery and security operations. |
| **Hardware Wallet** | A physical device that securely stores cryptographic keys offline. |
| **Recovery Phrase** | A sequence of words that can be used to recover wallet access. |
| **Social Recovery** | A method of account recovery using trusted contacts instead of a recovery phrase. |
| **Transaction Fee** | Cost to process a transaction on the network. |
| **Wallet** | Software that manages keys and interfaces with the ICN network. |
</rewritten_file> 