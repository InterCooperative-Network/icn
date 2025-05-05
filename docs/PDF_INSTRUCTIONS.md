# Converting ICN Compute Commons to PDF

This document provides instructions for converting the ICN Compute Commons manifesto to a polished PDF document suitable for presentation and distribution.

## Option 1: Using Pandoc (Command Line)

[Pandoc](https://pandoc.org/) is a universal document converter that works well for converting Markdown to PDF.

### Prerequisites
- Install [Pandoc](https://pandoc.org/installing.html)
- Install a LaTeX distribution:
  - For Windows: [MiKTeX](https://miktex.org/)
  - For macOS: [MacTeX](https://www.tug.org/mactex/)
  - For Linux: `texlive` packages (`sudo apt-get install texlive-xetex texlive-fonts-recommended texlive-plain-generic`)

### Steps

1. Open a terminal/command prompt
2. Navigate to the docs directory containing `ICN_COMPUTE_COMMONS.md`
3. Run the following command:

```bash
pandoc ICN_COMPUTE_COMMONS.md \
  --pdf-engine=xelatex \
  --variable mainfont="DejaVu Serif" \
  --variable sansfont="DejaVu Sans" \
  --variable monofont="DejaVu Sans Mono" \
  --variable fontsize=11pt \
  --variable geometry="margin=1in" \
  --variable colorlinks=true \
  --toc \
  --highlight-style=tango \
  -o ICN_COMPUTE_COMMONS.pdf
```

### Notes for Pandoc
- The Mermaid diagram won't convert automatically. You'll need to:
  1. Use a tool like [Mermaid Live Editor](https://mermaid.live/) to render the diagram
  2. Export it as an image (PNG/SVG)
  3. Replace the Mermaid code block with the image in a temporary copy of the markdown file

## Option 2: Using Visual Studio Code

### Prerequisites
- Install [Visual Studio Code](https://code.visualstudio.com/)
- Install the "Markdown PDF" extension by yzane

### Steps
1. Open `ICN_COMPUTE_COMMONS.md` in VS Code
2. Press F1 to open the command palette
3. Type "Markdown PDF: Export (pdf)" and select it
4. The PDF will be generated in the same directory

### Notes for VS Code
- The extension has good default styling
- For the Mermaid diagram, you'll need to:
  1. Install the "Markdown Preview Mermaid Support" extension
  2. Consider using a two-step process where you first export to HTML (which renders the diagram), then print to PDF from a browser

## Option 3: Using Typora (Recommended for Best Results)

[Typora](https://typora.io/) is a Markdown editor that provides excellent PDF export functionality.

### Prerequisites
- Install [Typora](https://typora.io/#download)

### Steps
1. Open `ICN_COMPUTE_COMMONS.md` in Typora
2. Go to File > Export > PDF
3. In the export dialog, you can choose:
   - Page size and margins
   - Header/footer options
   - Theme settings

### Notes for Typora
- Typora renders Mermaid diagrams correctly in PDF exports
- You can customize CSS for PDF export in Typora's preferences
- For a branded document, you can create a custom theme

## Option 4: Online Converters

### Options
- [Markdown to PDF](https://www.markdowntopdf.com/)
- [CloudConvert](https://cloudconvert.com/md-to-pdf)
- [Dillinger](https://dillinger.io/) (edit and export to PDF)

### Steps
1. Copy the content of `ICN_COMPUTE_COMMONS.md`
2. Paste into the online tool
3. Configure options (if available)
4. Download the PDF

### Notes for Online Converters
- Mermaid diagram support varies by tool
- Security consideration: Avoid using these tools for sensitive/private content
- Quality and formatting options may be limited

## Styling Recommendations

For a polished, professional document:

1. **Cover Page**: Consider creating a separate cover page with:
   - Title "The ICN Compute Commons"
   - Subtitle "Federated Labor, Planetary Mesh, and the Reclamation of Digital Infrastructure"
   - ICN logo (if available)
   - Date and version information

2. **Typography**:
   - Use a serif font for body text (e.g., Linux Libertine, DejaVu Serif)
   - Use a sans-serif font for headings (e.g., Work Sans, DejaVu Sans)
   - Use sufficient line spacing (1.2-1.5)

3. **Color Scheme**:
   - Use limited color palette (2-3 colors max)
   - Consider using ICN brand colors for headings and accents

4. **Images and Diagrams**:
   - For the Mermaid diagram, export at high resolution (300dpi minimum)
   - Consider adding the ICN logo to the header or footer

5. **Headers and Footers**:
   - Page numbers in the footer
   - Consider adding "ICN Compute Commons" in the header
   - Add contact information in the footer of the last page 