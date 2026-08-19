// src/lib/headingSlug.ts — GitHub-compatible heading slugs.
//
// Extracted from markdown.ts so the algorithm can be tested directly:
// scripts/tests/heading-slug.test.mjs pins the behaviour that 700+ in-page
// anchors across the documentation corpus depend on.
//
// ─── Why this exists ─────────────────────────────────────────────────────────
//
// `marked` does not add `id` attributes to headings, which meant every in-page
// anchor in the entire documentation corpus resolved nowhere on the public
// site — a table of contents linking to `#configuration` scrolled nothing, and
// cross-page deep links landed at the top of the target document.
// scripts/check-links.mjs found 701 of these at once.
//
// The algorithm matches GitHub's, so anchors written for the repository view
// keep working here and a link copied from GitHub resolves on the website.

/**
 * The HTML entities `marked` emits from its `escape()` pass, plus the
 * equivalent numeric spellings of the apostrophe.
 *
 * Every character these denote — `&`, `<`, `>`, `"`, `'` — is removed a moment
 * later by the `[^\w\s-]` allowlist below. Replacing the entity with the empty
 * string is therefore exactly equivalent to decoding it and letting the
 * allowlist strip the result, for any single entity.
 *
 * It is not equivalent for a *doubly* escaped input, and that difference is the
 * point. The previous implementation decoded in five sequential passes, so
 * `&amp;lt;` became `&lt;` on the first pass and then `<` on the second — a
 * double-unescape (CodeQL js/double-escaping, alert 104). Because this single
 * pass never produces a `&`, no later pass can decode one, and the class of
 * defect is gone rather than filtered.
 *
 * The behavioural difference is confined to that doubly-escaped input, and
 * there the new result is the more correct one: a heading whose rendered text
 * content is the four characters `&lt;` slugs to `lt` on GitHub, because
 * GitHub's slugger drops `&` and `;`. The old code produced an empty slug.
 * Verified against every ATX heading in the repository's docs tree (22,412
 * headings, 44,876 comparisons including each heading's `marked` inline
 * rendering): the two implementations agree everywhere except that input.
 */
const MARKED_ENTITIES = /&(?:amp|lt|gt|quot|#0*39|#[xX]0*27);/g;

/**
 * Remove HTML tags, repeating until the string stops changing.
 *
 * The input is `marked`'s inline output, so in practice one pass suffices —
 * headings contain `<em>`, `<code>`, `<strong>` and nothing nested inside a
 * tag. Iterating to a fixed point costs nothing there and removes the
 * single-pass assumption CodeQL flags (js/incomplete-multi-character-
 * sanitization, alert 105): one pass over `<scr<x>ipt>` can reassemble a tag
 * that was not present in the input.
 *
 * Termination: each iteration either removes at least one character or leaves
 * the string identical, and the identical case returns.
 *
 * This is a *semantic* step, not a security boundary — it exists so that
 * `<a href="x">Link</a>` slugs to `link` and not `a hrefx link`. The security
 * property is supplied unconditionally by the allowlist in `slugifyHeading`.
 */
function stripTags(text: string): string {
  let out = text;
  for (;;) {
    const next = out.replace(/<[^>]*>/g, "");
    if (next === out) return next;
    out = next;
  }
}

/**
 * The GitHub-compatible slug for one heading's rendered inline text.
 *
 * Lowercase, drop anything that is not alphanumeric, underscore, whitespace or
 * hyphen, then turn each whitespace character into one hyphen.
 *
 * The return value is guaranteed to match `/^[\w-]*$/` — the allowlist removes
 * every `<`, `>`, `"`, `'` and `&` regardless of what the earlier steps did, so
 * the result cannot close an attribute or open an element when interpolated
 * into `id="..."`. scripts/tests/heading-slug.test.mjs asserts this over a
 * corpus of injection attempts.
 */
export function slugifyHeading(text: string): string {
  return (
    stripTags(text)
      .replace(MARKED_ENTITIES, "")
      .trim()
      .toLowerCase()
      .replace(/[^\w\s-]/g, "")
      // Each space becomes one hyphen — NOT a collapsed run. GitHub does the
      // same, which is why "5. Storage & Replication" slugs to
      // "5-storage--replication" with a double hyphen: removing "&" leaves two
      // adjacent spaces. Collapsing them here broke every TOC anchor in the
      // corpus that contained an ampersand or a similar stripped character.
      .replace(/\s/g, "-")
  );
}

/**
 * A slugger scoped to one rendered page.
 *
 * Duplicate headings on a page get a numeric suffix, matching GitHub: the
 * second "Overview" on a page is `overview-1`.
 */
export function createHeadingSlugger(): (text: string) => string {
  const counts = new Map<string, number>();
  return function headingSlug(text: string): string {
    const base = slugifyHeading(text);
    const seen = counts.get(base) ?? 0;
    counts.set(base, seen + 1);
    return seen === 0 ? base : `${base}-${seen}`;
  };
}
