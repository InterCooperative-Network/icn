#!/bin/bash
# create-proposal.sh - Generate proposal templates for the ICN
# Creates proposal drafts from predefined templates in config/proposals/

set -e

# Configuration
CONFIG_DIR="./config/proposals"
DRAFTS_DIR="./drafts"

# Ensure directories exist
mkdir -p "$CONFIG_DIR" "$DRAFTS_DIR"

# Usage information
show_usage() {
  echo "Usage: $0 --type <proposal-type> --output <output-file> [options]"
  echo ""
  echo "Options:"
  echo "  --type, -t        Type of proposal (parameter-change, budget-allocation, "
  echo "                    federation-join, emergency-proposal)"
  echo "  --output, -o      Output file path (default: drafts/<type>-<timestamp>.dsl)"
  echo "  --title           Title for the proposal"
  echo "  --description     Description for the proposal"
  echo "  --param           Parameter name (for parameter-change)"
  echo "  --value           New value (for parameter-change)"
  echo "  --amount          Amount (for budget-allocation)"
  echo "  --recipient       Recipient (for budget-allocation)"
  echo "  --federation-id   Federation ID (for federation-join)"
  echo "  --emergency       Emergency details (for emergency-proposal)"
  echo "  --help, -h        Show this help message"
  echo ""
  echo "Examples:"
  echo "  $0 --type parameter-change --param max_validators --value 100"
  echo "  $0 --type budget-allocation --amount 500 --recipient did:icn:abc123"
  echo "  $0 --type federation-join --federation-id federation.example.org"
  echo "  $0 --type emergency-proposal --emergency \"Security patch required\""
  exit 1
}

# Check for required argument
if [ "$#" -lt 1 ]; then
  show_usage
fi

# Parse arguments
PROPOSAL_TYPE=""
OUTPUT_FILE=""
TITLE=""
DESCRIPTION=""
PARAM_NAME=""
PARAM_VALUE=""
AMOUNT=""
RECIPIENT=""
FEDERATION_ID=""
EMERGENCY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --type|-t)
      PROPOSAL_TYPE="$2"
      shift 2
      ;;
    --output|-o)
      OUTPUT_FILE="$2"
      shift 2
      ;;
    --title)
      TITLE="$2"
      shift 2
      ;;
    --description)
      DESCRIPTION="$2"
      shift 2
      ;;
    --param)
      PARAM_NAME="$2"
      shift 2
      ;;
    --value)
      PARAM_VALUE="$2"
      shift 2
      ;;
    --amount)
      AMOUNT="$2"
      shift 2
      ;;
    --recipient)
      RECIPIENT="$2"
      shift 2
      ;;
    --federation-id)
      FEDERATION_ID="$2"
      shift 2
      ;;
    --emergency)
      EMERGENCY="$2"
      shift 2
      ;;
    --help|-h)
      show_usage
      ;;
    *)
      echo "Unknown option: $1"
      show_usage
      ;;
  esac
done

# Validate proposal type
valid_types=("parameter-change" "budget-allocation" "federation-join" "emergency-proposal")
type_valid=false

for valid_type in "${valid_types[@]}"; do
  if [ "$PROPOSAL_TYPE" == "$valid_type" ]; then
    type_valid=true
    break
  fi
done

if [ "$type_valid" != true ]; then
  echo "Error: Invalid proposal type '$PROPOSAL_TYPE'"
  echo "Valid types are: ${valid_types[*]}"
  exit 1
fi

# Check if template exists
TEMPLATE_FILE="$CONFIG_DIR/$PROPOSAL_TYPE.dsl"
if [ ! -f "$TEMPLATE_FILE" ]; then
  # Create template directory if it doesn't exist
  mkdir -p "$CONFIG_DIR"
  
  # Generate template based on type
  case "$PROPOSAL_TYPE" in
    parameter-change)
      cat > "$TEMPLATE_FILE" << EOF
// Title: Parameter Change Proposal: {{PARAM_NAME}}
// Description: {{DESCRIPTION}}
// Author: {{AUTHOR}}
// Date: {{DATE}}

parameter_change {
  param: "{{PARAM_NAME}}",
  current_value: "{{CURRENT_VALUE}}",
  new_value: "{{NEW_VALUE}}",
  reason: "{{REASON}}",
  
  // Validation check
  validate: |
    (ctx) => {
      // Add validation logic here
      return true;
    }
}

// Implementation
execute: |
  (ctx) => {
    const param = ctx.params.param;
    const value = ctx.params.new_value;
    ctx.state.setParameter(param, value);
    return { success: true, message: \`Parameter \${param} changed to \${value}\` };
  }
EOF
      ;;
    budget-allocation)
      cat > "$TEMPLATE_FILE" << EOF
// Title: Budget Allocation Proposal: {{TITLE}}
// Description: {{DESCRIPTION}}
// Author: {{AUTHOR}}
// Date: {{DATE}}

budget_allocation {
  amount: {{AMOUNT}},
  token: "ICN",
  recipient: "{{RECIPIENT}}",
  purpose: "{{PURPOSE}}",
  timeframe: "{{TIMEFRAME}}",
  
  // Optional milestones
  milestones: [
    { description: "{{MILESTONE1}}", percentage: {{PERCENTAGE1}} },
    { description: "{{MILESTONE2}}", percentage: {{PERCENTAGE2}} }
  ],
  
  // Validation check
  validate: |
    (ctx) => {
      // Check if sufficient funds available
      const balance = ctx.state.getBalance("treasury");
      return balance >= ctx.params.amount;
    }
}

// Implementation
execute: |
  (ctx) => {
    const { amount, recipient, token } = ctx.params;
    ctx.state.transfer(token, "treasury", recipient, amount);
    return { success: true, message: \`Transferred \${amount} \${token} to \${recipient}\` };
  }
EOF
      ;;
    federation-join)
      cat > "$TEMPLATE_FILE" << EOF
// Title: Federation Join Request: {{FEDERATION_ID}}
// Description: {{DESCRIPTION}}
// Author: {{AUTHOR}}
// Date: {{DATE}}

federation_join {
  federation_id: "{{FEDERATION_ID}}",
  node_did: "{{NODE_DID}}",
  node_url: "{{NODE_URL}}",
  dns_record: "{{DNS_RECORD}}",
  public_key: "{{PUBLIC_KEY}}",
  
  // Federation capabilities
  capabilities: [
    {{CAPABILITIES}}
  ],
  
  // Validation check
  validate: |
    (ctx) => {
      // Verify the node is not already part of federation
      const existing = ctx.state.getFederationNodes();
      return !existing.includes(ctx.params.node_did);
    }
}

// Implementation
execute: |
  (ctx) => {
    const { node_did, federation_id, node_url } = ctx.params;
    ctx.state.registerFederationNode(node_did, federation_id, node_url);
    return { 
      success: true, 
      message: \`Node \${node_did} joined federation \${federation_id}\` 
    };
  }
EOF
      ;;
    emergency-proposal)
      cat > "$TEMPLATE_FILE" << EOF
// Title: EMERGENCY: {{TITLE}}
// Description: {{DESCRIPTION}}
// Author: {{AUTHOR}}
// Date: {{DATE}}

emergency_proposal {
  issue: "{{ISSUE}}",
  severity: "{{SEVERITY}}",
  affected_systems: [
    {{AFFECTED_SYSTEMS}}
  ],
  mitigation: "{{MITIGATION}}",
  
  // Fast-track voting parameters
  fast_track: true,
  voting_period_hours: 24,
  
  // Validation check
  validate: |
    (ctx) => {
      // Verify the caller has emergency authority
      return ctx.state.hasEmergencyAuthority(ctx.caller);
    }
}

// Implementation
execute: |
  (ctx) => {
    const { mitigation, affected_systems } = ctx.params;
    
    // Apply emergency changes
    for (const system of affected_systems) {
      ctx.state.applyEmergencyPatch(system, mitigation);
    }
    
    return { 
      success: true, 
      message: \`Emergency action applied to \${affected_systems.join(', ')}\` 
    };
  }
EOF
      ;;
  esac
  
  echo "Created template for $PROPOSAL_TYPE at $TEMPLATE_FILE"
fi

# Generate output filename if not provided
if [ -z "$OUTPUT_FILE" ]; then
  TIMESTAMP=$(date +%Y%m%d%H%M%S)
  OUTPUT_FILE="$DRAFTS_DIR/${PROPOSAL_TYPE}-${TIMESTAMP}.dsl"
fi

# Create output directory if it doesn't exist
mkdir -p "$(dirname "$OUTPUT_FILE")"

# Read the template
TEMPLATE=$(cat "$TEMPLATE_FILE")

# Apply common replacements
CURRENT_DATE=$(date +"%Y-%m-%d")
AUTHOR=$(./icn-wallet whoami 2>/dev/null | grep "DID:" | cut -d' ' -f2 || echo "{{YOUR_DID}}")

# Replace common placeholders
TEMPLATE=${TEMPLATE//\{\{DATE\}\}/$CURRENT_DATE}
TEMPLATE=${TEMPLATE//\{\{AUTHOR\}\}/$AUTHOR}

if [ -n "$TITLE" ]; then
  TEMPLATE=${TEMPLATE//\{\{TITLE\}\}/$TITLE}
fi

if [ -n "$DESCRIPTION" ]; then
  TEMPLATE=${TEMPLATE//\{\{DESCRIPTION\}\}/$DESCRIPTION}
else
  TEMPLATE=${TEMPLATE//\{\{DESCRIPTION\}\}/"Proposal description goes here"}
fi

# Apply type-specific replacements
case "$PROPOSAL_TYPE" in
  parameter-change)
    if [ -n "$PARAM_NAME" ]; then
      TEMPLATE=${TEMPLATE//\{\{PARAM_NAME\}\}/$PARAM_NAME}
    fi
    if [ -n "$PARAM_VALUE" ]; then
      TEMPLATE=${TEMPLATE//\{\{NEW_VALUE\}\}/$PARAM_VALUE}
    fi
    # Get current value if possible
    CURRENT_VALUE=$(./icn-wallet query parameter --name "$PARAM_NAME" 2>/dev/null || echo "{{CURRENT_VALUE}}")
    TEMPLATE=${TEMPLATE//\{\{CURRENT_VALUE\}\}/$CURRENT_VALUE}
    TEMPLATE=${TEMPLATE//\{\{REASON\}\}/"Reason for change goes here"}
    ;;
  budget-allocation)
    if [ -n "$AMOUNT" ]; then
      TEMPLATE=${TEMPLATE//\{\{AMOUNT\}\}/$AMOUNT}
    fi
    if [ -n "$RECIPIENT" ]; then
      TEMPLATE=${TEMPLATE//\{\{RECIPIENT\}\}/$RECIPIENT}
    fi
    TEMPLATE=${TEMPLATE//\{\{PURPOSE\}\}/"Purpose of funding goes here"}
    TEMPLATE=${TEMPLATE//\{\{TIMEFRAME\}\}/"Timeframe for the budget"}
    TEMPLATE=${TEMPLATE//\{\{MILESTONE1\}\}/"First milestone"}
    TEMPLATE=${TEMPLATE//\{\{PERCENTAGE1\}\}/25}
    TEMPLATE=${TEMPLATE//\{\{MILESTONE2\}\}/"Second milestone"}
    TEMPLATE=${TEMPLATE//\{\{PERCENTAGE2\}\}/75}
    ;;
  federation-join)
    if [ -n "$FEDERATION_ID" ]; then
      TEMPLATE=${TEMPLATE//\{\{FEDERATION_ID\}\}/$FEDERATION_ID}
    fi
    # Get node DID if possible
    NODE_DID=$(./icn-wallet whoami 2>/dev/null | grep "DID:" | cut -d' ' -f2 || echo "{{NODE_DID}}")
    TEMPLATE=${TEMPLATE//\{\{NODE_DID\}\}/$NODE_DID}
    TEMPLATE=${TEMPLATE//\{\{NODE_URL\}\}/"https://node.example.org"}
    TEMPLATE=${TEMPLATE//\{\{DNS_RECORD\}\}/"node.example.org"}
    TEMPLATE=${TEMPLATE//\{\{PUBLIC_KEY\}\}/"{{PUBLIC_KEY}}"}
    TEMPLATE=${TEMPLATE//\{\{CAPABILITIES\}\}/"\"storage\", \"compute\", \"governance\""}
    ;;
  emergency-proposal)
    if [ -n "$EMERGENCY" ]; then
      TEMPLATE=${TEMPLATE//\{\{ISSUE\}\}/$EMERGENCY}
    fi
    TEMPLATE=${TEMPLATE//\{\{SEVERITY\}\}/"critical"}
    TEMPLATE=${TEMPLATE//\{\{AFFECTED_SYSTEMS\}\}/"\"governance\", \"identity\""}
    TEMPLATE=${TEMPLATE//\{\{MITIGATION\}\}/"Proposed solution goes here"}
    ;;
esac

# Write the proposal to the output file
echo "$TEMPLATE" > "$OUTPUT_FILE"
echo "Proposal template generated at: $OUTPUT_FILE"
echo "Edit the file to complete the proposal, then move it to the $DRAFTS_DIR directory when ready."

# Optional: Open the file in an editor if EDITOR is set
if [ -n "$EDITOR" ]; then
  $EDITOR "$OUTPUT_FILE"
fi 