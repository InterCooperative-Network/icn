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
// parse5 is a spec-compliant HTML5 parser and it was already in this project's
// dependency tree: astro -> @astrojs/markdown-remark -> hast-util-from-html
// (and rehype-raw) all depend on parse5@7.3.0. Listing it explicitly as a
// devDependency at that same version pins what `npm ci` already installs, so
// this adds a correct parser at no download and no new transitive tree.
//
// The parser handles end-tag whitespace, attribute values containing `>`, tag
// casing, comments, CDATA and the raw-text tokenisation rules for <script> —
// all of the cases the regexp got wrong, by construction rather than by adding
// more alternations to a pattern.

import { elementsNamed, hasAttr, ownText, toDocument } from "./html-tree.mjs";

/**
 * Every inline `<script>` body in an HTML document, in document order.
 *
 * Elements carrying a `src` attribute are external references with no body and
 * are not returned.
 *
 * Accepts either raw HTML or a document already parsed by `parseHtml`, so a
 * caller running several structural checks can parse once and pass the tree
 * to each of them.
 *
 * Throws if the document cannot be parsed. Callers must treat a throw as a
 * check failure and must not fall through to scanning an empty string — that
 * would restore exactly the fail-open behaviour this module exists to remove.
 *
 * @param {string|object} htmlOrDocument
 * @returns {string[]}
 */
export function extractInlineScripts(htmlOrDocument) {
  return elementsNamed(toDocument(htmlOrDocument), "script")
    .filter((el) => !hasAttr(el, "src"))
    .map((el) => ownText(el));
}
