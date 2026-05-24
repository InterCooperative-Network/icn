/** Workstream heat map. Hand-edit when activity shifts. */
export type Heat = "stable" | "active" | "quiet" | "early";

export interface Workstream {
  name: string;
  heat: Heat;
  note: string;
}

export const workstreams: Workstream[] = [
  { name: "kernel",         heat: "stable", note: "Crate consolidation, strict meaning firewall. Stable." },
  { name: "identity",       heat: "stable", note: "DIDs, Ed25519, age-encrypted keystore. Stable." },
  { name: "trust",          heat: "stable", note: "TrustPolicyOracle, trust-gated rate limits. Stable." },
  { name: "gossip / net",   heat: "stable", note: "QUIC/TLS, anti-entropy, replay guard. Stable." },
  { name: "ledger",         heat: "active", note: "Opaque receipt storage stack (May 6–7). Active." },
  { name: "governance",     heat: "active", note: "Action-card runtime, completion receipts. Active." },
  { name: "compute",        heat: "quiet",  note: "Placement spec landed (#1826); runtime slice pending." },
  { name: "gateway",        heat: "active", note: "/me/standing, /me/action-cards. Active." },
  { name: "federation",     heat: "early",  note: "Anti-entropy spec (#1829); runtime slice pending." },
  { name: "docs / spec",    heat: "active", note: "13-PR architecture-spec ladder May 14–15. Hot." },
  { name: "design",         heat: "active", note: "Member shell v0 (#1830), steward cockpit v0 (#1831)." },
  { name: "pilot-ui",       heat: "quiet",  note: "Demo-mode tabs verified March 2026." },
  { name: "website",        heat: "quiet",  note: "intercooperative.network live; periodic claim updates." },
  { name: "ops",            heat: "active", note: "K3s cluster, deploy makefile, CI. Active." },
  { name: "security",       heat: "active", note: "Abuse-case hardening strategy (2026-05-16)." },
];

export const heatLegend: Record<Heat, { dot: string; label: string }> = {
  stable: { dot: "⚫", label: "shipped & stable" },
  active: { dot: "🟢", label: "active" },
  quiet:  { dot: "🟡", label: "developed, quiet" },
  early:  { dot: "🔵", label: "early / pre-build" },
};
