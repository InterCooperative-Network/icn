# Module 12: Observability and Metrics

## Overview
This module explains how ICN observes itself in production: metrics, tracing,
logging, and dashboards. The goal is to understand how visibility is built into
the system and why it is critical for distributed coordination.

## Objectives
- Understand ICN's observability stack and its design rationale
- Learn how metrics are defined and registered across subsystems
- Understand tracing/logging configuration and what it reveals
- Map observability signals to the subsystems they diagnose

## Prerequisites
- Module 2 (Architecture Overview)
- Module 9 (Operations and Deployment)

## Key Reading
- `icn/crates/icn-obs/` - Observability crate
- `monitoring/README.md` - Metrics and dashboard setup
- `docs/production-hardening.md` - Production concerns
- `docs/onboarding/patterns.md` - Metrics integration pattern

---

## Walkthrough

### 1. Why Observability Matters in ICN

ICN is distributed, eventually consistent, and trust-gated. Without visibility
into what each node believes and how it behaves, operational debugging becomes
guesswork. Observability provides:

- **Metrics**: Quantitative health signals (counters, gauges, histograms)
- **Tracing**: Causal flow across async tasks and services
- **Logging**: Human-readable narratives for incidents

### 2. Metrics Architecture

Metrics are defined in subsystem-specific modules and registered during startup.
Each metric has a clear name, unit, and description to avoid ambiguity.

Common naming pattern:
```
icn_{subsystem}_{metric}_{unit}
```

### 3. Tracing and Logging

Tracing provides causal context across async boundaries. Logging supplies
structured, human-friendly events. Together they support:

- Performance diagnosis (latency hotspots)
- Failure analysis (why a message was dropped)
- Capacity planning (which subsystem is saturated)

### 4. Dashboards and Alerts

Grafana dashboards and Prometheus alert rules provide a consistent, operational
view of the network. This enables:

- Early detection of gossip lag or ledger backlog
- Trust-rate limiting anomalies
- Resource bottlenecks per node

---

## Exercises

1. **Find metric definitions**
   - Open `icn/crates/icn-obs/src/metrics/`
   - Pick a subsystem module and list three metrics it exports

2. **Trace a startup path**
   - Find where metrics are initialized in the runtime
   - Identify which actors register metrics on startup

3. **Interpret a dashboard**
   - Open `monitoring/grafana-dashboard.json`
   - Identify panels that show gossip throughput or ledger activity

4. **Map signal to subsystem**
   - Choose three metrics and note which subsystem behavior they reflect

---

## Checkpoints

- [ ] You can explain the purpose of metrics, tracing, and logging in ICN
- [ ] You can find where metrics are defined and registered
- [ ] You can map metrics to subsystems and behaviors
- [ ] You can identify dashboard panels for gossip and ledger health

---

## Notes and gotchas

- Metrics should be stable and well-described; renaming breaks dashboards.
- Excessive logging in hot paths can impact performance.
- Tracing spans should be scoped to meaningful operations, not every function.
