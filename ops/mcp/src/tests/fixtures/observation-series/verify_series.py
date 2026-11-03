#!/usr/bin/env python3
"""Offline verifier for the ICN multi-export continuity package.

    python3 -I verify_series.py [package-dir] [--also-absent STRING ...]

Standard library only. It imports and runs NO ICN code and needs no network. Exit status 0 means
every check passed; anything else means the package must not be used.

It does three independent things:
  1. INTEGRITY  - exact file set, SHA256SUMS, manifest, and for every snapshot the sidecar's
                  SHA-256/length binding, SQLite header, exact tables/columns, NULL semantics,
                  digest and claim-path forms, and a raw-byte privacy scan.
  2. RE-DERIVATION - recomputes, from the databases alone, every per-export fact and every
                  pairwise comparison (new / absent / changed sessions, claim changes, verdict,
                  anomalies) and requires them to equal reference/series-contract.json. The
                  producer-side model that wrote the contract is therefore not its own witness.
  3. SEMANTICS  - asserts the specific lifecycle transitions the series was built to exercise,
                  and that ICN's invariants (immutable fields, unique conversation ids, per-export
                  claim tokens) hold in the real outputs.

What passing does NOT prove: authenticity, or that two exports are the same registry. Nothing is
signed, and the files carry no registry identity.
"""
import hashlib
import json
import os
import re
import sqlite3
import stat
import sys
from datetime import datetime, timezone

args = sys.argv[1:]
extra_absent = []
while "--also-absent" in args:
    i = args.index("--also-absent")
    extra_absent.append(args[i + 1])
    del args[i : i + 2]
ROOT = os.path.abspath(args[0]) if args else os.path.dirname(os.path.abspath(__file__))

LABELS = ["A1", "A2", "B1", "A3", "A4"]
FORMAT = "icn-ops-registry-snapshot-meta/v1"
POLICY = "observation-v1"
IMMUTABLE = ["repo", "provider", "provider_session_id", "parent_session_id", "host", "started_at"]
COMPARED = ["repo", "repo_id", "worktree_id", "worktree_name", "worktree_generation", "provider", "provider_session_id",
            "parent_session_id", "host", "agent_pid", "started_at", "last_heartbeat", "last_progress", "progress_count"]
EXPECTED_FILES = (
    {f"snapshots/{l}.db" for l in LABELS}
    | {f"snapshots/{l}.db.meta.json" for l in LABELS}
    | {"reference/series-contract.json", "reference/contract.json", "reference/test-vectors.json",
       "HANDOFF-SERIES.md", "MANIFEST.json", "SHA256SUMS", "verify_series.py"}
)

failures = 0


def check(name, ok, detail=""):
    global failures
    if not ok:
        failures += 1
    print(("PASS  " if ok else "FAIL  ") + name + ("" if ok or not detail else f"  -> {detail}"))
    return ok


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()


p = lambda rel: os.path.join(ROOT, *rel.split("/"))
jload = lambda rel: json.load(open(p(rel), encoding="utf-8"))


def walk():
    out = set()
    for d, dirs, files in os.walk(ROOT, followlinks=False):
        for n in files + dirs:
            full = os.path.join(d, n)
            rel = os.path.relpath(full, ROOT).replace(os.sep, "/")
            out.add(rel)
    return out


# ── 1. layout & checksums ────────────────────────────────────────────────────
entries = walk()
files = {e for e in entries if not os.path.isdir(p(e)) or os.path.islink(p(e))}
check("layout: exactly the expected files", files == EXPECTED_FILES, f"extra={sorted(files - EXPECTED_FILES)} missing={sorted(EXPECTED_FILES - files)}")
check("layout: only the snapshots/ and reference/ directories", {e for e in entries if os.path.isdir(p(e)) and not os.path.islink(p(e))} == {"snapshots", "reference"})
check("layout: every entry is a regular file or plain directory (no symlink/device)", all(stat.S_ISREG(os.lstat(p(e)).st_mode) or stat.S_ISDIR(os.lstat(p(e)).st_mode) for e in entries))
check("layout: no SQLite WAL/SHM/journal sibling", not any(re.search(r"-(wal|shm|journal)$", e) for e in entries))
if failures:
    print("\nSERIES NOT VERIFIED (layout)")
    sys.exit(1)

sums = {}
for line in open(p("SHA256SUMS"), encoding="utf-8").read().splitlines():
    m = re.fullmatch(r"([0-9a-f]{64})  (\S+)", line)
    check("SHA256SUMS: line well-formed", bool(m), line) if not m else sums.__setitem__(m.group(2), m.group(1))
check("SHA256SUMS: covers every file except itself", set(sums) == EXPECTED_FILES - {"SHA256SUMS"})
check("SHA256SUMS: every hash matches", all(sha(p(n)) == h for n, h in sums.items() if n in EXPECTED_FILES))

man = jload("MANIFEST.json")
check("manifest: producer commit is 40 hex", bool(re.fullmatch(r"[0-9a-f]{40}", str(man.get("producer", {}).get("git_commit", "")))))
mf = {f["path"]: f for f in man.get("files", [])}
listed = EXPECTED_FILES - {"MANIFEST.json", "SHA256SUMS", "verify_series.py"}
check("manifest: lists every artifact", set(mf) == listed, str(sorted(set(mf) ^ listed)))
check("manifest: hashes and lengths match", all(mf[n]["sha256"] == sha(p(n)) and mf[n]["bytes"] == os.path.getsize(p(n)) for n in listed if n in mf))
check("manifest: declares the profile/policy/format/schema tuple", man.get("tuple") == {"profile": "observation", "policy": POLICY, "format": FORMAT, "registry_schema_version": 5}, str(man.get("tuple")))

series = jload("reference/series-contract.json")
golden = jload("reference/contract.json")
vec = jload("reference/test-vectors.json")
check("series contract: version and tuple", series.get("contract_version") == "icn-observation-series-contract/v1" and series.get("tuple") == man.get("tuple"))
check("series contract: exports are exactly A1, A2, B1, A3, A4 in generation order", [e["label"] for e in series["exports"]] == LABELS)

# ── 2. per-snapshot integrity ────────────────────────────────────────────────
withheld_cols = golden["field_policy"]["sessions"]["withheld"]
canaries = ["SERIES-CANARY", "/synthetic/series-root", "/home/", "/tmp/", "icn-dev", "ubuntu", "10.8."] + extra_absent
D = {}  # label -> dict of facts derived HERE
for lab in LABELS:
    dbp, mp = p(f"snapshots/{lab}.db"), p(f"snapshots/{lab}.db.meta.json")
    meta = json.load(open(mp, encoding="utf-8"))
    pre = f"{lab}:"
    check(f"{pre} sidecar tuple (format/profile/policy/schema)", meta.get("format") == FORMAT and meta.get("profile") == "observation" and meta.get("disclosure", {}).get("policy") == POLICY and meta.get("snapshot", {}).get("schema_version_max") == 5)
    check(f"{pre} sidecar names its file and binds length + SHA-256 of the database", meta["snapshot"]["file"] == f"{lab}.db" and meta["snapshot"]["bytes"] == os.path.getsize(dbp) and meta["snapshot"]["sha256"] == sha(dbp))
    check(f"{pre} host is a claim, unauthenticated; no source path/identity", meta["host"]["authenticated"] is None and meta["host"]["authenticity"] == "none" and not any(k in meta["source"] for k in ("path_given", "realpath", "dev", "ino", "identity_sha256")))
    hdr = open(dbp, "rb").read(100)
    check(f"{pre} SQLite header: magic, 4096 pages, rollback-journal 1/1, UTF-8", hdr[:16] == b"SQLite format 3\x00" and int.from_bytes(hdr[16:18], "big") == 4096 and hdr[18] == 1 and hdr[19] == 1 and int.from_bytes(hdr[56:60], "big") == 1)
    db = sqlite3.connect(f"file:{dbp}?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    q = lambda sql, *a: db.execute(sql, a).fetchall()
    check(f"{pre} integrity + foreign keys + delete journal", q("PRAGMA integrity_check")[0][0] == "ok" and q("PRAGMA foreign_key_check") == [] and q("PRAGMA journal_mode")[0][0] == "delete")
    objs = q("SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'")
    tables = sorted(r["name"] for r in objs if r["type"] == "table")
    check(f"{pre} exactly the three observation tables; only tables and indexes", tables == ["file_claims", "schema_version", "sessions"] and {r["type"] for r in objs} <= {"table", "index"})
    for t in tables:
        cols = [[c["name"], c["type"], c["notnull"]] for c in q(f"PRAGMA table_info({t})")]
        want = [[c["name"], c["declared_type"], 1 if c["not_null_in_schema"] else 0] for c in golden["database"]["columns"][t]]
        check(f"{pre} {t} columns/types match the golden contract", cols == want)
    check(f"{pre} all 9 withheld columns are NULL in every row", len(withheld_cols) == 9 and all(q(f"SELECT COUNT(*) FROM sessions WHERE {c} IS NOT NULL")[0][0] == 0 for c in withheld_cols))
    dig = q("SELECT repo_id AS v FROM sessions WHERE repo_id IS NOT NULL UNION ALL SELECT worktree_id FROM sessions WHERE worktree_id IS NOT NULL")
    check(f"{pre} identity digests well-formed; NULL stays NULL", all(re.fullmatch(r"sha256:[0-9a-f]{64}", r["v"]) for r in dig))
    claims = [(r["session_id"], r["file_path"]) for r in q("SELECT session_id, file_path FROM file_claims")]
    check(f"{pre} claim paths are repo-relative-safe or withheld tokens", all(re.fullmatch(r"withheld:[0-9a-f]{16}", fp) or (all(re.fullmatch(r"[A-Za-z0-9._@+=,-]+", s) and s != ".." for s in fp.split("/"))) for _, fp in claims))
    check(f"{pre} sidecar row counts equal the database", meta["consistency"]["table_row_counts"] == {t: q(f"SELECT COUNT(*) FROM {t}")[0][0] for t in tables})
    blob = open(dbp, "rb").read() + open(mp, "rb").read()
    check(f"{pre} no canary, raw synthetic path, or non-fixture host/path in database or sidecar", not [c for c in canaries if c.encode() in blob], str([c for c in canaries if c.encode() in blob]))
    convs = [r["provider_session_id"] for r in q("SELECT provider_session_id FROM sessions WHERE provider_session_id IS NOT NULL")]
    check(f"{pre} a conversation id appears on at most one row (R10)", len(convs) == len(set(convs)))
    D[lab] = {
        "meta": meta,
        "sessions": {r["id"]: {k: r[k] for k in r.keys()} for r in q("SELECT * FROM sessions")},
        "claims": claims,
        "applied": [[r["version"], r["applied_at"]] for r in q("SELECT version, applied_at FROM schema_version ORDER BY version")],
    }
    db.close()

# ── 3. re-derive per-export facts and compare with the contract ──────────────
def sqltime(iso):  # '2026-01-01T10:10:00.000Z' -> '2026-01-01 10:10:00'
    return iso[:19].replace("T", " ")


def secs(sqlstamp):
    return int(datetime.strptime(sqlstamp, "%Y-%m-%d %H:%M:%S").replace(tzinfo=timezone.utc).timestamp())


def isoz(iso):
    return datetime.strptime(iso[:19], "%Y-%m-%dT%H:%M:%S").replace(tzinfo=timezone.utc).timestamp() + float(iso[19:23] or 0) * 1.0


for ex in series["exports"]:
    lab = ex["label"]
    d = D[lab]
    sess = d["sessions"]
    pre = f"{lab} (re-derived):"
    check(f"{pre} acquisition times, host and registry stamps match the contract", d["meta"]["acquisition"]["started_at"] == ex["acquisition"]["started_at"] and d["meta"]["acquisition"]["finished_at"] == ex["acquisition"]["finished_at"] and d["meta"]["host"]["claimed"]["hostname"] == ex["claimed_host"] and d["applied"] == ex["schema_version_applied_at"])
    check(f"{pre} acquisition finished_at is later than started_at", d["meta"]["acquisition"]["finished_at"] > d["meta"]["acquisition"]["started_at"])
    withheld_rows = [c for c in d["claims"] if c[1].startswith("withheld:")]
    check(f"{pre} counts", ex["counts"] == {"sessions": len(sess), "file_claims": len(d["claims"]), "withheld_claim_rows": len(withheld_rows)})
    check(f"{pre} session id set", ex["session_ids"] == sorted(sess))
    acq = secs(sqltime(d["meta"]["acquisition"]["started_at"]))
    ages = {i: ((acq - secs(s["last_heartbeat"])) // 60 if s["last_heartbeat"] else None) for i, s in sess.items()}
    check(f"{pre} heartbeat ages at acquisition (floor minutes; negative = after acquisition)", ex["heartbeat_age_minutes_at_acquisition"] == ages)
    clear = {}
    for sid, fp in d["claims"]:
        if not fp.startswith("withheld:"):
            clear.setdefault(fp, []).append(sid)
    check(f"{pre} cleartext claims", {c["path"]: c["sessions"] for c in ex["cleartext_claims"]} == {k: sorted(v) for k, v in clear.items()})
    grp = {}
    for sid, fp in withheld_rows:
        grp.setdefault(fp, []).append(sid)
    check(f"{pre} withheld-claim equality classes (within this snapshot only)", sorted(map(tuple, ex["withheld_claim_groups"])) == sorted(tuple(sorted(v)) for v in grp.values()))
    after = sorted(i for i, s in sess.items() if any(s[c] is not None and s[c] > sqltime(d["meta"]["acquisition"]["started_at"]) for c in ("started_at", "last_heartbeat", "last_progress")))
    check(f"{pre} rows stamped after the export's own acquisition time", ex["row_timestamps_after_acquisition"] == after)


def pair(a, b):
    A, B = D[a], D[b]
    same = A["applied"] == B["applied"]
    reasons, verdict = [], "comparable_if_same_registry_asserted"
    if not same:
        verdict = "refused_different_registry"
    elif not (isoz(B["meta"]["acquisition"]["started_at"]) > isoz(A["meta"]["acquisition"]["started_at"])):
        verdict = "refused_not_later"
    ia, ib = set(A["sessions"]), set(B["sessions"])
    changed, unchanged, anomalies = {}, [], []
    for i in sorted(ia & ib):
        diff = {f: [A["sessions"][i].get(f), B["sessions"][i].get(f)] for f in COMPARED if A["sessions"][i].get(f) != B["sessions"][i].get(f)}
        if diff:
            changed[i] = diff
        else:
            unchanged.append(i)
        for f in IMMUTABLE:
            if f in diff:
                anomalies.append(f"{i}: {f} changed, which ICN never does to an existing session id")
        if "progress_count" in diff and diff["progress_count"][1] < diff["progress_count"][0]:
            anomalies.append(f"{i}: progress_count decreased")
    if verdict == "comparable_if_same_registry_asserted" and anomalies:
        verdict = "refused_anomaly"
    ca = {(s, f) for s, f in A["claims"] if not f.startswith("withheld:")}
    cb = {(s, f) for s, f in B["claims"] if not f.startswith("withheld:")}
    shared = None
    if not same:
        ca_c = {s["provider_session_id"] for s in A["sessions"].values() if s["provider_session_id"]}
        cb_c = {s["provider_session_id"] for s in B["sessions"].values() if s["provider_session_id"]}
        la = {s["worktree_id"] for s in A["sessions"].values() if s["worktree_id"]}
        lb = {s["worktree_id"] for s in B["sessions"].values() if s["worktree_id"]}
        shared = {"conversation_ids": sorted(ca_c & cb_c), "lane_digests": sorted(la & lb)}
    return {"verdict": verdict, "same": same, "new": sorted(ib - ia), "absent": sorted(ia - ib), "changed": changed, "unchanged": unchanged,
            "added": sorted(cb - ca), "removed": sorted(ca - cb), "anomalies": anomalies, "shared": shared}


PAIR = {}
for pr in series["pairs"]:
    a, b = pr["from"], pr["to"]
    r = pair(a, b)
    PAIR[(a, b)] = r
    pre = f"{a}->{b} (re-derived):"
    check(f"{pre} verdict is {pr['verdict']}", r["verdict"] == pr["verdict"], r["verdict"])
    check(f"{pre} newly present / absent sessions", r["new"] == pr["sessions"]["newly_present"] and r["absent"] == pr["sessions"]["absent_in_later_export"])
    check(f"{pre} changed fields per session id", r["changed"] == pr["sessions"]["changed"] and r["unchanged"] == pr["sessions"]["unchanged"])
    check(f"{pre} cleartext claim additions/removals; withheld claims declared not comparable",
          r["added"] == sorted((c["session_id"], c["path"]) for c in pr["claims"]["cleartext_added"])
          and r["removed"] == sorted((c["session_id"], c["path"]) for c in pr["claims"]["cleartext_removed"])
          and pr["claims"]["withheld"] == "not_comparable_across_exports")
    check(f"{pre} anomalies", r["anomalies"] == pr["anomalies"])
    if r["shared"] is not None:
        sh = pr["shared_with_other_registry"]
        check(f"{pre} coincidental overlap between DIFFERENT registries is reported as coincidence", sh["conversation_ids"] == r["shared"]["conversation_ids"] and sh["lane_digests"] == r["shared"]["lane_digests"] and "COINCIDENCE" in sh["note"])

# ── 4. the specific transitions the series was built to exercise ─────────────
L = series["session_labels"]
a_, b_, e_, l_, c_, d_ = (L["A"][k] for k in ("s-a", "s-b", "s-e", "s-l", "s-c", "s-d"))
check("A1->A2: a new activation appears (s-c); s-a's progress_count rose 3 -> 5 on the same id", PAIR[("A1", "A2")]["new"] == [c_] and PAIR[("A1", "A2")]["changed"][a_]["progress_count"] == [3, 5])
a23 = PAIR[("A2", "A3")]
check("A2->A3: s-c is absent (released) and a NEW id s-d carries the SAME conversation id", a23["absent"] == [c_] and a23["new"] == [d_] and D["A2"]["sessions"][c_]["provider_session_id"] == D["A3"]["sessions"][d_]["provider_session_id"] and c_ != d_)
check("A2->A3: the new activation restarts progress_count (1), not the released row's value", D["A3"]["sessions"][d_]["progress_count"] == 1 and D["A2"]["sessions"][c_]["progress_count"] == 0)
check("A2->A3: s-a changed ONLY worktree_generation (lane recreated at the same path) on the same id", set(a23["changed"][a_]) - {"last_heartbeat", "last_progress", "progress_count"} == {"worktree_generation"} and a23["changed"][a_]["worktree_generation"][0] != a23["changed"][a_]["worktree_generation"][1])
check("A2->A3: s-e moved lanes: worktree_id, worktree_name AND worktree_generation changed together on the same id, repo_id did not", {"worktree_id", "worktree_name", "worktree_generation"} <= set(a23["changed"][e_]) and "repo_id" not in a23["changed"][e_])
check("A2->A3: stale rows persist (s-b, s-l present, age >= 30 min) - stale is not ended (R9)", {b_, l_} <= set(D["A3"]["sessions"]) and all(series["exports"][3]["heartbeat_age_minutes_at_acquisition"][i] >= 30 for i in (b_, l_)))
check("A2->A3: an ownership conflict coexists - cleartext notes.md held by stale s-b and live s-d; withheld path likewise (one equality class of 2)",
      sorted(x for x in next(c for c in series["exports"][3]["cleartext_claims"] if c["path"].endswith("notes.md"))["sessions"]) == sorted([b_, d_])
      and series["exports"][3]["withheld_claim_groups"] == [sorted([b_, d_])])
check("A2->A3: the released session's claim is gone with it (other.rs removed)", any(x["path"].endswith("other.rs") and x["session_id"] == c_ for x in series["pairs"][1]["claims"]["cleartext_removed"]))
check("NULL identities stay unknown: s-l has NULL repo_id/worktree_id/generation/conversation in every A export, and s-b has a NULL conversation id",
      all(D[l]["sessions"][l_][f] is None for l in ("A1", "A2", "A3") for f in ("repo_id", "worktree_id", "worktree_generation", "provider_session_id")) and D["A1"]["sessions"][b_]["provider_session_id"] is None)
check("A3->A4: a later-labelled export with an EARLIER acquisition time is refused as a successor, and its rows are stamped after its own acquisition", PAIR[("A3", "A4")]["verdict"] == "refused_not_later" and series["exports"][4]["row_timestamps_after_acquisition"] != [])
check("A4 is otherwise identical to A3 (nothing changed)", PAIR[("A3", "A4")]["new"] == [] and PAIR[("A3", "A4")]["absent"] == [] and PAIR[("A3", "A4")]["changed"] == {})
check("B1 vs A: different registry stamps -> refused, yet B shares a conversation id AND lane digests with A (coincidence, never identity)", PAIR[("A2", "B1")]["verdict"] == "refused_different_registry" and PAIR[("A2", "B1")]["shared"]["conversation_ids"] != [] and PAIR[("A2", "B1")]["shared"]["lane_digests"] != [])
check("B1 is from a different claimed host", D["B1"]["meta"]["host"]["claimed"]["hostname"] != D["A1"]["meta"]["host"]["claimed"]["hostname"])
for pr in series["pairs"]:
    if pr["verdict"].startswith("comparable"):
        check(f"{pr['from']}->{pr['to']}: ICN's immutable fields hold for every id present in both", PAIR[(pr["from"], pr["to"])]["anomalies"] == [])

# per-export claim tokens must not be reusable as path identities
tok_by_export = {lab: {fp for _, fp in D[lab]["claims"] if fp.startswith("withheld:")} for lab in LABELS}
seen = {}
for lab, toks in tok_by_export.items():
    for t in toks:
        seen.setdefault(t, set()).add(lab)
check("claim tokens: no token VALUE recurs in a second export, so none can serve as a stable path identifier (R4)", all(len(v) == 1 for v in seen.values()), str({k: v for k, v in seen.items() if len(v) > 1}))
check("the SAME withheld path is claimed in A1, A2, A3 and A4 yet its token differs in every one of them", len({t for lab in ("A1", "A2", "A3", "A4") for t in tok_by_export[lab]}) == sum(len(tok_by_export[l]) for l in ("A1", "A2", "A3", "A4")))

# ── 5. text files: no credential shapes / non-fixture paths ──────────────────
secret_res = [r"-----BEGIN [A-Z ]*PRIVATE KEY-----", r"\bAKIA[0-9A-Z]{16}\b", r"\bgh[pousr]_[A-Za-z0-9]{36,}\b", r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b",
              r"\b10\.8\.\d{1,3}\.\d{1,3}\b", r"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\."]
hits = []
for rel in ("reference/series-contract.json", "reference/contract.json", "reference/test-vectors.json", "HANDOFF-SERIES.md", "MANIFEST.json"):
    text = open(p(rel), encoding="utf-8").read()
    hits += [(rel, rx) for rx in secret_res if re.search(rx, text)]
    hits += [(rel, n) for n in ["/home/", "/tmp/"] + extra_absent if n in text]
check("text files: no credential shape, private IP or non-fixture path", not hits, str(hits))

print()
if failures:
    print(f"SERIES NOT VERIFIED ({failures} check(s) failed)")
    sys.exit(1)
print("SERIES VERIFIED: profile=observation policy=observation-v1 format=" + FORMAT + " registry_schema=5 exports=" + ",".join(LABELS))
