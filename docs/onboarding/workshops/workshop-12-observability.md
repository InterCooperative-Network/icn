# Workshop 12: Observability and Metrics

## Learning Objectives

By the end of this workshop, you will be able to:

1. **Locate metrics** - Find and interpret metric definitions
2. **Trace observability setup** - Identify where metrics and tracing are initialized
3. **Connect signals to subsystems** - Map metrics to runtime behavior

## Goal
Build a mental model of how ICN observes itself and how operators diagnose
issues using metrics, tracing, and logs.

## Prerequisites
- Completed [Module 12: Observability](../modules/module-12-observability.md)
- Familiarity with Module 9 (Operations)

## Estimated time
1-2 hours

## Related Materials
- `icn/crates/icn-obs/`
- `monitoring/README.md`
- `docs/onboarding/patterns.md`

---

## Part 1: Metric Definitions

### Steps
1. Open `icn/crates/icn-obs/src/metrics/`
2. Choose one subsystem file (e.g., gossip or ledger)
3. List three metrics and their descriptions

### Questions
1. What naming convention is used?
2. Which metrics indicate throughput vs errors?

### Checkpoint
- [ ] You can find and interpret metric definitions

---

## Part 2: Initialization Path

### Steps
1. Find where observability is initialized in runtime startup
2. Identify which components register metrics

### Questions
1. Where are metric descriptions registered?
2. Where is tracing configured?

### Checkpoint
- [ ] You can trace the observability initialization path

---

## Part 3: Dashboard Mapping

### Steps
1. Open `monitoring/grafana-dashboard.json`
2. Identify panels related to gossip and ledger
3. Note which metrics drive those panels

### Checkpoint
- [ ] You can map dashboard panels to subsystem metrics

---

## Summary

After completing this workshop you should be able to:
- Locate metric definitions and interpret their intent
- Trace how observability is initialized
- Connect dashboards to runtime behavior

## Next steps
Proceed to Workshop 13: Security and Privacy
