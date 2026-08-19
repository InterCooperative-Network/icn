// html-tree.mjs — the parse5 primitives the HTML safety checks share.
//
// One place that knows the shape of a parse5 node, so html-scripts.mjs and
// html-controls.mjs can ask structural questions ("which elements", "what is
// this element's label") instead of matching raw markup.
//
// Why this exists at all: every hand-written HTML tag regexp in this directory
// has eventually been found to miss a spelling that browsers accept. The
// script filter missed `</script >` (CodeQL alert 106) and the mutation-control
// filter missed `type='submit'` with single quotes. Both are the same mistake,
// and the fix for both is to stop reading markup as text.

import { parse } from "parse5";

/**
 * Parse an HTML document.
 *
 * Throws on a non-string. Callers must treat a throw as a check failure and
 * must not fall through to inspecting an empty tree — a check that silently
 * examines nothing reports PASS on a page it never read, which is worse than
 * having no check, because the green result is taken as evidence.
 *
 * @param {string} html
 */
export function parseHtml(html) {
  if (typeof html !== "string") {
    throw new TypeError(
      `parseHtml: expected an HTML string, received ${typeof html}`,
    );
  }
  return parse(html);
}

/**
 * Accept either raw HTML or an already-parsed document, and refuse anything
 * else.
 *
 * The refusal is the point. An earlier version of this shim treated "not a
 * string" as "already a document", so `undefined` walked an empty tree and
 * returned no findings — a checker reporting PASS because it had nothing to
 * look at. That is the same fail-open the parser was brought in to remove,
 * reintroduced one layer up.
 *
 * @param {string|object} htmlOrDocument
 */
export function toDocument(htmlOrDocument) {
  if (typeof htmlOrDocument === "string") return parseHtml(htmlOrDocument);
  if (
    htmlOrDocument &&
    typeof htmlOrDocument === "object" &&
    Array.isArray(htmlOrDocument.childNodes)
  ) {
    return htmlOrDocument;
  }
  throw new TypeError(
    `expected an HTML string or a parsed document, received ${
      htmlOrDocument === null ? "null" : typeof htmlOrDocument
    }`,
  );
}

/** parse5 gives elements a `tagName`; text, comment and doctype nodes do not. */
function isElement(node) {
  return typeof node?.tagName === "string";
}

/**
 * Every element in the document, in document order.
 *
 * Descends into `<template>` content, which parse5 parks on `.content` off the
 * main tree. A control inside a template is inert until the template is
 * cloned, but a static safety check should still see it: the cost of a false
 * positive is a conversation, the cost of a false negative is an unreviewed
 * control shipping on a page that claims to be read-only.
 *
 * @param {object} node
 * @returns {Generator<object>}
 */
export function* walkElements(node) {
  if (!node || typeof node !== "object") return;
  if (isElement(node)) yield node;
  if (node.content) yield* walkElements(node.content);
  for (const child of node.childNodes ?? []) yield* walkElements(child);
}

/** Elements whose tag name is `name` (parse5 lowercases tag names). */
export function elementsNamed(root, name) {
  const wanted = name.toLowerCase();
  return [...walkElements(root)].filter((el) => el.tagName === wanted);
}

/**
 * An attribute's value, or undefined.
 *
 * parse5 lowercases attribute *names* but not their values, so callers that
 * care about a value (`type="SUBMIT"`) must compare case-insensitively
 * themselves. Quoting and whitespace have already been normalised away by the
 * parser, which is the entire point: `type=submit`, `type='submit'`,
 * `type = "submit"` and `TYPE="Submit"` all arrive here identically.
 */
export function getAttr(node, name) {
  const wanted = name.toLowerCase();
  return (node.attrs ?? []).find((a) => a.name.toLowerCase() === wanted)?.value;
}

/** True if `node` carries attribute `name`, whatever its value. */
export function hasAttr(node, name) {
  return getAttr(node, name) !== undefined;
}

/** The concatenated text of a node's immediate `#text` children. */
export function ownText(node) {
  return (node.childNodes ?? [])
    .filter((c) => c.nodeName === "#text")
    .map((c) => c.value)
    .join("");
}

/**
 * All text beneath a node, descendants included.
 *
 * Nesting is why this is recursive rather than `ownText`:
 * `<button><span>Confirm</span></button>` has no text of its own, and a label
 * check that only read immediate children would not see the word at all.
 */
export function textContent(node) {
  if (!node || typeof node !== "object") return "";
  if (node.nodeName === "#text") return node.value ?? "";
  let out = "";
  if (node.content) out += textContent(node.content);
  for (const child of node.childNodes ?? []) out += textContent(child);
  return out;
}
