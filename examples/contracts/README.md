# CCL Contract Examples

This directory contains example contracts in CCL (Cooperative Contract Language) JSON format.

## File Format

CCL contracts are represented as JSON files with the `.ccl.json` extension. The format directly maps to the `Contract` struct from `icn-ccl/src/ast.rs`.

### Contract Structure

```json
{
  "name": "ContractName",
  "participants": ["did:icn:z...", "did:icn:z..."],
  "currency": "hours",
  "state_vars": [
    {
      "name": "variable_name",
      "initial_value": { "Int": 0 }
    }
  ],
  "rules": [
    {
      "name": "rule_name",
      "params": ["param1", "param2"],
      "body": []
    }
  ],
  "triggers": []
}
```

## Example Contracts

### 1. TimeBank (timebank.ccl.json)
Mutual credit timebank - exchange hours of service.

### 2. Simple Agreement (simple-agreement.ccl.json)
Basic state management contract.

### 3. Calculator (calculator.ccl.json)
Stateless computational contract.

## Deployment

1. Create contract in `.ccl.json` format
2. All participants sign deployment  
3. Deploy: `icnctl contract deploy contract.ccl.json`
4. Execute: `icnctl contract call <hash> <rule> <args>`

See full documentation in docs/CCL-FORMAT.md
