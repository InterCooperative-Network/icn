"""Non-interactive text report (same reconciliation as the TUI)."""

from __future__ import annotations

import time

from .util import ago, fmt_ts, short


def source_lines(snap: dict) -> list[str]:
    out = []
    for k in sorted(snap):
        if k.startswith("_"):
            continue
        o = snap[k]
        flag = "OK " if o.ok and not o.from_cache else ("CACHED" if o.ok else "FAIL")
        extra = f" — {o.error}" if o.error else ""
        out.append(f"  [{flag:6}] {o.source:34} observed {fmt_ts(o.observed_at)} ({ago(o.observed_at)}){extra}")
    return out


def render(snap: dict, views: dict, width: int = 110) -> str:
    L = []
    now = time.time()
    L.append(f"ICN DEVELOPMENT CONTROL CENTER — report generated {fmt_ts(now)}")
    L.append("Statuses are reconciled from live sources; anything not observed is UNKNOWN, never OK.")
    L.append("")
    L.append("== NEEDS ATTENTION ==")
    for a in views["attention"][:25]:
        L.append(f"  [{a['status']}] {a['title']}")
        if a.get("detail"):
            L.append(f"      {a['detail']}  (source: {a['source']})")
    if not views["attention"]:
        L.append("  nothing flagged")
    L.append("")
    acts = views.get("activities", [])
    if acts:
        from .activities import in_view, summary
        c = summary(acts)
        L.append(f"== WORK ({c['total']} activities) == Active {c['Active']} · Paused {c['Paused']} · needs you {c['attention']}"
                 f" · Completed {c['Completed']} · Archived {c['Archived']} · Unresolved {c['Unresolved']}")
        for a in [a for a in acts if in_view(a, "now")][:30]:
            L.append(f"  {'!' if a['attention'] else ' '} [{a['status']}] {a['program_name']}: {a['title'][:90]}")
            L.append(f"      next: {a['next'][:150]}")
        L.append("")
    L.append("== WHAT SHOULD HAPPEN NEXT ==")
    for s in views["next"]:
        L.append(f"  - {s}")
    L.append("")
    L.append("== SESSIONS ==")
    for s in views["sessions"]:
        L.append(f"  [{s['status']}] {s['name']}  — {s['state_label']}")
        L.append(f"      where: {s.get('worktree') or s.get('cwd') or '?'}  branch: {s.get('branch') or '-'}"
                 f"  tmux: {s.get('tmux') or '-'}  pid: {s.get('pid') or '-'}  last activity: {ago(s.get('last_activity'))}")
        if s.get("task"):
            L.append(f"      task: {s['task'][:150]}  ({s['task_source']})")
        if s.get("pr"):
            L.append(f"      PR: {s['pr']['repo']}#{s['pr']['number']} ({s['pr']['how']}) — {s['pr'].get('status')}")
        if s.get("claims"):
            L.append(f"      claims: {', '.join(s['claims'][:5])}{' …' if len(s['claims']) > 5 else ''}")
        if s.get("handoffs"):
            L.append(f"      handoff: {s['handoffs'][0]}")
        L.append(f"      resume: {s['resume']}   sources: {', '.join(s['sources'])}")
    L.append("")
    L.append("== OPEN PULL REQUESTS ==")
    for p in views["prs"]:
        lc = p.get("lifecycle") or {}
        L.append(f"  [{p['status']}] {p['repo']}#{p['number']} {'DRAFT ' if p['draft'] else ''}{p['title'][:80]}")
        L.append(f"      head {short(p['head'])}  lifecycle {lc.get('state') or '-'}  required CI {p['required']['verdict']}"
                 f" ({p['required']['detail']})  threads unresolved {p['unresolved_threads']}  program {p['program']}")
        L.append(f"      {p['status_reason']}")
        if p.get("evidence_dirs"):
            L.append(f"      local review evidence: {', '.join(p['evidence_dirs'])}")
    if not views["prs"]:
        o = snap.get("github_prs")
        L.append("  GitHub unavailable: " + (o.error if o else "not yet observed") if (o is None or not o.ok) else "  none open")
    L.append("")
    L.append("== RECENTLY MERGED (7d; merged is not the same as an independently verified capability) ==")
    for m in views["merged"][:15]:
        L.append(f"  {m['repo']}#{m['number']} merged {ago(m.get('merged_ts'))}: {m['title'][:90]}")
    L.append("")
    L.append("== PROGRAMS ==")
    for pg in views["programs"]:
        L.append(f"  {pg['name']}: {len(pg['prs'])} open PR(s), {len(pg['live_agents'])} live agent(s), "
                 f"{len(pg['worktrees'])} worktree(s), {len(pg['waiting'])} awaiting maintainer")
        L.append(f"      next: {pg['next']}")
    L.append("")
    wts = views["worktrees"]
    keep = [w for w in wts if w["verdict"] == "KEEP"]
    cand = [w for w in wts if w["verdict"].startswith("CLEANUP")]
    L.append(f"== WORKTREES == {len(wts)} total, {len(keep)} must be kept, {len(cand)} cleanup candidates (verify first)")
    for w in keep[:40]:
        L.append(f"  KEEP {w['repo']}/{w['name']} [{w.get('branch') or 'detached'}]: {'; '.join(w['keep_reasons'][:4])}")
    L.append("")
    L.append("== SOURCES ==")
    L.extend(source_lines(snap))
    return "\n".join(L) + "\n"
