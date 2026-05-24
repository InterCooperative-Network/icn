#!/usr/bin/env python3
"""Render a markdown file to a print-styled PDF via headless Chrome.

Usage: md-to-pdf.py <input.md> <output.pdf>

Writes an intermediate file next to the output PDF named
``<output-stem>.rendered.html`` (e.g. ``foo.pdf`` -> ``foo.rendered.html``).

Browser discovery:
  - The ``CHROME`` or ``CHROME_BIN`` env var, if set, takes precedence.
  - Otherwise ``shutil.which`` searches PATH for: ``chrome``, ``google-chrome``,
    ``google-chrome-stable``, ``chromium``, ``chromium-browser``, ``msedge``.
  - Otherwise common per-OS install paths are tried as a last resort.

Security:
  - ``--no-sandbox`` is OFF by default. Set ``MD_TO_PDF_NO_SANDBOX=1`` to enable
    it for constrained CI environments where sandboxing is unavailable.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import markdown
except ImportError:
    sys.exit(
        "md-to-pdf.py: missing dependency 'markdown'. "
        "Install with: python -m pip install markdown"
    )


def resolve_chrome() -> str:
    """Resolve a Chrome / Chromium executable across Linux, macOS, Windows.

    Order: $CHROME or $CHROME_BIN env var, then PATH lookup for common names,
    then a handful of well-known install locations as a last resort.
    """
    if env := os.environ.get("CHROME") or os.environ.get("CHROME_BIN"):
        if Path(env).exists():
            return env
    for name in (
        "chrome",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "msedge",
    ):
        if path := shutil.which(name):
            return path
    well_known = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ]
    for candidate in well_known:
        if Path(candidate).exists():
            return candidate
    sys.exit(
        "md-to-pdf.py: no Chrome/Chromium found. "
        "Set CHROME or CHROME_BIN, or install Chrome and put it on PATH."
    )

CSS = """
:root {
  --ink: #1a1d21;
  --ink-soft: #4a4f55;
  --line: #d9dde2;
  --line-soft: #eceff3;
  --paper: #fcfcfa;
  --accent: #2b5876;
}
* { box-sizing: border-box; }
html { font-size: 14px; }
body {
  margin: 0;
  background: #f3f1eb;
  color: var(--ink);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", "Inter", system-ui, sans-serif;
  line-height: 1.5;
}
main {
  max-width: 780px;
  margin: 0 auto;
  padding: 36px 44px 60px;
  background: var(--paper);
  border-left: 1px solid var(--line);
  border-right: 1px solid var(--line);
}
h1 { font-size: 1.6rem; letter-spacing: -0.01em; line-height: 1.2; border-bottom: 2px solid var(--ink); padding-bottom: 10px; margin: 0 0 20px; }
h2 { font-size: 1.1rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--accent); margin: 28px 0 10px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
h3 { font-size: 1rem; margin: 20px 0 8px; color: var(--ink); }
h4 { font-size: 0.92rem; margin: 16px 0 6px; color: var(--ink); }
p { margin: 0 0 10px; }
ul, ol { margin: 6px 0 12px; padding-left: 22px; }
li { margin: 3px 0; }
blockquote { margin: 12px 0; padding: 10px 14px; border-left: 3px solid var(--accent); background: #fff; font-size: 0.97rem; line-height: 1.55; }
blockquote p:last-child { margin-bottom: 0; }
code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.85rem; background: var(--line-soft); padding: 1px 4px; border-radius: 3px; }
pre { background: #fff; border: 1px solid var(--line); padding: 10px 12px; border-radius: 4px; overflow-x: auto; font-size: 0.82rem; }
table { width: 100%; border-collapse: collapse; font-size: 0.86rem; margin: 8px 0 14px; background: #fff; }
th, td { border: 1px solid var(--line); padding: 6px 9px; vertical-align: top; text-align: left; }
th { background: var(--line-soft); font-size: 0.74rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--ink-soft); font-weight: 700; }
hr { border: 0; border-top: 1px solid var(--line); margin: 20px 0; }
.frontmatter { font-size: 0.78rem; color: var(--ink-soft); border: 1px solid var(--line); border-radius: 4px; padding: 6px 10px; margin-bottom: 14px; background: #fff; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
@media print {
  html, body { background: #fff; }
  main { max-width: 100%; margin: 0; padding: 22px 28px; border: none; }
  h2, h3 { page-break-after: avoid; }
  table, blockquote, pre { page-break-inside: avoid; }
}
"""


def strip_frontmatter(text: str) -> tuple[str, str]:
    """Pull a YAML-style frontmatter block off the top, return (frontmatter_html, body)."""
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
    html = '<div class="frontmatter">' + " · ".join(items) + "</div>"
    return html, body


def render(md_path: Path, pdf_path: Path) -> None:
    text = md_path.read_text(encoding="utf-8")
    front_html, body = strip_frontmatter(text)
    body_html = markdown.markdown(
        body,
        extensions=["tables", "fenced_code", "sane_lists"],
    )
    title = md_path.stem
    page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>{CSS}</style>
</head>
<body>
<main>
{front_html}
{body_html}
</main>
</body>
</html>
"""
    html_path = pdf_path.with_suffix(".rendered.html")
    html_path.write_text(page, encoding="utf-8")

    # Use forward-slash URI; Chrome on Windows accepts file:/// + UNC via file://wsl.localhost/...
    file_url = "file:///" + str(html_path.resolve()).replace("\\", "/")
    cmd = [resolve_chrome(), "--headless", "--disable-gpu"]
    if os.environ.get("MD_TO_PDF_NO_SANDBOX") == "1":
        cmd.append("--no-sandbox")
    cmd.extend([
        "--no-pdf-header-footer",
        f"--print-to-pdf={pdf_path}",
        file_url,
    ])
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print("Chrome stderr:", result.stderr, file=sys.stderr)
        sys.exit(result.returncode)
    print(f"Rendered {md_path.name} -> {pdf_path.name}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    render(Path(sys.argv[1]), Path(sys.argv[2]))
