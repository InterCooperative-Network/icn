# ICN Printing Press

A tiny build system for turning markdown into designed print PDFs. Themed,
reproducible, scriptable.

```
scripts/printing-press/
├── press.py                  # main render script (CLI + manifest mode)
├── themes/                   # one .css per theme
│   ├── _base.css             # shared structural styles (don't edit casually)
│   ├── civic.css             # serious peer-to-peer, navy + Georgia
│   ├── plain.css             # warm public-facing, terracotta + Cambria
│   └── handbill.css          # high-impact one-pager, cherry + Impact
├── templates/
│   └── document.html         # base HTML wrapper
└── manifests/
    └── library.toml          # batch-render config for the public-literature set
```

---

## Quick start

Render a single file:

```bash
python scripts/printing-press/press.py \
  docs/strategy/ICN_FOR_EVERYONE.md \
  "out/ICN in Plain English.pdf" \
  --theme plain --dropcap \
  --eyebrow "INTERCOOPERATIVE NETWORK" \
  --subtitle "A friendly introduction for anyone curious."
```

Or render the whole library in one shot:

```bash
python scripts/printing-press/press.py --manifest scripts/printing-press/manifests/library.toml
```

---

## CLI flags

| Flag | What it does |
|---|---|
| `<input.md>` | Markdown source (positional, required unless `--manifest`) |
| `<output.pdf>` | PDF destination (positional, required unless `--manifest`) |
| `--theme NAME` | Theme to apply (`civic`, `plain`, `handbill`, or any new theme you add). Default: `civic`. |
| `--title TITLE` | Cover-page title. Defaults to the first H1 in the markdown. |
| `--subtitle SUB` | Cover-page subtitle / strapline. |
| `--eyebrow EYEBROW` | Small uppercase label above the cover title (e.g. `INTERCOOPERATIVE NETWORK`). |
| `--meta META` | Small bottom-of-cover line (e.g. date, version, URL). |
| `--no-cover` | Omit the cover page entirely. Use for short handouts or one-pagers. |
| `--dropcap` | Drop-cap on the first body paragraph (looks great with `plain`). |
| `--hide-frontmatter` | Hide the `Status:` / `Canonical:` chip that the strategy docs use. |
| `--manifest PATH` | Batch-render mode. See `manifests/library.toml` for example shape. |
| `--lang CODE` | HTML lang attribute. Default: `en`. |

---

## Themes

### `civic`
**For:** serious peer-to-peer documents (movement-facing briefings, technical literature, institutional explainers).
**Feel:** calm, confident, navy + Georgia. Looks like something a serious organization would publish.
**Use it for:** `ICN_FOR_COOPERATIVE_MOVEMENT.md`, any technical or strategic ICN document.

### `plain`
**For:** public-facing, plain-language documents.
**Feel:** warm, welcoming, terracotta + Cambria, slightly looser leading, optional drop cap. Less institutional.
**Use it for:** `ICN_FOR_EVERYONE.md`, anything aimed at people who aren't in the cooperative movement yet.

### `handbill`
**For:** one-page (or tight) call-to-action / event flyers.
**Feel:** larger type, uppercase headers, cherry-red accent, Impact font. Agitprop energy.
**Use it for:** event handouts, recruitment flyers, "what's a co-op" handbills.

---

## Adding a new theme

Drop a new `.css` file into `themes/` named `<your-theme>.css`. Override the CSS
variables defined in `_base.css` (look at `civic.css` / `plain.css` /
`handbill.css` for the pattern). The variables you must define:

```css
:root {
  --body-font: ...;
  --header-font: ...;
  --mono-font: ...;
  --body-leading: 1.55;       /* 1.5 - 1.75 reasonable range */
  --page-margin: 1.0in;

  --ink: #...;
  --ink-soft: #...;
  --ink-faint: #...;
  --paper: #...;
  --accent: #...;
  --accent-dark: #...;
  --rule: #...;
  --rule-strong: #...;
  --quote-bg: #...;
  --callout-bg: #...;
  --code-bg: #...;
  --table-bg: #...;
  --table-header-bg: #...;

  --cover-title-size: 4.0rem;
  --cover-title-weight: 700;
}
```

After that, the theme is available as `--theme <your-theme>`.

---

## Manifests (batch render)

A manifest lets you describe a whole set of documents and render them with one
command. Format: TOML.

```toml
[defaults]
show_cover = true
hide_frontmatter = true
meta = "Pre-pilot · Open source · intercooperative.network"

[[documents]]
input    = "docs/strategy/ICN_FOR_COOPERATIVE_MOVEMENT.md"
output   = "out/ICN for the Cooperative Movement.pdf"
theme    = "civic"
eyebrow  = "INTERCOOPERATIVE NETWORK"
title    = "Infrastructure for the Cooperative Movement"
subtitle = "Plain-English introduction for cooperative developers..."

[[documents]]
input    = "docs/strategy/ICN_FOR_EVERYONE.md"
output   = "out/ICN in Plain English.pdf"
theme    = "plain"
dropcap  = true
eyebrow  = "INTERCOOPERATIVE NETWORK"
title    = "ICN, in Plain English"
subtitle = "Friendly introduction..."
```

Any field under `[[documents]]` overrides the same field in `[defaults]`. Run:

```bash
python scripts/printing-press/press.py --manifest scripts/printing-press/manifests/library.toml
```

---

## How it works (briefly)

1. **Read markdown.** Strip YAML frontmatter (`---` blocks) into a small chip.
2. **Convert body to HTML** via the `markdown` package (with the `tables`,
   `fenced_code`, `sane_lists`, `smarty` extensions enabled).
3. **Assemble HTML** from `templates/document.html`: base CSS, theme CSS,
   optional cover, frontmatter chip, body.
4. **Render to PDF** via headless Chrome/Chromium/Edge with `--print-to-pdf`.
   Browser is discovered via the `CHROME` env var, `shutil.which()`, then
   common per-OS install paths.

No external services. No network calls. Reproducible.

---

## Browser discovery

The script looks for a Chromium-family browser in this order:

1. `CHROME` environment variable.
2. `shutil.which()` for: `chrome`, `google-chrome`, `chromium`,
   `chromium-browser`, `msedge`.
3. Common per-OS install paths (Windows: Program Files; macOS:
   /Applications; Linux: /usr/bin, /snap/bin).

Raises a clear error if none found.

To enable `--no-sandbox` in CI environments where Chrome's sandbox is
unavailable, set `MD_TO_PDF_NO_SANDBOX=1`. (It's off by default — security
footgun.)

---

## Dependencies

- Python 3.11+ (uses `tomllib` from stdlib)
- `markdown` package: `python -m pip install markdown`
- A Chromium-family browser somewhere discoverable
