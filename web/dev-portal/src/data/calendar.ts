/**
 * Calendar entries. Hand-maintained for now; eventually pulled from an iCal.
 *
 * Each entry has an ISO date (YYYY-MM-DD), a tag for color, and a short label.
 * The landing renders the next ~5; the full calendar lives at /calendar.
 */
export type CalendarTag = "sync" | "deadline" | "milestone" | "external" | "personal";

export interface CalendarEntry {
  date: string;
  label: string;
  tag: CalendarTag;
  note?: string;
}

// Edit this list as the calendar shifts. ISO YYYY-MM-DD; sorted ascending.
export const calendar: CalendarEntry[] = [
  // Examples — replace with real entries:
  { date: "2026-05-20", label: "Weekly sync · 14:00 ET", tag: "sync" },
  { date: "2026-05-20", label: "Standup due · 18:00 ET", tag: "deadline" },
  { date: "2026-05-21", label: "NYCN organizer prep call", tag: "external" },
  { date: "2026-05-22", label: "Week-review by EOD", tag: "deadline" },
  { date: "2026-05-26", label: "Pilot ladder rehearsal #1", tag: "milestone" },
];

export function upcoming(limit = 5, fromIso?: string): CalendarEntry[] {
  const from = fromIso ?? new Date().toISOString().slice(0, 10);
  return calendar.filter((c) => c.date >= from).slice(0, limit);
}

export function tagDot(tag: CalendarTag): string {
  switch (tag) {
    case "sync":      return "🟢";
    case "deadline":  return "🟡";
    case "milestone": return "🔵";
    case "external":  return "🟣";
    case "personal":  return "⚪";
  }
}
