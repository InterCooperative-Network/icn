#!/usr/bin/env python3
"""
Validate fake governed-bridge conformance fixtures.

Scope: the first machine-checkable conformance slice for the governed-bridge
spec set (ICN docs/spec/governed-bridge-*.md, derived from the NYCN airlock lane
and the requirements note docs/architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md).

What this is
------------
A LOCAL, offline check over **fake** fixtures that models the minimum chain

    binding -> dry-run -> steward review -> expected receipts

and proves the minimum invariants the six governed-bridge docs assert. It reads
no real source system, writes to no ICN node, reaches no network, imports no real
data, and touches nothing outside the repo.

What this is NOT
---------------
- not runtime bridge behaviour; not a connector; not a real import;
- not an ICN node write; not pilot-readiness.

It validates one or more fixture folders under
`tools/bridge-conformance/*/`, each containing:

    README.md
    binding.example.yaml
    dry-run.example.yaml
    steward-review.example.yaml
    expected-receipts.example.yaml

Usage
-----
  python3 tools/validate-governed-bridge-conformance.py
  python3 tools/validate-governed-bridge-conformance.py tools/bridge-conformance/review-coverage-v0

With no arguments it discovers every `tools/bridge-conformance/*/` folder. With
arguments, each argument is a fixture folder to check (must resolve inside the
repo).

Exit codes
----------
0  all checks pass.
1  any check fails, a required file is missing, or a fixture is malformed.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

try:
    import yaml
except Exception:  # pragma: no cover
    sys.stderr.write(
        "validate-governed-bridge-conformance.py: PyYAML is required but not "
        "installed. Install it (`pip install pyyaml`) to run this validator.\n"
    )
    sys.exit(1)

REPO_ROOT = Path(__file__).resolve().parent.parent
CONFORMANCE_DIR = REPO_ROOT / "tools" / "bridge-conformance"


def _within_repo(p) -> bool:
    """True if p resolves (symlinks collapsed) to a path inside the repo.

    Guards both explicit args and auto-discovered fixtures against a symlink
    that escapes the repository boundary.
    """
    try:
        Path(p).resolve().relative_to(REPO_ROOT.resolve())
        return True
    except (ValueError, OSError):
        return False

REQUIRED_FILES = (
    "README.md",
    "binding.example.yaml",
    "dry-run.example.yaml",
    "steward-review.example.yaml",
    "expected-receipts.example.yaml",
)

# --- governed-bridge vocabulary (from the six specs) ------------------------- #
CUSTODY_KINDS = frozenset(
    {
        "scoped_vault",
        "artifact_registry",
        "governed_object",
        "external_reference",
        "policy_gate",
        "policy_block",
        "discard",
    }
)
# custody-target kind -> the single target-specific receipt an approved write
# requires. governed_object is deliberately NOT here: it is floor-checked against
# the GOVERNED_OBJECT_CREATION_RECEIPTS family (a single name would re-narrow it).
TARGET_RECEIPT = {
    "scoped_vault": "VaultObjectWriteReceipt",
    "artifact_registry": "ArtifactRegistrationReceipt",
    "external_reference": "ExternalReferenceObservationReceipt",
}
# governed-object creation receipts: the generic receipt plus class-specific
# instances that satisfy the governed-object floor while the governed-object
# model reconciles. binding.required_receipts pins the per-field expectation.
GOVERNED_OBJECT_CREATION_RECEIPTS = frozenset(
    {
        "GovernedObjectCreationReceipt",  # generic
        "FollowUpObjectCreationReceipt",  # follow-up class instance
    }
)
# governed_object is a write kind (family-floor-checked), the rest map to a
# single TARGET_RECEIPT name.
WRITE_KINDS = frozenset(TARGET_RECEIPT.keys()) | {"governed_object"}
POLICY_BLOCK_KINDS = frozenset({"policy_gate", "policy_block"})
DISCARD_KIND = "discard"

DECISION_VERBS = frozenset(
    {
        "approve",
        "reject",
        "hold",
        "block",
        "discard",
        "request_reobservation",
        "request_reclassification",
        "request_member_consent_review",
    }
)

POLICY_BLOCK_RECEIPT_RE = re.compile(r"(?:PolicyBlock|ConsentBlock)Receipt")
FORBIDDEN_RECEIPT = "ActionCardCreationReceipt"
# ArtifactReceipt (implemented transfer proof) must never satisfy registration.
ARTIFACT_TRANSFER_RECEIPT = "ArtifactReceipt"
REGISTRATION_RECEIPT = "ArtifactRegistrationReceipt"

# --- privacy / fake-data scan patterns (conservative) ------------------------ #
EMAIL_RE = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
PHONE_RE = re.compile(r"(?<!\d)(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}(?!\d)")
DOLLAR_RE = re.compile(r"\$\s?\d")
# A raw URL that is not a docs-local relative link.
URL_RE = re.compile(r"(?i)\b(?:https?|ftp)://\S+")
CRED_ASSIGN_RE = re.compile(
    r"^\s*[-\s\"']*\b("
    r"password|passwd|api[_-]?key|apikey|secret[_-]?key|access[_-]?token|"
    r"private[_-]?key|client[_-]?secret|auth[_-]?token"
    r")\b\s*[:=]\s*(\S.*)$",
    re.IGNORECASE,
)
CRED_PLACEHOLDER_RE = re.compile(
    r"(?i)^[\"'\s]*(redacted|placeholder|example|none|null|tbd|n/a|<[^>]+>|x{3,}|\*{3,})"
)
# Payment/settlement *processing* language (data, not prohibition prose).
PAYMENT_ASSIGN_RE = re.compile(
    r"(?im)^\s*[-\s]*\b(process_settlement|process_payment|settle_payment|"
    r"charge_card|payment_instrument|wallet_address|crypto_token)\b\s*:\s*(\S+)"
)
_ALLOWED_EMAIL_DOMAINS = frozenset({"example.org", "example.com", "example.net"})


def _is_allowed_email_domain(domain: str) -> bool:
    d = domain.lower().rstrip(".")
    return (
        d in _ALLOWED_EMAIL_DOMAINS
        or d.endswith(".example.org")
        or d.endswith(".example.com")
        or d.endswith(".example.net")
        or d.endswith(".example")
    )


# --------------------------------------------------------------------------- #
# typed accessors — never crash on malformed YAML, never str()-coerce bad types
# --------------------------------------------------------------------------- #
def require_mapping(node, label, errors):
    if not isinstance(node, dict):
        errors.append(f"{label}: expected a mapping, got {type(node).__name__}")
        return {}
    return node


def require_list(node, label, errors):
    if not isinstance(node, list):
        errors.append(f"{label}: expected a list, got {type(node).__name__}")
        return []
    return node


def require_str(node, label, errors):
    if not isinstance(node, str) or not node.strip():
        errors.append(f"{label}: expected a non-empty string, got {type(node).__name__}")
        return None
    return node


def require_bool(node, label, errors, want=None):
    if not isinstance(node, bool):
        errors.append(f"{label}: expected a boolean, got {type(node).__name__}")
        return None
    if want is not None and node is not want:
        errors.append(f"{label}: must be {str(want).lower()}")
    return node


def require_str_list(node, label, errors):
    lst = require_list(node, label, errors)
    out = []
    for i, v in enumerate(lst):
        if isinstance(v, str) and v.strip():
            out.append(v)
        else:
            errors.append(f"{label}[{i}]: expected a non-empty string")
    return out


def _load_yaml(path: Path, label, errors):
    if not _within_repo(path):
        errors.append(f"{label}: file escapes the repository (symlink); refusing to read")
        return None
    try:
        with open(path, "r", encoding="utf-8") as fh:
            return yaml.safe_load(fh)
    except Exception as e:
        errors.append(f"{label}: YAML parse failed: {e}")
        return None


_OPAQUE_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]*$")


def _is_opaque_ref(s: str) -> bool:
    """An opaque/hash-bound id: no @, no scheme, no whitespace, not phone/URL."""
    if not isinstance(s, str) or not s:
        return False
    if EMAIL_RE.search(s) or URL_RE.search(s) or PHONE_RE.search(s):
        return False
    if "@" in s or "/" in s or " " in s:
        return False
    return bool(_OPAQUE_ID_RE.match(s))


# --------------------------------------------------------------------------- #
# per-file checks
# --------------------------------------------------------------------------- #
def check_binding(data, errors, rel):
    b = require_mapping(data, rel, errors)
    if not b:
        return {}
    meta = require_mapping(b.get("meta"), f"{rel}: meta", errors)
    require_bool(meta.get("fictional"), f"{rel}: meta.fictional", errors, want=True)
    if "imports_real_data" in meta:
        require_bool(
            meta.get("imports_real_data"),
            f"{rel}: meta.imports_real_data",
            errors,
            want=False,
        )

    for key in (
        "binding_id",
        "tool_manifest_id",
        "receipt_sink",
        "steward_review_surface",
    ):
        require_str(b.get(key), f"{rel}: {key}", errors)
    allowed_sources = require_str_list(b.get("allowed_source_systems"), f"{rel}: allowed_source_systems", errors)
    require_str_list(b.get("promotion_gates"), f"{rel}: promotion_gates", errors)
    require_str_list(b.get("required_receipts"), f"{rel}: required_receipts", errors)
    require_str_list(
        b.get("export_delete_recovery_refs"), f"{rel}: export_delete_recovery_refs", errors
    )
    require_mapping(
        b.get("external_reference_policy"), f"{rel}: external_reference_policy", errors
    )

    rrre = b.get("real_row_read_enabled")
    require_bool(rrre, f"{rel}: real_row_read_enabled", errors)

    fcm = require_mapping(b.get("field_custody_map"), f"{rel}: field_custody_map", errors)
    if not fcm:
        errors.append(f"{rel}: field_custody_map must not be empty")

    field_map = {}  # field_path -> (kind, required_receipts list)
    for field_path, spec in fcm.items():
        lbl = f"{rel}: field_custody_map[{field_path!r}]"
        s = require_mapping(spec, lbl, errors)
        require_str(s.get("source_authority"), f"{lbl}.source_authority", errors)
        require_str(s.get("privacy_class"), f"{lbl}.privacy_class", errors)
        ct = require_mapping(s.get("custody_target"), f"{lbl}.custody_target", errors)
        kind = require_str(ct.get("kind"), f"{lbl}.custody_target.kind", errors)
        if kind is not None and kind not in CUSTODY_KINDS:
            errors.append(
                f"{lbl}.custody_target.kind {kind!r} is not an allowed custody kind "
                f"{sorted(CUSTODY_KINDS)}"
            )
        rr = require_str_list(s.get("required_receipts"), f"{lbl}.required_receipts", errors)
        for r in rr:
            if r == FORBIDDEN_RECEIPT:
                errors.append(f"{lbl}.required_receipts: {FORBIDDEN_RECEIPT} must not be used")
        field_map[field_path] = (kind, rr)

    # real_row_read must be false unless every promotion gate is marked satisfied.
    gates_satisfied = b.get("promotion_gates_satisfied")
    if rrre is True:
        if gates_satisfied is not True:
            errors.append(
                f"{rel}: real_row_read_enabled is true but promotion_gates_satisfied "
                f"is not explicitly true"
            )

    return {
        "binding_id": b.get("binding_id"),
        "field_map": field_map,
        "external_reference_policy": b.get("external_reference_policy") or {},
        "allowed_source_systems": allowed_sources,
    }


def check_dry_run(data, binding_ctx, errors, rel):
    d = require_mapping(data, rel, errors)
    if not d:
        return {}
    meta = require_mapping(d.get("meta"), f"{rel}: meta", errors)
    require_bool(meta.get("fictional"), f"{rel}: meta.fictional", errors, want=True)
    require_bool(meta.get("performs_writes"), f"{rel}: meta.performs_writes", errors, want=False)

    for key in ("dry_run_id", "binding_id", "source_system_id", "source_shape_ref", "plan_hash"):
        require_str(d.get(key), f"{rel}: {key}", errors)
    if d.get("binding_id") != binding_ctx.get("binding_id"):
        errors.append(f"{rel}: binding_id does not match binding.example.yaml")
    allowed_sources = binding_ctx.get("allowed_source_systems") or []
    ssid = d.get("source_system_id")
    if isinstance(ssid, str) and allowed_sources and ssid not in allowed_sources:
        errors.append(
            f"{rel}: source_system_id {ssid!r} is not in binding allowed_source_systems"
        )

    field_map = binding_ctx.get("field_map", {})
    actions = require_list(d.get("proposed_actions"), f"{rel}: proposed_actions", errors)
    action_index = {}  # action_id -> (src_ref, field_path, kind)
    coverage = {}  # (src_ref, field_path) -> action_id
    proposed_fields = set()  # every valid field_path proposed, regardless of action_id validity
    for i, act in enumerate(actions):
        lbl = f"{rel}: proposed_actions[{i}]"
        a = require_mapping(act, lbl, errors)
        aid = require_str(a.get("action_id"), f"{lbl}.action_id", errors)
        src = require_str(a.get("source_record_ref"), f"{lbl}.source_record_ref", errors)
        fld = require_str(a.get("field_path"), f"{lbl}.field_path", errors)
        require_str(a.get("source_authority"), f"{lbl}.source_authority", errors)
        require_str(a.get("privacy_class"), f"{lbl}.privacy_class", errors)
        require_str_list(a.get("required_receipts"), f"{lbl}.required_receipts", errors)
        require_bool(a.get("decision_required"), f"{lbl}.decision_required", errors, want=True)

        pct = require_mapping(a.get("proposed_custody_target"), f"{lbl}.proposed_custody_target", errors)
        kind = require_str(pct.get("kind"), f"{lbl}.proposed_custody_target.kind", errors)

        if src is not None and not _is_opaque_ref(src):
            errors.append(
                f"{lbl}.source_record_ref {src!r} is not opaque/hash-bound "
                f"(no email/phone/URL/natural key)"
            )
        if fld is not None and field_map and fld not in field_map:
            errors.append(f"{lbl}.field_path {fld!r} is not in the binding field_custody_map")
        elif fld in field_map and kind is not None:
            bound_kind = field_map[fld][0]
            if bound_kind is not None and kind != bound_kind:
                errors.append(
                    f"{lbl}.proposed_custody_target.kind {kind!r} != binding's "
                    f"{bound_kind!r} for field {fld!r}"
                )
        if fld is not None:
            proposed_fields.add(fld)
        if aid:
            action_index[aid] = (src, fld, kind)
        if src is not None and fld is not None:
            key = (src, fld)
            if key in coverage:
                errors.append(
                    f"{lbl}: duplicate proposed action for (record, field) {key}"
                )
            else:
                coverage[key] = aid

    # Every binding custody rule must be exercised: a field_path declared in
    # field_custody_map that no proposed action references is an unexercised
    # (and therefore unreviewed, unreceipted) custody rule. proposed_fields is
    # collected independently of action_id validity so a broken action_id does
    # not cascade into a spurious coverage error here.
    for bound_field in field_map:
        if bound_field not in proposed_fields:
            errors.append(
                f"{rel}: binding field_custody_map[{bound_field!r}] is never exercised "
                f"by any dry-run proposed action"
            )

    return {
        "dry_run_id": d.get("dry_run_id"),
        "plan_hash": d.get("plan_hash"),
        "action_index": action_index,
        "coverage": coverage,
    }


def check_review(data, binding_ctx, dry_ctx, errors, rel):
    r = require_mapping(data, rel, errors)
    if not r:
        return {}
    meta = require_mapping(r.get("meta"), f"{rel}: meta", errors)
    require_bool(meta.get("fictional"), f"{rel}: meta.fictional", errors, want=True)

    for key in ("review_id", "dry_run_id", "binding_id", "reviewer_authority_ref", "reviewed_plan_hash"):
        require_str(r.get(key), f"{rel}: {key}", errors)
    # reviewer_display_role is allowed but never sufficient on its own.
    if r.get("reviewer_display_role") and not (isinstance(r.get("reviewer_authority_ref"), str) and r.get("reviewer_authority_ref").strip()):
        errors.append(
            f"{rel}: reviewer_display_role present but reviewer_authority_ref is missing — "
            f"a role label alone is not authority proof"
        )
    if r.get("binding_id") != binding_ctx.get("binding_id"):
        errors.append(f"{rel}: binding_id does not match binding.example.yaml")
    if r.get("dry_run_id") != dry_ctx.get("dry_run_id"):
        errors.append(f"{rel}: dry_run_id does not match dry-run.example.yaml")
    if r.get("reviewed_plan_hash") != dry_ctx.get("plan_hash"):
        errors.append(f"{rel}: reviewed_plan_hash != dry-run plan_hash")

    action_index = dry_ctx.get("action_index", {})
    decisions = require_list(r.get("decisions"), f"{rel}: decisions", errors)
    reviewed = {}  # (src, fld) -> decision verb
    decided_actions = {}  # action_id -> decision dict
    for i, dec in enumerate(decisions):
        lbl = f"{rel}: decisions[{i}]"
        dd = require_mapping(dec, lbl, errors)
        require_str(dd.get("decision_id"), f"{lbl}.decision_id", errors)
        aid = require_str(dd.get("action_id"), f"{lbl}.action_id", errors)
        src = require_str(dd.get("source_record_ref"), f"{lbl}.source_record_ref", errors)
        fld = require_str(dd.get("field_path"), f"{lbl}.field_path", errors)
        require_str(dd.get("reason"), f"{lbl}.reason", errors)
        verb = require_str(dd.get("decision"), f"{lbl}.decision", errors)
        exp = require_str_list(dd.get("expected_receipts"), f"{lbl}.expected_receipts", errors)

        if verb is not None and verb not in DECISION_VERBS:
            errors.append(f"{lbl}.decision {verb!r} is not an allowed decision verb {sorted(DECISION_VERBS)}")
        # a decision keyed only on a bare field name (no record ref) is a false-pass trap
        if src is None:
            errors.append(f"{lbl}: decision has no source_record_ref — bare field-name coverage is forbidden")
        for r_ in exp:
            if r_ == FORBIDDEN_RECEIPT:
                errors.append(f"{lbl}.expected_receipts: {FORBIDDEN_RECEIPT} must not appear")
        # automatic policy blocks must not carry a human review receipt
        automatic = dd.get("automatic") is True
        if verb == "block" and automatic and any("BridgeReviewDecisionReceipt" == x for x in exp):
            errors.append(
                f"{lbl}: an automatic policy block must not carry BridgeReviewDecisionReceipt "
                f"(it must not masquerade as human review)"
            )
        if aid:
            if aid in decided_actions:
                errors.append(f"{lbl}: duplicate decision for action_id {aid!r}")
            decided_actions[aid] = dd
            if aid not in action_index:
                errors.append(f"{lbl}.action_id {aid!r} does not match any dry-run action")
        if src is not None and fld is not None:
            key = (src, fld)
            if key in reviewed:
                errors.append(f"{lbl}: duplicate decision coverage for (record, field) {key}")
            else:
                reviewed[key] = verb

    # every proposed action gets exactly one decision (no silent gaps)
    dry_cov = dry_ctx.get("coverage", {})
    missing = sorted(set(dry_cov.keys()) - set(reviewed.keys()))
    for key in missing:
        errors.append(f"{rel}: no steward decision for proposed (record, field) {key}")
    extra = sorted(set(reviewed.keys()) - set(dry_cov.keys()))
    for key in extra:
        errors.append(f"{rel}: decision for (record, field) {key} has no matching dry-run action")

    return {"review_id": r.get("review_id"), "decided_actions": decided_actions, "reviewed": reviewed}


def check_expected_receipts(data, binding_ctx, dry_ctx, review_ctx, errors, rel):
    e = require_mapping(data, rel, errors)
    if not e:
        return
    meta = require_mapping(e.get("meta"), f"{rel}: meta", errors)
    require_bool(meta.get("fictional"), f"{rel}: meta.fictional", errors, want=True)
    for key in ("binding_id", "dry_run_id", "review_id"):
        require_str(e.get(key), f"{rel}: {key}", errors)
    if e.get("binding_id") != binding_ctx.get("binding_id"):
        errors.append(f"{rel}: binding_id does not match binding.example.yaml")
    if e.get("dry_run_id") != dry_ctx.get("dry_run_id"):
        errors.append(f"{rel}: dry_run_id does not match dry-run.example.yaml")
    if e.get("review_id") != review_ctx.get("review_id"):
        errors.append(f"{rel}: review_id does not match steward-review.example.yaml")

    exp = require_mapping(e.get("expected_receipts"), f"{rel}: expected_receipts", errors)
    run_level = require_str_list(exp.get("run_level"), f"{rel}: expected_receipts.run_level", errors)
    if "BridgeDryRunReceipt" not in run_level:
        errors.append(f"{rel}: expected_receipts.run_level must include BridgeDryRunReceipt (a dry-run ran)")
    if "BridgeReviewDecisionReceipt" not in run_level:
        errors.append(f"{rel}: expected_receipts.run_level must include BridgeReviewDecisionReceipt (a human reviewed)")
    for r_ in run_level:
        if r_ == FORBIDDEN_RECEIPT:
            errors.append(f"{rel}: {FORBIDDEN_RECEIPT} must not appear")

    per_action = require_list(exp.get("per_action"), f"{rel}: expected_receipts.per_action", errors)
    decided = review_ctx.get("decided_actions", {})
    action_index = dry_ctx.get("action_index", {})
    seen_actions = set()
    for i, pa in enumerate(per_action):
        lbl = f"{rel}: expected_receipts.per_action[{i}]"
        p = require_mapping(pa, lbl, errors)
        aid = require_str(p.get("action_id"), f"{lbl}.action_id", errors)
        kind = require_str(p.get("custody_target_kind"), f"{lbl}.custody_target_kind", errors)
        rc = require_str_list(p.get("receipts"), f"{lbl}.receipts", errors)
        if aid:
            if aid in seen_actions:
                errors.append(f"{lbl}: duplicate per_action entry for action_id {aid!r}")
            seen_actions.add(aid)
            if aid not in action_index:
                errors.append(f"{lbl}.action_id {aid!r} does not match any dry-run action")
        for r_ in rc:
            if r_ == FORBIDDEN_RECEIPT:
                errors.append(f"{lbl}.receipts: {FORBIDDEN_RECEIPT} must not appear")

        verb = None
        if aid in decided:
            verb = decided[aid].get("decision")
        # kind must match the dry-run action's proposed kind
        if aid in action_index and kind is not None:
            proposed_kind = action_index[aid][2]
            if proposed_kind is not None and kind != proposed_kind:
                errors.append(f"{lbl}.custody_target_kind {kind!r} != dry-run's {proposed_kind!r} for {aid!r}")

        # receipt-vocabulary rules per outcome
        if verb == "approve" and kind in WRITE_KINDS:
            if "BridgeImportReceipt" not in rc:
                errors.append(f"{lbl}: approved write to {kind!r} must include BridgeImportReceipt")
            if kind == "governed_object":
                # Family floor: the generic GovernedObjectCreationReceipt or a
                # class instance (e.g. the follow-up receipt). The binding's
                # required_receipts pins which one each field expects.
                if not any(r_ in GOVERNED_OBJECT_CREATION_RECEIPTS for r_ in rc):
                    errors.append(
                        f"{lbl}: approved write to 'governed_object' must include a "
                        f"governed-object creation receipt (one of "
                        f"{sorted(GOVERNED_OBJECT_CREATION_RECEIPTS)})"
                    )
            else:
                target = TARGET_RECEIPT[kind]
                if target not in rc:
                    errors.append(f"{lbl}: approved write to {kind!r} must include {target}")
            if kind == "artifact_registry" and ARTIFACT_TRANSFER_RECEIPT in rc and REGISTRATION_RECEIPT not in rc:
                errors.append(
                    f"{lbl}: {ARTIFACT_TRANSFER_RECEIPT} does not satisfy {REGISTRATION_RECEIPT} "
                    f"(transfer proof is not registry registration)"
                )
            # The binding is the per-run receipt contract: an approved write must
            # expect every receipt its field's custody rule declares. Scoped to
            # approved writes only — hold/block/discard outcomes must not be
            # forced to carry write receipts.
            if aid in action_index:
                fld = action_index[aid][1]
                bound = binding_ctx.get("field_map", {}).get(fld) if fld is not None else None
                if bound is not None:
                    for br in bound[1]:
                        if br not in rc:
                            errors.append(
                                f"{lbl}: approved write for field {fld!r} must include "
                                f"binding-declared receipt {br!r}"
                            )
        if verb == "discard" or kind == DISCARD_KIND:
            if "DiscardDecisionReceipt" not in rc:
                errors.append(f"{lbl}: a discard must include DiscardDecisionReceipt")
        if kind in POLICY_BLOCK_KINDS or verb == "block":
            if not any(POLICY_BLOCK_RECEIPT_RE.search(x) for x in rc):
                errors.append(f"{lbl}: a policy block must include a policy-block receipt (…PolicyBlock/…ConsentBlock)")

    # every decided action that has a receipt need should appear in per_action
    for aid in action_index:
        if aid not in seen_actions:
            errors.append(f"{rel}: expected_receipts.per_action is missing action {aid!r}")


# --------------------------------------------------------------------------- #
# privacy / fake-data scan over every file in the fixture dir
# --------------------------------------------------------------------------- #
def scan_privacy(folder: Path, errors, rel_folder):
    for name in sorted(os.listdir(folder)):
        path = folder / name
        if not path.is_file():
            continue
        if not _within_repo(path):
            errors.append(f"{rel_folder}/{name}: file escapes the repository (symlink); refusing to read")
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except Exception as e:
            errors.append(f"{rel_folder}/{name}: could not read: {e}")
            continue
        for lineno, line in enumerate(text.splitlines(), start=1):
            where = f"{rel_folder}/{name}:{lineno}"
            for m in EMAIL_RE.finditer(line):
                domain = m.group(0).rsplit("@", 1)[-1]
                if not _is_allowed_email_domain(domain):
                    errors.append(f"{where}: non-reserved email domain {domain!r}")
            if PHONE_RE.search(line):
                errors.append(f"{where}: phone-number-looking string")
            if DOLLAR_RE.search(line):
                errors.append(f"{where}: dollar-amount-looking string")
            for m in URL_RE.finditer(line):
                errors.append(f"{where}: raw URL {m.group(0)[:60]!r} (fixtures must not carry external URLs)")
            m = CRED_ASSIGN_RE.match(line)
            if m and not CRED_PLACEHOLDER_RE.match(m.group(2)):
                errors.append(f"{where}: credential-looking assignment for {m.group(1)!r}")
            m = PAYMENT_ASSIGN_RE.match(line)
            if m:
                errors.append(f"{where}: payment/settlement-processing field {m.group(1)!r} (observe-only, never process)")
            if FORBIDDEN_RECEIPT in line:
                errors.append(f"{where}: {FORBIDDEN_RECEIPT} must not appear (action cards are derived read views)")


# --------------------------------------------------------------------------- #
# per-folder driver
# --------------------------------------------------------------------------- #
def check_folder(folder: Path) -> list:
    errors: list = []
    rel = os.path.relpath(folder, REPO_ROOT)

    missing = [f for f in REQUIRED_FILES if not (folder / f).is_file()]
    for f in missing:
        errors.append(f"{rel}: required file missing: {f}")
    if any(m in {"binding.example.yaml", "dry-run.example.yaml", "steward-review.example.yaml", "expected-receipts.example.yaml"} for m in missing):
        return errors

    binding = _load_yaml(folder / "binding.example.yaml", f"{rel}/binding.example.yaml", errors)
    dry = _load_yaml(folder / "dry-run.example.yaml", f"{rel}/dry-run.example.yaml", errors)
    review = _load_yaml(folder / "steward-review.example.yaml", f"{rel}/steward-review.example.yaml", errors)
    receipts = _load_yaml(folder / "expected-receipts.example.yaml", f"{rel}/expected-receipts.example.yaml", errors)
    if errors:
        # parse failure(s) — do not attempt structural checks on None
        scan_privacy(folder, errors, rel)
        return errors

    binding_ctx = check_binding(binding, errors, f"{rel}/binding.example.yaml")
    dry_ctx = check_dry_run(dry, binding_ctx, errors, f"{rel}/dry-run.example.yaml")
    review_ctx = check_review(review, binding_ctx, dry_ctx, errors, f"{rel}/steward-review.example.yaml")
    check_expected_receipts(receipts, binding_ctx, dry_ctx, review_ctx, errors, f"{rel}/expected-receipts.example.yaml")

    scan_privacy(folder, errors, rel)
    return errors


def discover_folders(errors=None) -> list:
    """Discover fixture folders under tools/bridge-conformance/*.

    A symlinked entry is rejected outright and never followed: discovery must
    not walk a symlinked fixture dir, whether it points in-repo or escapes the
    tree. `is_dir()` alone follows symlinks, so the `is_symlink()` guard runs
    first. Explicit argv paths keep their own `_within_repo` resolution guard in
    main(); this closes the discovery-mode vector.
    """
    if not CONFORMANCE_DIR.is_dir():
        return []
    out = []
    for p in sorted(CONFORMANCE_DIR.iterdir(), key=lambda x: x.name):
        if p.name.startswith("."):
            continue
        if p.is_symlink():
            if errors is not None:
                errors.append(
                    f"{os.path.relpath(p, REPO_ROOT)}: symlinked fixture dir is not "
                    f"allowed in discovery (refusing to follow a symlink under "
                    f"tools/bridge-conformance/)"
                )
            continue
        if p.is_dir():
            out.append(p)
    return out


def main(argv) -> int:
    print("==> validate-governed-bridge-conformance.py")

    total_errors = 0
    # Boundary-check BOTH explicit args and auto-discovered folders: a symlinked
    # fixture dir must not let the validator read outside the repository, and in
    # discovery mode a symlink must not be followed at all.
    discovery_errors: list = []
    candidates = [Path(a) for a in argv] if argv else discover_folders(discovery_errors)
    for e in discovery_errors:
        print(f"    FAIL: {e}")
        total_errors += 1
    folders = []
    for p in candidates:
        if not _within_repo(p):
            label = str(p) if argv else p.name
            print(f"    FAIL: {label}: path escapes the repository (symlink or outside); refusing to read")
            total_errors += 1
            continue
        folders.append(p)

    if not folders and total_errors == 0:
        print("    no fixture folders found under tools/bridge-conformance/*/")
        return 0

    for folder in folders:
        rel = os.path.relpath(folder, REPO_ROOT)
        if not folder.is_dir():
            print(f"    FAIL: {rel}: not a directory")
            total_errors += 1
            continue
        errs = check_folder(folder)
        if errs:
            print(f"    FAIL: {rel} ({len(errs)} issue(s)):")
            for e in errs:
                print(f"      - {e}")
            total_errors += len(errs)
        else:
            print(f"    ok: {rel}")

    print()
    if total_errors:
        print(f"==> result: FAIL ({total_errors} issue(s))")
        return 1
    print("==> result: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
