/** Strategic phases of ICN. Mirrors docs/PHASE_PROGRESS.md. */
export interface StrategicPhase {
  num: 0 | 1 | 2 | 3 | 4;
  name: string;
  status: "done" | "active" | "future";
  caption: string;
}

export const strategicPhases: StrategicPhase[] = [
  { num: 0, name: "Genesis", status: "done", caption: "Substrate, kernel, K3s" },
  { num: 1, name: "Pre-Pilot Hardening", status: "done", caption: "Misbehavior, scheduler, gateway" },
  { num: 2, name: "Pilot Launch", status: "active", caption: "Partner-bound (NYCN)" },
  { num: 3, name: "Federation Depth", status: "future", caption: "Not started" },
  { num: 4, name: "Institution-in-a-Box", status: "future", caption: "Not started" },
];

/** Next 3 implementation phases (the numbered 1-35+ sequence). */
export interface ImplementationPhase {
  num: number;
  name: string;
  workstream: string;
  status: "scoped" | "in-progress" | "blocked" | "done";
  estStart: string;
  note: string;
}

export const nextImplementationPhases: ImplementationPhase[] = [
  { num: 19, name: "Release Infrastructure", workstream: "ops", status: "scoped", estStart: "May 2026", note: "Tag/release pipeline cleanup. Not blocked." },
  { num: 20, name: "Testing Foundation", workstream: "ops · dev", status: "scoped", estStart: "May 2026", note: "Multi-node integration test harness expansion." },
  { num: 21, name: "Network Connectivity", workstream: "gossip · net", status: "scoped", estStart: "Jun 2026", note: "IPv6, transport robustness, endpoint sets." },
];

/** Current Phase 2 closure gates. */
export const phase2Gates: string[] = [
  "Organizer presentation to NYCN organizers",
  "Pilot formalization",
  "First operator rehearsal against real material",
  "idea-0019 RFC: visibility/privacy run with redaction",
  "idea-0019 RFC: accessibility-gate ProcessGateResult on real surface",
  "idea-0019 RFC: open-question triage (Q1/Q3/Q4)",
  "idea-0020 RFC: ADR-0026 receipt for one DAP primitive",
  "idea-0020 RFC: Q1 or Q5 resolved in writing",
];
