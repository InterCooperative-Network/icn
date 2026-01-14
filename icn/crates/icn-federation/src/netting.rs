//! Multilateral Netting Algorithm
//!
//! Implements circular debt resolution for clearing houses.
//! Example: A owes B $100, B owes C $80, C owes A $60
//! After netting: A owes B $40, B owes C $20, C owes A $0

use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

/// A debt obligation between two parties
#[derive(Debug, Clone)]
pub struct Obligation {
    pub from: String,
    pub to: String,
    pub amount: i64,
    pub currency: String,
}

/// Result of multilateral netting
#[derive(Debug, Clone)]
pub struct NettingResult {
    /// Original obligations
    pub original: Vec<Obligation>,
    /// Netted obligations (after cycle cancellation)
    pub netted: Vec<Obligation>,
    /// Cycles that were found and canceled
    pub cycles_canceled: Vec<DebtCycle>,
    /// Total amount reduced through netting
    pub amount_reduced: i64,
}

/// A cycle of debts that can be canceled
#[derive(Debug, Clone)]
pub struct DebtCycle {
    /// Sequence of parties in the cycle (e.g., ["A", "B", "C", "A"])
    pub participants: Vec<String>,
    /// Amount that can be canceled (minimum in the cycle)
    pub amount: i64,
    /// Currency of the cycle
    pub currency: String,
}

/// Multilateral netting engine
pub struct NettingEngine {
    /// Graph of obligations: (from, to) -> amount
    graph: HashMap<(String, String), i64>,
    /// Currency for this netting session
    currency: String,
}

impl NettingEngine {
    /// Create a new netting engine for a specific currency
    pub fn new(currency: String) -> Self {
        Self {
            graph: HashMap::new(),
            currency,
        }
    }

    /// Add an obligation to the graph
    pub fn add_obligation(&mut self, from: String, to: String, amount: i64) {
        if amount == 0 {
            return;
        }

        let key = (from.clone(), to.clone());
        *self.graph.entry(key).or_insert(0) += amount;
    }

    /// Perform bilateral netting first (A owes B $100, B owes A $30 -> A owes B $70)
    fn bilateral_netting(&mut self) {
        let keys: Vec<_> = self.graph.keys().cloned().collect();

        for (from, to) in keys {
            let forward = self.graph.get(&(from.clone(), to.clone())).copied().unwrap_or(0);
            let reverse = self.graph.get(&(to.clone(), from.clone())).copied().unwrap_or(0);

            if forward > 0 && reverse > 0 {
                let net = forward - reverse;
                self.graph.remove(&(from.clone(), to.clone()));
                self.graph.remove(&(to.clone(), from.clone()));

                if net > 0 {
                    self.graph.insert((from, to), net);
                } else if net < 0 {
                    self.graph.insert((to, from), -net);
                }
            }
        }
    }

    /// Find all participants in the graph
    fn get_participants(&self) -> HashSet<String> {
        let mut participants = HashSet::new();
        for ((from, to), _) in &self.graph {
            participants.insert(from.clone());
            participants.insert(to.clone());
        }
        participants
    }

    /// Find a cycle starting from a given node using DFS
    fn find_cycle(&self, start: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let mut path_set = HashSet::new();

        self.dfs_find_cycle(start, &mut visited, &mut path, &mut path_set)
    }

    fn dfs_find_cycle(
        &self,
        current: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        path_set: &mut HashSet<String>,
    ) -> Option<Vec<String>> {
        if path_set.contains(current) {
            // Found a cycle
            let cycle_start = path.iter().position(|p| p == current)?;
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(current.to_string());
            return Some(cycle);
        }

        if visited.contains(current) {
            return None;
        }

        visited.insert(current.to_string());
        path.push(current.to_string());
        path_set.insert(current.to_string());

        // Find all neighbors (people current owes money to)
        for ((from, to), amount) in &self.graph {
            if from == current && *amount > 0 {
                if let Some(cycle) = self.dfs_find_cycle(to, visited, path, path_set) {
                    return Some(cycle);
                }
            }
        }

        path.pop();
        path_set.remove(current);
        None
    }

    /// Cancel a cycle by reducing all debts in it by the minimum amount
    fn cancel_cycle(&mut self, cycle: &[String]) -> DebtCycle {
        if cycle.len() < 3 {
            warn!("Invalid cycle with less than 3 participants");
            return DebtCycle {
                participants: cycle.to_vec(),
                amount: 0,
                currency: self.currency.clone(),
            };
        }

        // Find the minimum debt in the cycle
        let mut min_amount = i64::MAX;
        for i in 0..cycle.len() - 1 {
            let from = &cycle[i];
            let to = &cycle[i + 1];
            let amount = self.graph.get(&(from.clone(), to.clone())).copied().unwrap_or(0);
            min_amount = min_amount.min(amount);
        }

        if min_amount == 0 {
            return DebtCycle {
                participants: cycle.to_vec(),
                amount: 0,
                currency: self.currency.clone(),
            };
        }

        // Reduce all debts in the cycle by the minimum amount
        for i in 0..cycle.len() - 1 {
            let from = &cycle[i];
            let to = &cycle[i + 1];
            let key = (from.clone(), to.clone());
            if let Some(amount) = self.graph.get_mut(&key) {
                *amount -= min_amount;
                if *amount == 0 {
                    self.graph.remove(&key);
                }
            }
        }

        debug!(
            "Canceled cycle {:?} with amount {}",
            cycle, min_amount
        );

        DebtCycle {
            participants: cycle.to_vec(),
            amount: min_amount,
            currency: self.currency.clone(),
        }
    }

    /// Perform multilateral netting (find and cancel cycles)
    pub fn net(&mut self) -> NettingResult {
        let original: Vec<Obligation> = self
            .graph
            .iter()
            .map(|((from, to), amount)| Obligation {
                from: from.clone(),
                to: to.clone(),
                amount: *amount,
                currency: self.currency.clone(),
            })
            .collect();

        let mut total_reduced = 0;

        // Step 1: Bilateral netting
        self.bilateral_netting();

        // Step 2: Multilateral netting (cycle detection)
        let mut cycles = Vec::new();
        let participants = self.get_participants();

        // Try to find cycles starting from each participant
        for start in &participants {
            while let Some(cycle) = self.find_cycle(start) {
                let debt_cycle = self.cancel_cycle(&cycle);
                total_reduced += debt_cycle.amount;
                cycles.push(debt_cycle);

                // Refresh participants after cycle cancellation
                if self.graph.is_empty() {
                    break;
                }
            }
        }

        let netted: Vec<Obligation> = self
            .graph
            .iter()
            .filter(|(_, amount)| **amount > 0)
            .map(|((from, to), amount)| Obligation {
                from: from.clone(),
                to: to.clone(),
                amount: *amount,
                currency: self.currency.clone(),
            })
            .collect();

        debug!(
            "Netting complete: {} cycles canceled, {} reduced",
            cycles.len(),
            total_reduced
        );

        NettingResult {
            original,
            netted,
            cycles_canceled: cycles,
            amount_reduced: total_reduced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bilateral_netting() {
        let mut engine = NettingEngine::new("USD".to_string());
        engine.add_obligation("Alice".to_string(), "Bob".to_string(), 100);
        engine.add_obligation("Bob".to_string(), "Alice".to_string(), 30);

        let result = engine.net();

        assert_eq!(result.netted.len(), 1);
        assert_eq!(result.netted[0].from, "Alice");
        assert_eq!(result.netted[0].to, "Bob");
        assert_eq!(result.netted[0].amount, 70);
    }

    #[test]
    fn test_simple_cycle() {
        let mut engine = NettingEngine::new("USD".to_string());
        // A owes B 100, B owes C 80, C owes A 60
        engine.add_obligation("A".to_string(), "B".to_string(), 100);
        engine.add_obligation("B".to_string(), "C".to_string(), 80);
        engine.add_obligation("C".to_string(), "A".to_string(), 60);

        let result = engine.net();

        // After netting: A owes B 40, B owes C 20, C owes A 0
        assert_eq!(result.cycles_canceled.len(), 1);
        assert_eq!(result.cycles_canceled[0].amount, 60);
        assert_eq!(result.amount_reduced, 60);

        // Check remaining obligations
        assert_eq!(result.netted.len(), 2);
        let a_to_b = result.netted.iter().find(|o| o.from == "A" && o.to == "B");
        assert_eq!(a_to_b.unwrap().amount, 40);

        let b_to_c = result.netted.iter().find(|o| o.from == "B" && o.to == "C");
        assert_eq!(b_to_c.unwrap().amount, 20);
    }

    #[test]
    fn test_complete_cycle_cancellation() {
        let mut engine = NettingEngine::new("USD".to_string());
        // A owes B 100, B owes C 100, C owes A 100 (perfect cycle)
        engine.add_obligation("A".to_string(), "B".to_string(), 100);
        engine.add_obligation("B".to_string(), "C".to_string(), 100);
        engine.add_obligation("C".to_string(), "A".to_string(), 100);

        let result = engine.net();

        // All debts should cancel out
        assert_eq!(result.cycles_canceled.len(), 1);
        assert_eq!(result.cycles_canceled[0].amount, 100);
        assert_eq!(result.netted.len(), 0);
    }

    #[test]
    fn test_four_party_cycle() {
        let mut engine = NettingEngine::new("USD".to_string());
        // A -> B -> C -> D -> A
        engine.add_obligation("A".to_string(), "B".to_string(), 100);
        engine.add_obligation("B".to_string(), "C".to_string(), 80);
        engine.add_obligation("C".to_string(), "D".to_string(), 90);
        engine.add_obligation("D".to_string(), "A".to_string(), 70);

        let result = engine.net();

        // Minimum is 70, so that gets canceled from the cycle
        assert!(result.cycles_canceled.len() >= 1);
        assert_eq!(result.amount_reduced, 70);
    }

    #[test]
    fn test_multiple_cycles() {
        let mut engine = NettingEngine::new("USD".to_string());
        // Cycle 1: A -> B -> C -> A
        engine.add_obligation("A".to_string(), "B".to_string(), 100);
        engine.add_obligation("B".to_string(), "C".to_string(), 100);
        engine.add_obligation("C".to_string(), "A".to_string(), 100);

        // Cycle 2: D -> E -> F -> D
        engine.add_obligation("D".to_string(), "E".to_string(), 50);
        engine.add_obligation("E".to_string(), "F".to_string(), 50);
        engine.add_obligation("F".to_string(), "D".to_string(), 50);

        let result = engine.net();

        // Both cycles should be found and canceled
        assert!(result.cycles_canceled.len() >= 2);
        assert_eq!(result.amount_reduced, 150); // 100 + 50
        assert_eq!(result.netted.len(), 0);
    }

    #[test]
    fn test_no_cycle() {
        let mut engine = NettingEngine::new("USD".to_string());
        // Linear chain: A -> B -> C (no cycle)
        engine.add_obligation("A".to_string(), "B".to_string(), 100);
        engine.add_obligation("B".to_string(), "C".to_string(), 80);

        let result = engine.net();

        // No cycles to cancel
        assert_eq!(result.cycles_canceled.len(), 0);
        assert_eq!(result.netted.len(), 2);
    }
}
