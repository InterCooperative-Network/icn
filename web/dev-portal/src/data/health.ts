/**
 * Project-health snapshot.
 *
 * In a real deploy these values come from live API calls (GitHub Checks API,
 * the K3s status relay, etc.) on the server side, cached briefly. For the
 * scaffold, they're hand-edited. Each metric carries a `status` so the UI can
 * render an at-a-glance traffic light.
 */
export type HealthStatus = "green" | "yellow" | "red" | "unknown";

export interface HealthMetric {
  label: string;
  status: HealthStatus;
  primary: string;      // big readout, e.g. "all green" or "5 open"
  secondary?: string;   // small caption
  href?: string;        // click-through
}

export const health: HealthMetric[] = [
  { label: "CI",            status: "green",  primary: "all green", secondary: "main + 5 PRs",     href: "https://github.com/InterCooperative-Network/icn/actions" },
  { label: "Deploy",        status: "green",  primary: "K3s up",    secondary: "image 91a63eec",   href: "/inside/deploy" },
  { label: "Open PRs",      status: "green",  primary: "5",         secondary: "0 stale (>14d)",   href: "/prs" },
  { label: "Open issues",   status: "yellow", primary: "1873",      secondary: "50 epic-labeled",  href: "/issues" },
  { label: "Vuln alerts",   status: "red",    primary: "219",       secondary: "1 critical · 113 high", href: "https://github.com/InterCooperative-Network/icn/security/dependabot" },
  { label: "Phase",         status: "yellow", primary: "2 · holding", secondary: "partner-bound",  href: "/phase" },
];

export function statusColor(s: HealthStatus): { bg: string; fg: string; dot: string } {
  switch (s) {
    case "green":   return { bg: "var(--done-bg)",     fg: "var(--done-fg)",     dot: "🟢" };
    case "yellow":  return { bg: "var(--active-bg)",   fg: "var(--active-fg)",   dot: "🟡" };
    case "red":     return { bg: "var(--tone-critical-bg)", fg: "var(--tone-critical-fg)", dot: "🔴" };
    case "unknown": return { bg: "var(--tone-neutral-bg)",  fg: "var(--tone-neutral-fg)",  dot: "⚪" };
  }
}
