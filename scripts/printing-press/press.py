#!/usr/bin/env python3
"""ICN Printing Press — render markdown to designed print PDFs.

Usage:
  press.py <input.md> <output.pdf> [--theme NAME] [--no-cover]
                                   [--title TITLE] [--subtitle SUB]
                                   [--eyebrow EYEBROW] [--meta META]
                                   [--dropcap]

Examples:
  press.py docs/strategy/ICN_FOR_EVERYONE.md \\
           "out/ICN in Plain English.pdf" \\
           --theme plain --dropcap \\
           --eyebrow "INTERCOOPERATIVE NETWORK" \\
           --subtitle "A friendly introduction for anyone curious about cooperative infrastructure"

Themes (in scripts/printing-press/themes/):
  civic     — serious, peer-to-peer, navy accent, Georgia body
  plain     — warm, welcoming, terracotta accent, Cambria body
  handbill  — high-impact, agitprop, Impact + Cherry red
  (add more by dropping a .css next to the existing ones)

Manifest mode (batch render):
  press.py --manifest scripts/printing-press/manifests/library.toml

Browser discovery: same as md-to-pdf.py — uses CHROME env var, then
shutil.which(), then common per-OS install paths. Set MD_TO_PDF_NO_SANDBOX=1
to enable --no-sandbox in CI environments.
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

try:
    import markdown
except ImportError:
    sys.stderr.write(
        "ERROR: the 'markdown' package is required.\n"
        "Install with: python -m pip install markdown\n"
    )
    sys.exit(2)

HERE = Path(__file__).resolve().parent
THEMES_DIR = HERE / "themes"
TEMPLATES_DIR = HERE / "templates"
DEFAULT_THEME = "civic"
SUPPORTED_THEMES = sorted(p.stem for p in THEMES_DIR.glob("*.css") if not p.stem.startswith("_"))


# -----------------------------------------------------------------------------
# Browser discovery
# -----------------------------------------------------------------------------
def find_chrome() -> str:
    env_override = os.environ.get("CHROME")
    if env_override and Path(env_override).exists():
        return env_override

    for name in ("chrome", "google-chrome", "chromium", "chromium-browser", "msedge"):
        found = shutil.which(name)
        if found:
            return found

    candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ]
    for path in candidates:
        if Path(path).exists():
            return path
    raise FileNotFoundError(
        "No Chrome/Chromium/Edge executable found. "
        "Set the CHROME env var to a browser path, or install one of: "
        "chrome, google-chrome, chromium, chromium-browser, msedge."
    )


# -----------------------------------------------------------------------------
# Markdown -> HTML
# -----------------------------------------------------------------------------
def strip_frontmatter(text: str) -> tuple[str, str]:
    """Pull a YAML-style frontmatter block off the top, return (html_chip, body)."""
    if not text.startswith("---"):
        return "", text
    rest = text[3:]
    end = rest.find("\n---")
    if end < 0:
        return "", text
    front_raw = rest[:end].strip()
    body = rest[end + 4 :].lstrip("\n")
    items = [ln.strip() for ln in front_raw.splitlines() if ln.strip()]
    if not items:
        return "", body
    chip = '<div class="frontmatter">' + " · ".join(items) + "</div>"
    return chip, body


def md_to_body_html(body: str) -> str:
    return markdown.markdown(
        body,
        extensions=["tables", "fenced_code", "sane_lists", "smarty"],
    )


def first_h1(text: str) -> str | None:
    """Find first markdown H1 (# Foo) for cover title fallback."""
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("# ") and not s.startswith("## "):
            return s[2:].strip()
    return None


# -----------------------------------------------------------------------------
# Theme + template assembly
# -----------------------------------------------------------------------------
def load_theme(theme: str) -> tuple[str, str]:
    """Return (base_css, theme_css). Raises if theme missing."""
    base_path = THEMES_DIR / "_base.css"
    theme_path = THEMES_DIR / f"{theme}.css"
    if not theme_path.exists():
        avail = ", ".join(SUPPORTED_THEMES)
        raise SystemExit(f"theme '{theme}' not found. available: {avail}")
    return base_path.read_text(encoding="utf-8"), theme_path.read_text(encoding="utf-8")


def render_cover(opts: dict) -> str:
    if not opts.get("show_cover", True):
        return ""
    eyebrow = opts.get("eyebrow") or ""
    title = opts.get("title") or "Untitled"
    subtitle = opts.get("subtitle") or ""
    meta = opts.get("meta") or ""
    parts = ['<section class="cover">']
    if eyebrow:
        parts.append(f'  <div class="eyebrow">{eyebrow}</div>')
    parts.append(f"  <h1>{title}</h1>")
    if subtitle:
        parts.append(f'  <p class="subtitle">{subtitle}</p>')
    if meta:
        parts.append(f'  <div class="meta">{meta}</div>')
    parts.append("</section>")
    return "\n".join(parts)


def strip_leading_h1(body: str) -> str:
    """Drop the first H1 (and any blank lines + leading blockquote) so it
    doesn't repeat after the cover-page title."""
    lines = body.splitlines()
    out = []
    i = 0
    n = len(lines)
    found_h1 = False
    while i < n:
        line = lines[i]
        stripped = line.strip()
        # Skip leading blank lines
        if not found_h1 and not stripped:
            i += 1
            continue
        # Detect and skip the first H1
        if not found_h1 and stripped.startswith("# ") and not stripped.startswith("## "):
            found_h1 = True
            i += 1
            # Skip a single blank line + a single blockquote subtitle that
            # typically follows the H1 in our docs (the "> ..." paragraph).
            while i < n and not lines[i].strip():
                i += 1
            if i < n and lines[i].lstrip().startswith(">"):
                while i < n and lines[i].lstrip().startswith(">"):
                    i += 1
                while i < n and not lines[i].strip():
                    i += 1
            continue
        out.append(line)
        i += 1
    return "\n".join(out) if found_h1 else body


def build_html(md_text: str, opts: dict) -> str:
    chip, body = strip_frontmatter(md_text)

    # If we're rendering a cover, the H1 + its subtitle blockquote are
    # already represented on the cover. Don't repeat them in the body.
    if opts.get("show_cover", True):
        body = strip_leading_h1(body)

    body_html = md_to_body_html(body)

    if opts.get("hide_frontmatter"):
        chip = ""

    base_css, theme_css = load_theme(opts["theme"])
    cover_block = render_cover(opts)

    body_class = []
    if opts.get("dropcap"):
        body_class.append("dropcap")
    body_class_attr = f' class="{" ".join(body_class)}"' if body_class else ""

    template = (TEMPLATES_DIR / "document.html").read_text(encoding="utf-8")
    html = template
    html = html.replace("{{ lang }}", opts.get("lang", "en"))
    html = html.replace("{{ title }}", opts.get("title") or "Document")
    html = html.replace("{{ base_css }}", base_css)
    html = html.replace("{{ theme_css }}", theme_css)
    html = html.replace("{{ body_class_attr }}", body_class_attr)
    html = html.replace("{{ cover_block }}", cover_block)
    html = html.replace("{{ frontmatter_html }}", chip)
    html = html.replace("{{ body_html }}", body_html)
    return html


# -----------------------------------------------------------------------------
# Rendering
# -----------------------------------------------------------------------------
def render_pdf(html_path: Path, pdf_path: Path) -> None:
    chrome = find_chrome()
    file_url = "file:///" + str(html_path.resolve()).replace("\\", "/")
    cmd = [
        chrome,
        "--headless",
        "--disable-gpu",
        "--no-pdf-header-footer",
        f"--print-to-pdf={pdf_path}",
        file_url,
    ]
    if os.environ.get("MD_TO_PDF_NO_SANDBOX") == "1":
        cmd.insert(3, "--no-sandbox")
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write("Chrome stderr:\n" + result.stderr + "\n")
        sys.exit(result.returncode)


def render_one(md_path: Path, pdf_path: Path, opts: dict) -> None:
    text = md_path.read_text(encoding="utf-8")

    # Fill missing title from first H1 in body
    if not opts.get("title"):
        opts["title"] = first_h1(text) or md_path.stem

    html_path = pdf_path.with_suffix(".rendered.html")
    html_path.write_text(build_html(text, opts), encoding="utf-8")
    render_pdf(html_path, pdf_path)
    print(f"  {md_path.name}  ->  {pdf_path.name}  [theme: {opts['theme']}]")


# -----------------------------------------------------------------------------
# Manifest mode (batch render)
# -----------------------------------------------------------------------------
def run_manifest(manifest_path: Path) -> None:
    data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    defaults = data.get("defaults", {})
    documents = data.get("documents", [])
    if not documents:
        sys.stderr.write("manifest has no [[documents]] entries\n")
        sys.exit(2)
    print(f"Rendering {len(documents)} document(s) from {manifest_path.name}:")
    for entry in documents:
        opts = {**defaults, **entry}
        opts.setdefault("theme", DEFAULT_THEME)
        opts.setdefault("show_cover", True)
        md = Path(entry["input"]).expanduser().resolve()
        pdf = Path(entry["output"]).expanduser().resolve()
        pdf.parent.mkdir(parents=True, exist_ok=True)
        render_one(md, pdf, opts)


# -----------------------------------------------------------------------------
# CLI
# -----------------------------------------------------------------------------
def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="press",
        description="ICN Printing Press — render markdown to designed print PDFs.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=f"Available themes: {', '.join(SUPPORTED_THEMES)}",
    )
    p.add_argument("input", nargs="?", help="input markdown file")
    p.add_argument("output", nargs="?", help="output PDF file")
    p.add_argument("--theme", default=DEFAULT_THEME, help=f"theme name (default: {DEFAULT_THEME})")
    p.add_argument("--no-cover", action="store_true", help="omit the cover page")
    p.add_argument("--title", help="cover title (default: first H1 in document)")
    p.add_argument("--subtitle", help="cover subtitle / strapline")
    p.add_argument("--eyebrow", help="small uppercase label above the cover title")
    p.add_argument("--meta", help="small meta line at the bottom of the cover (e.g. date, version)")
    p.add_argument("--dropcap", action="store_true", help="enable drop-cap on first paragraph")
    p.add_argument("--hide-frontmatter", action="store_true", help="hide the Status/Canonical chip")
    p.add_argument("--manifest", help="render multiple documents defined in a TOML manifest")
    p.add_argument("--lang", default="en", help="HTML lang attribute (default: en)")
    return p.parse_args()


def main() -> None:
    args = parse_args()
    if args.manifest:
        run_manifest(Path(args.manifest))
        return
    if not args.input or not args.output:
        sys.stderr.write(__doc__)
        sys.exit(2)
    opts = {
        "theme": args.theme,
        "title": args.title,
        "subtitle": args.subtitle,
        "eyebrow": args.eyebrow,
        "meta": args.meta,
        "show_cover": not args.no_cover,
        "dropcap": args.dropcap,
        "hide_frontmatter": args.hide_frontmatter,
        "lang": args.lang,
    }
    render_one(Path(args.input), Path(args.output), opts)


if __name__ == "__main__":
    main()
