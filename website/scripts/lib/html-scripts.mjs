// html-scripts.mjs — inline <script> extraction backed by a real HTML5 parser.
//
// ─── Why not a regexp ────────────────────────────────────────────────────────
//
// check-fixture-safety.mjs used to pull inline scripts out with
//
//     html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)
//
// which CodeQL flagged as js/bad-tag-filter (alert 106), and it was right in
// the way that matters: the pattern **fails open**. The HTML spec allows
// whitespace before the `>` of an end tag, so `</script >`, `</script\n>` and
// `</script\t>` are all valid — and none of them match. A script block closed
// that way is not extracted at all, so its contents are never scanned, and the
// live-data check reports PASS on a page it never looked at. The same is true
// of an attribute value containing `>`, e.g. `<script data-x="a>b">`, where
// `[^>]*` stops early and the captured body is wrong.
//
// A checker that silently inspects nothing is worse than no checker, because
// the green result is read as evidence.
//
// ─── Why parse5 ──────────────────────────────────────────────────────────────
//
// parse5 is a spec-compliant HTML5 parser and it is already in this project's
// dependency tree: astro -> @astrojs/markdown-remark -> hast-util-from-html
// (and rehype-raw) all depend on parse5@7.3.0. Listing it explicitly as a
// devDependency at that same version pins what `npm ci` already installs, so
// this adds a correct parser at no download and no new transitive tree.
//
// The parser handles end-tag whitespace, attribute values containing `>`, tag
// casing, comments, CDATA and the raw-text tokenisation rules for <script> —
// all of the cases the regexp got wrong, by construction rather than by adding
// more alternations to a pattern.

import { parse } from "parse5";

/** True if `node` carries an attribute named `name` (case-insensitively). */
function hasAttr(node, name) {
  return (node.attrs ?? []).some((a) => a.name.toLowerCase() === name);
}

/** The concatenated text of a node's immediate `#text` children. */
function textOf(node) {
  return (node.childNodes ?? [])
    .filter((c) => c.nodeName === "#text")
    .map((c) => c.value)
    .join("");
}

function collect(node, out) {
  if (!node || typeof node !== "object") return;

  if (node.nodeName === "script" && !hasAttr(node, "src")) {
    out.push(textOf(node));
  }

  // A <template>'s children are parked on `.content`, off the main tree.
  // Scripts there are inert until the template is cloned, but a static safety
  // check should look at them anyway — the cost of a false positive is a
  // conversation, the cost of a false negative is an unreviewed script.
  if (node.content) collect(node.content, out);

  for (const child of node.childNodes ?? []) collect(child, out);
}

/**
 * Every inline `<script>` body in an HTML document, in document order.
 *
 * Elements carrying a `src` attribute are external references with no body and
 * are not returned.
 *
 * Throws if the document cannot be parsed. Callers must treat a throw as a
 * check failure and must not fall through to scanning an empty string — that
 * would restore exactly the fail-open behaviour this module exists to remove.
 *
 * @param {string} html
 * @returns {string[]}
 */
export function extractInlineScripts(html) {
  if (typeof html !== "string") {
    throw new TypeError(
      `extractInlineScripts: expected an HTML string, received ${typeof html}`,
    );
  }
  const out = [];
  collect(parse(html), out);
  return out;
}
