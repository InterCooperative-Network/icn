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

## Executing Contracts via Compute Layer

CCL contracts are executed via the distributed compute layer. You can submit tasks via:

### CLI

```bash
# Submit contract for execution
icnctl compute submit --contract calculator.ccl.json --fuel 10000

# With inputs
icnctl compute submit --contract timebank.ccl.json --fuel 10000 \
    --input '{"from": "did:icn:alice", "to": "did:icn:bob", "hours": 2}'

# Check status
icnctl compute status <task_hash>
```

### TypeScript SDK

```typescript
import { ICNClient } from '@icn/client';
import * as fs from 'fs';

const client = new ICNClient({ baseUrl: 'http://localhost:8080' });
await client.authenticate('did:icn:...', signer, 'my-coop', ['compute:write']);

// Load and submit contract
const contract = JSON.parse(fs.readFileSync('timebank.ccl.json', 'utf8'));
const { task_hash } = await client.submitTask({
    code: JSON.stringify(contract),
    fuel_limit: 10000,
    inputs: { from: 'did:icn:alice', to: 'did:icn:bob', hours: 2 }
});

// Wait for result
const result = await client.waitForTask(task_hash);
console.log('Output:', result.result?.output);
```

### REST API

```bash
# Submit contract
CONTRACT=$(cat calculator.ccl.json)
curl -X POST http://localhost:8080/v1/compute/submit \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"code\": $(echo $CONTRACT | jq -Rs .), \"fuel_limit\": 10000}"
```

## CCL AST Reference

See the CCL crate source at [icn/crates/icn-ccl/src/ast.rs](../../icn/crates/icn-ccl/src/ast.rs) for the full AST definition.
