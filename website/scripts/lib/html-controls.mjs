// html-controls.mjs — structural detection of mutation affordances.
//
// ─── Why not a regexp ────────────────────────────────────────────────────────
//
// check-fixture-safety.mjs check 7 asserts that the public walkthrough stays
// read-only: under ADR-0027, anything that produces a receipt has to declare
// mandate, reversibility and the receipt before its confirm step, and the
// walkthrough declares none of those. It used to assert that with three
// hand-written patterns:
//
//     /<form\b/i
//     /<button[^>]*>\s*(confirm|submit|approve|vote|send)\b/i
//     /<input[^>]*type="submit"/i
//
// which miss controls that are structurally identical to ones they catch:
//
//   · `<input type='submit'>`      — single quotes; the pattern requires `"`
//   · `<input type=submit>`        — unquoted, and equally valid HTML
//   · `<input type = "submit">`    — spaces around `=`
//   · `<input value="Go" type="submit">` — matched, but only by luck of order
//   · `<button><span>Confirm</span></button>` — the label is nested, so
//     `[^>]*>\s*` never reaches the word
//   · `<button aria-label="Approve"><svg/></button>` — an icon button whose
//     only label is an attribute
//
// Every one of those ships a working control while the checker reports PASS.
// That is the same fail-open shape as CodeQL alert 106 on the script filter,
// in a check that CodeQL does not look at — so it would not have been found by
// waiting for the scanner.
//
// Reading the parsed tree removes the whole class at once: parse5 has already
// normalised quoting, spacing, casing and nesting by the time we ask a
// question, so there is no spelling left to miss.

import {
  getAttr,
  textContent,
  toDocument,
  walkElements,
} from "./html-tree.mjs";

/**
 * Words that turn a control into a commitment rather than a disclosure.
 *
 * Word-bounded on purpose: "Resend" and "Subsequent" are not "send" and
 * "sent". Kept identical to the list the regexps used, so this pass changes
 * how the check looks, not what it considers a mutation.
 */
const ACTION_WORDS = /\b(?:confirm|submit|approve|vote|send)\b/i;

/** `<input type=...>` values that submit a form. */
const SUBMITTING_INPUT_TYPES = new Set(["submit", "image"]);

/**
 * Everything that reads as this element's label to a person using the page.
 *
 * Text content covers nesting; `aria-label` and `title` cover icon-only
 * controls, whose visible label is an SVG and whose real label is an
 * attribute; `value` covers `<input type="submit" value="Confirm">`, where the
 * attribute *is* the button face.
 */
function labelOf(el) {
  return [
    textContent(el),
    getAttr(el, "aria-label") ?? "",
    getAttr(el, "title") ?? "",
    getAttr(el, "value") ?? "",
  ]
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

/** A short, quotable description of an element for an error message. */
function describe(el) {
  const type = getAttr(el, "type");
  const label = labelOf(el);
  const attr = type ? ` type="${type}"` : "";
  const text = label ? ` — "${label.slice(0, 40)}"` : "";
  return `<${el.tagName}${attr}>${text}`;
}

/**
 * Mutation affordances in a document.
 *
 * Accepts raw HTML or an already-parsed document, and throws on anything else
 * rather than reporting "nothing found" for input it could not read.
 *
 * Deliberately *not* included: `<a>` elements whose text happens to contain an
 * action word. Links navigate; treating them as controls would flag ordinary
 * prose like "Send us a note" and would widen this check beyond what it has
 * ever asserted.
 *
 * @param {string|object} htmlOrDocument
 * @returns {Array<{why: string, detail: string}>}
 */
export function findMutationControls(htmlOrDocument) {
  const document = toDocument(htmlOrDocument);
  const findings = [];

  for (const el of walkElements(document)) {
    const tag = el.tagName;
    const type = (getAttr(el, "type") ?? "").trim().toLowerCase();

    if (tag === "form") {
      findings.push({ why: "<form> element", detail: describe(el) });
      continue;
    }

    if (tag === "input" && SUBMITTING_INPUT_TYPES.has(type)) {
      findings.push({ why: "submit input", detail: describe(el) });
      continue;
    }

    if (tag === "button" && type === "submit") {
      findings.push({ why: "submit button", detail: describe(el) });
      continue;
    }

    // A control whose label commits to an action, whatever its type. This is
    // the rule that catches an icon button labelled only by aria-label, and a
    // button whose word sits inside a nested <span>.
    if (
      (tag === "button" || tag === "input") &&
      ACTION_WORDS.test(labelOf(el))
    ) {
      findings.push({
        why: "confirm/submit control",
        detail: describe(el),
      });
    }
  }

  return findings;
}
