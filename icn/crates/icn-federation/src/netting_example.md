# Multilateral Netting in ICN

## Overview

The ICN clearing house implements both bilateral and multilateral netting to optimize inter-cooperative settlements.

## Bilateral Netting

When two cooperatives trade frequently, their mutual debts can be netted:

```
Before: A owes B $100, B owes A $30
After:  A owes B $70
```

## Multilateral Netting (Cycle Detection)

When three or more cooperatives have circular debts, these can be netted across the cycle:

```
Before:
  A owes B $100
  B owes C $80
  C owes A $60

After (60 canceled from cycle):
  A owes B $40
  B owes C $20
  C owes A $0
```

## Algorithm

The netting engine uses depth-first search (DFS) to detect cycles in the debt graph:

1. Build directed graph: nodes are cooperatives, edges are debts
2. Perform bilateral netting first (A ↔ B simplification)
3. Find cycles using DFS from each node
4. For each cycle, find the minimum debt and cancel it
5. Repeat until no more cycles exist

## Example Usage

```rust
use icn_federation::{ClearingManager, netting::NettingEngine};

// Create a clearing manager
let manager = ClearingManager::new(store, "my-coop".to_string())?;

// After creating agreements and accumulating positions...

// Perform multilateral netting for USD currency
let result = manager.perform_multilateral_netting("USD")?;

println!("Cycles canceled: {}", result.cycles_canceled.len());
println!("Total reduced: ${}", result.amount_reduced);

// The result contains:
// - original: All obligations before netting
// - netted: Simplified obligations after netting  
// - cycles_canceled: Details of each cycle found
// - amount_reduced: Total amount eliminated
```

## Benefits

1. **Reduces transaction volume**: Fewer actual transfers needed
2. **Lower credit exposure**: Less outstanding debt at any given time
3. **Fairness**: Automatic optimization across all participants
4. **Transparency**: All cycles and reductions are logged

## Limitations

- Only works within a single currency
- Requires signed clearing agreements between all parties
- Computationally expensive for very large networks (O(V×E) worst case)
- Does not handle disputed or frozen positions
