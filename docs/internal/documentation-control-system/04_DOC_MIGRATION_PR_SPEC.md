# Documentation System Migration PR Specification

## Goal

Create a single PR that makes the ICN docs corpus meaningfully more legible, more structured, and safer for both humans and agents.

This PR should **not** attempt to perfect every document.  
It should establish the system that makes future perfection tractable.

---

## Required outcomes

### 1. Canonical control system added
The repo should include a formal development/documentation control-system doc.

### 2. Registry introduced or normalized
The repo should have:
- a human-readable registry
- machine-readable registry continuity

### 3. Truth classes applied
At least the top-level and high-value docs should have explicit truth classes.

### 4. Canonical docs identified
The entry docs should be obvious and explicitly marked.

### 5. Historical docs separated
Historical material should stop impersonating active canon.

### 6. Templates added
The repo should gain issue/PR/doc templates aligned with the new system.

---

## What belongs in this PR

- docs-system spec
- doc registry
- truth-class header pattern
- directory cleanup
- supersession markers
- updated index
- issue template
- PR template
- ADR/RFC/spec/status templates
- agent prompt templates

---

## What does NOT belong in this PR

- rewriting all architecture docs
- settling every design debate
- changing code semantics
- deleting large amounts of historical material
- speculative content expansion

---

## Required PR sections

### Summary
What changed and why.

### Structural changes
What directories/files were added, moved, archived, or marked.

### Registry/report
How many docs were classified, moved, archived, or flagged.

### Verification
How indexing, paths, and canonical references were checked.

### Open questions
What still requires human review.

---

## Acceptance criteria

The PR is good if:

- new contributors can find the right docs quickly
- agents can orient without loading the entire repo
- plans/specs/history are visibly distinct
- canonical docs are obvious
- archive drift is reduced
- no major knowledge loss occurred

